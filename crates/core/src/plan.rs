//! Plans (proposed moves), safe execution with a journal, and undo.

use crate::protect::{inside_project, is_protected};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: u32,
    pub from: PathBuf,
    pub to: PathBuf,
    pub size: u64,
    /// Human readable reason ("PDF → Dokumente", "6 Monate ungenutzt, 2 GB → HDD").
    pub reason: String,
    /// Which rule / engine created it ("sort_by_type", "tier_to_slow_drive", "ai").
    pub rule: String,
    pub enabled: bool,
}

/// Information without an automatic action (e.g. duplicates — never auto-deleted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub bytes: u64,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub title: String,
    /// "rules" or "ai:<provider>/<model>"
    pub source: String,
    pub target_root: PathBuf,
    pub actions: Vec<Action>,
    pub findings: Vec<Finding>,
    /// Free-text explanation (e.g. from the AI).
    pub summary: String,
}

impl Plan {
    pub fn new(title: impl Into<String>, source: impl Into<String>, target_root: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            title: title.into(),
            source: source.into(),
            target_root,
            actions: vec![],
            findings: vec![],
            summary: String::new(),
        }
    }

    pub fn push(
        &mut self,
        from: PathBuf,
        to: PathBuf,
        size: u64,
        reason: impl Into<String>,
        rule: &str,
    ) {
        let id = self.actions.len() as u32 + 1;
        self.actions.push(Action {
            id,
            from,
            to,
            size,
            reason: reason.into(),
            rule: rule.into(),
            enabled: true,
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Ok,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub action_id: u32,
    pub from: PathBuf,
    pub to: PathBuf,
    pub status: StepStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub run_id: Uuid,
    pub plan_id: Uuid,
    pub plan_title: String,
    pub dry_run: bool,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub moved: u32,
    pub skipped: u32,
    pub failed: u32,
    pub bytes_moved: u64,
    pub steps: Vec<StepResult>,
    pub undone: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum JournalLine {
    Mkdir { path: PathBuf },
    Move { from: PathBuf, to: PathBuf },
}

fn has_parent_refs(p: &Path) -> bool {
    p.components().any(|c| matches!(c, Component::ParentDir))
}

/// Check a single action. Returns a reason string if it must be skipped.
pub fn validate(a: &Action) -> Result<(), String> {
    if !a.from.is_absolute() || !a.to.is_absolute() {
        return Err("Pfad ist nicht absolut".into());
    }
    if has_parent_refs(&a.from) || has_parent_refs(&a.to) {
        return Err("Pfad enthält '..'".into());
    }
    if a.from == a.to {
        return Err("Quelle und Ziel sind identisch".into());
    }
    let md =
        fs::symlink_metadata(&a.from).map_err(|_| "Quelldatei existiert nicht mehr".to_string())?;
    if !md.is_file() {
        return Err("Quelle ist keine normale Datei".into());
    }
    if is_protected(&a.from) {
        return Err("Quelle liegt in einem geschützten Bereich".into());
    }
    if is_protected(&a.to) {
        return Err("Ziel liegt in einem geschützten Bereich".into());
    }
    let root = a.from.ancestors().last().unwrap_or(Path::new("/"));
    if inside_project(&a.from, root) {
        return Err("Datei gehört zu einem Code-Projekt".into());
    }
    Ok(())
}

/// If `p` exists, append " (1)", " (2)" … before the extension.
fn unique_path(p: &Path) -> PathBuf {
    if !p.exists() {
        return p.to_path_buf();
    }
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = p.parent().unwrap_or(Path::new(""));
    for i in 1..10_000 {
        let c = parent.join(format!("{stem} ({i}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    parent.join(format!("{stem} ({}){ext}", Uuid::new_v4()))
}

/// Move a file, falling back to copy + verify + remove across drives.
fn move_file(from: &Path, to: &Path) -> Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let src_len = fs::metadata(from)?.len();
    fs::copy(from, to).with_context(|| format!("Kopieren nach {} fehlgeschlagen", to.display()))?;
    let dst_len = fs::metadata(to)?.len();
    if dst_len != src_len {
        let _ = fs::remove_file(to);
        bail!("Kopie unvollständig ({dst_len} von {src_len} Bytes)");
    }
    if let Err(e) = fs::remove_file(from) {
        let _ = fs::remove_file(to);
        bail!("Original konnte nicht entfernt werden: {e}");
    }
    Ok(())
}

fn runs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("runs")
}

/// Execute (or dry-run) all enabled actions of a plan. Never deletes anything.
pub fn execute(plan: &Plan, dry_run: bool, data_dir: &Path) -> Result<RunReport> {
    let run_id = Uuid::new_v4();
    let started_at = Utc::now();
    let dir = runs_dir(data_dir);
    fs::create_dir_all(&dir)?;
    let mut journal = if dry_run {
        None
    } else {
        Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(format!("{run_id}.jsonl")))?,
        )
    };
    let mut log = |line: JournalLine| -> Result<()> {
        if let Some(j) = journal.as_mut() {
            writeln!(j, "{}", serde_json::to_string(&line)?)?;
            j.flush()?;
        }
        Ok(())
    };

    let mut steps = Vec::new();
    let (mut moved, mut skipped, mut failed, mut bytes) = (0, 0, 0, 0u64);
    for a in plan.actions.iter().filter(|a| a.enabled) {
        if let Err(reason) = validate(a) {
            skipped += 1;
            steps.push(StepResult {
                action_id: a.id,
                from: a.from.clone(),
                to: a.to.clone(),
                status: StepStatus::Skipped,
                message: reason,
            });
            continue;
        }
        let dest = unique_path(&a.to);
        if dry_run {
            moved += 1;
            bytes += a.size;
            let msg = if dest != a.to {
                format!("Ziel existiert, würde umbenannt: {}", dest.display())
            } else {
                "würde verschoben".into()
            };
            steps.push(StepResult {
                action_id: a.id,
                from: a.from.clone(),
                to: dest,
                status: StepStatus::Ok,
                message: msg,
            });
            continue;
        }
        // create missing parent folders (journaled, so undo can remove them if empty)
        let result = (|| -> Result<()> {
            if let Some(parent) = dest.parent() {
                let mut missing = Vec::new();
                let mut cur = Some(parent);
                while let Some(c) = cur {
                    if c.exists() {
                        break;
                    }
                    missing.push(c.to_path_buf());
                    cur = c.parent();
                }
                for m in missing.into_iter().rev() {
                    fs::create_dir(&m)?;
                    log(JournalLine::Mkdir { path: m })?;
                }
            }
            move_file(&a.from, &dest)?;
            log(JournalLine::Move {
                from: a.from.clone(),
                to: dest.clone(),
            })?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                moved += 1;
                bytes += a.size;
                steps.push(StepResult {
                    action_id: a.id,
                    from: a.from.clone(),
                    to: dest,
                    status: StepStatus::Ok,
                    message: "verschoben".into(),
                });
            }
            Err(e) => {
                failed += 1;
                steps.push(StepResult {
                    action_id: a.id,
                    from: a.from.clone(),
                    to: dest,
                    status: StepStatus::Failed,
                    message: e.to_string(),
                });
            }
        }
    }

    let report = RunReport {
        run_id,
        plan_id: plan.id,
        plan_title: plan.title.clone(),
        dry_run,
        started_at,
        finished_at: Utc::now(),
        moved,
        skipped,
        failed,
        bytes_moved: bytes,
        steps,
        undone: false,
    };
    if !dry_run {
        fs::write(
            dir.join(format!("{run_id}.report.json")),
            serde_json::to_string_pretty(&report)?,
        )?;
    }
    Ok(report)
}

/// Undo a run: move every file back (newest first) and remove folders we created, if empty.
pub fn undo(run_id: Uuid, data_dir: &Path) -> Result<RunReport> {
    let dir = runs_dir(data_dir);
    let report_path = dir.join(format!("{run_id}.report.json"));
    let mut report: RunReport =
        serde_json::from_str(&fs::read_to_string(&report_path).context("Lauf nicht gefunden")?)?;
    if report.undone {
        bail!("Dieser Lauf wurde bereits rückgängig gemacht");
    }
    let f = fs::File::open(dir.join(format!("{run_id}.jsonl"))).context("Journal fehlt")?;
    let lines: Vec<JournalLine> = BufReader::new(f)
        .lines()
        .map_while(|l| l.ok())
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect();

    let mut steps = Vec::new();
    let (mut ok, mut failed) = (0, 0);
    for line in lines.into_iter().rev() {
        match line {
            JournalLine::Move { from, to } => {
                let res = if !to.exists() {
                    Err(anyhow::anyhow!("Datei ist nicht mehr am Zielort"))
                } else if from.exists() {
                    Err(anyhow::anyhow!(
                        "Am Ursprungsort liegt inzwischen eine andere Datei"
                    ))
                } else {
                    if let Some(p) = from.parent() {
                        let _ = fs::create_dir_all(p);
                    }
                    move_file(&to, &from)
                };
                match res {
                    Ok(()) => {
                        ok += 1;
                        steps.push(StepResult {
                            action_id: 0,
                            from: to,
                            to: from,
                            status: StepStatus::Ok,
                            message: "zurückverschoben".into(),
                        });
                    }
                    Err(e) => {
                        failed += 1;
                        steps.push(StepResult {
                            action_id: 0,
                            from: to,
                            to: from,
                            status: StepStatus::Failed,
                            message: e.to_string(),
                        });
                    }
                }
            }
            JournalLine::Mkdir { path } => {
                // remove_dir only succeeds on empty folders — user files are never deleted
                let _ = fs::remove_dir(&path);
            }
        }
    }
    report.undone = true;
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    Ok(RunReport {
        run_id,
        plan_id: report.plan_id,
        plan_title: format!("Rückgängig: {}", report.plan_title),
        dry_run: false,
        started_at: Utc::now(),
        finished_at: Utc::now(),
        moved: ok,
        skipped: 0,
        failed,
        bytes_moved: 0,
        steps,
        undone: true,
    })
}

/// All previous (non-dry) runs, newest first, without step details.
pub fn list_runs(data_dir: &Path) -> Vec<RunReport> {
    let mut out: Vec<RunReport> = fs::read_dir(runs_dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".report.json"))
        .filter_map(|e| serde_json::from_str::<RunReport>(&fs::read_to_string(e.path()).ok()?).ok())
        .map(|mut r| {
            r.steps.clear();
            r
        })
        .collect();
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_and_undo_roundtrip() {
        let d = tempfile::Builder::new().prefix("dstest").tempdir().unwrap();
        let root = d.path().join("dl");
        let data = d.path().join("data");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.pdf"), b"pdf").unwrap();
        fs::write(root.join("b.pdf"), b"pdf2").unwrap();
        fs::create_dir_all(root.join("Dokumente")).unwrap();
        fs::write(root.join("Dokumente/b.pdf"), b"existing").unwrap();

        let mut plan = Plan::new("t", "rules", root.clone());
        plan.push(
            root.join("a.pdf"),
            root.join("Dokumente/Neu/a.pdf"),
            3,
            "x",
            "t",
        );
        plan.push(
            root.join("b.pdf"),
            root.join("Dokumente/b.pdf"),
            4,
            "x",
            "t",
        );
        plan.push(
            root.join("missing.pdf"),
            root.join("x/missing.pdf"),
            1,
            "x",
            "t",
        );

        let dry = execute(&plan, true, &data).unwrap();
        assert_eq!(dry.moved, 2);
        assert!(root.join("a.pdf").exists());

        let r = execute(&plan, false, &data).unwrap();
        assert_eq!((r.moved, r.skipped, r.failed), (2, 1, 0));
        assert!(root.join("Dokumente/Neu/a.pdf").exists());
        assert!(
            root.join("Dokumente/b (1).pdf").exists(),
            "collision renamed"
        );
        assert_eq!(fs::read(root.join("Dokumente/b.pdf")).unwrap(), b"existing");

        let u = undo(r.run_id, &data).unwrap();
        assert_eq!(u.moved, 2);
        assert!(root.join("a.pdf").exists() && root.join("b.pdf").exists());
        assert!(
            !root.join("Dokumente/Neu").exists(),
            "created folder removed"
        );
        assert!(
            root.join("Dokumente/b.pdf").exists(),
            "pre-existing file untouched"
        );
        assert!(undo(r.run_id, &data).is_err());
        assert_eq!(list_runs(&data).len(), 1);
    }

    #[test]
    fn rejects_dangerous_actions() {
        let a = Action {
            id: 1,
            from: "/etc/passwd".into(),
            to: "/tmp/x".into(),
            size: 0,
            reason: String::new(),
            rule: String::new(),
            enabled: true,
        };
        assert!(validate(&a).is_err());
        let b = Action {
            from: "/tmp/../etc/x".into(),
            ..a.clone()
        };
        assert!(validate(&b).is_err());
    }
}
