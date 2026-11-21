//! Rule-based planner — works fully offline, no AI needed.

use crate::bench::BenchResult;
use crate::categories::Category;
use crate::drives::{drive_for, DriveInfo, DriveKind};
use crate::plan::{Finding, Plan};
use crate::protect::inside_project;
use crate::scan::{unix_now, ScanIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleOptions {
    /// Sort loose files directly inside the target folder into category folders.
    pub sort_by_type: bool,
    /// Suggest moving large, long-unused files from a fast to a slower drive.
    pub tier_to_slow_drive: bool,
    /// Report likely duplicates (information only, never auto-deleted).
    pub report_duplicates: bool,
    pub old_after_days: u32,
    pub large_file_mb: u64,
}

impl Default for RuleOptions {
    fn default() -> Self {
        Self {
            sort_by_type: true,
            tier_to_slow_drive: true,
            report_duplicates: true,
            old_after_days: 180,
            large_file_mb: 500,
        }
    }
}

/// Rough speed score of a drive: measured read speed if available, otherwise by kind.
pub fn speed_score(d: &DriveInfo, bench: &HashMap<String, BenchResult>) -> f64 {
    if let Some(b) = bench.get(&d.id) {
        return b.seq_read_mbps;
    }
    match (d.kind, d.removable) {
        (DriveKind::Ssd, _) => 1500.0,
        (DriveKind::Hdd, _) => 150.0,
        (DriveKind::Unknown, true) => 80.0,
        (DriveKind::Unknown, false) => 300.0,
    }
}

pub fn fmt_bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

pub fn build_plan(
    idx: &ScanIndex,
    target: &Path,
    opts: &RuleOptions,
    drives: &[DriveInfo],
    bench: &HashMap<String, BenchResult>,
) -> Plan {
    let mut plan = Plan::new(
        format!("Aufräumen: {}", target.display()),
        "rules",
        target.to_path_buf(),
    );
    let now = unix_now();
    let mut notes = Vec::new();

    // 1) Sort loose files by type
    if opts.sort_by_type {
        let mut n = 0;
        if let Some(list) = idx.files_by_dir.get(target) {
            for &i in list {
                let f = &idx.files[i];
                if f.protected || inside_project(&f.path, target) {
                    continue;
                }
                let cat = Category::of(&f.path);
                let Some(name) = f.path.file_name() else {
                    continue;
                };
                let to = target.join(cat.folder_name()).join(name);
                plan.push(
                    f.path.clone(),
                    to,
                    f.size,
                    format!("Lose Datei → {}", cat.folder_name()),
                    "sort_by_type",
                );
                n += 1;
            }
        }
        if n > 0 {
            notes.push(format!("{n} lose Dateien nach Typ in Ordner sortieren"));
        }
    }

    // 2) Large & long-unused files → slower drive with enough space
    if opts.tier_to_slow_drive {
        let min_size = opts.large_file_mb * 1024 * 1024;
        let mut by_src: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, f) in idx.files.iter().enumerate() {
            if !f.path.starts_with(target) || f.protected || f.size < min_size {
                continue;
            }
            if !f
                .age_days(now)
                .map(|d| d >= opts.old_after_days as i64)
                .unwrap_or(false)
            {
                continue;
            }
            if plan.actions.iter().any(|a| a.from == f.path) || inside_project(&f.path, target) {
                continue;
            }
            if let Some(d) = drive_for(&f.path, drives) {
                by_src.entry(d.id.clone()).or_default().push(i);
            }
        }
        for (src_id, files) in by_src {
            let Some(src) = drives.iter().find(|d| d.id == src_id) else {
                continue;
            };
            let need: u64 = files.iter().map(|&i| idx.files[i].size).sum();
            let src_speed = speed_score(src, bench);
            let slow = drives
                .iter()
                .filter(|d| d.id != src.id && !d.read_only && !d.removable)
                .filter(|d| speed_score(d, bench) < src_speed * 0.6)
                .filter(|d| d.available_bytes > need + need / 10)
                .max_by_key(|d| d.available_bytes);
            let Some(slow) = slow else { continue };
            for &i in &files {
                let f = &idx.files[i];
                let rel = f.path.strip_prefix(&src.mount_point).unwrap_or(&f.path);
                let to: PathBuf = slow.mount_point.join("DriveSage-Archiv").join(rel);
                let days = f.age_days(now).unwrap_or(0);
                plan.push(
                    f.path.clone(),
                    to,
                    f.size,
                    format!(
                        "{} seit {} Tagen ungenutzt → langsameres Laufwerk {}",
                        fmt_bytes(f.size),
                        days,
                        slow.mount_point.display()
                    ),
                    "tier_to_slow_drive",
                );
            }
            notes.push(format!(
                "{} große, alte Dateien ({}) von {} nach {} verschieben",
                files.len(),
                fmt_bytes(need),
                src.mount_point.display(),
                slow.mount_point.display()
            ));
        }
    }

    // 3) Duplicates — information only
    if opts.report_duplicates {
        for g in idx.find_duplicates(1024 * 1024, 3000) {
            if !g.paths.iter().any(|p| p.starts_with(target)) {
                continue;
            }
            plan.findings.push(Finding {
                kind: "duplicate".into(),
                title: format!("{} identische Kopien", g.paths.len()),
                detail: format!(
                    "Je {} — {} könnten frei werden, wenn du nur eine Kopie behältst.",
                    fmt_bytes(g.size),
                    fmt_bytes(g.wasted)
                ),
                bytes: g.wasted,
                paths: g.paths,
            });
        }
        if !plan.findings.is_empty() {
            notes.push(format!(
                "{} Gruppen möglicher Duplikate gefunden (werden nicht automatisch gelöscht)",
                plan.findings.len()
            ));
        }
    }

    plan.summary = if notes.is_empty() {
        "Hier ist schon alles ordentlich — keine Vorschläge.".into()
    } else {
        notes.join(" · ")
    };
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn sorts_loose_files() {
        let d = tempfile::Builder::new().prefix("dstest").tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let root = d.path().to_path_buf();
        std::fs::write(root.join("r.pdf"), b"1").unwrap();
        std::fs::write(root.join("p.png"), b"2").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/keep.pdf"), b"3").unwrap();
        let idx = scan(&[root.clone()], &[], &AtomicBool::new(false), &|_| {});
        let p = build_plan(&idx, &root, &RuleOptions::default(), &[], &HashMap::new());
        assert_eq!(p.actions.len(), 2);
        assert!(p
            .actions
            .iter()
            .any(|a| a.to == root.join("Dokumente/r.pdf")));
        assert!(p.actions.iter().any(|a| a.to == root.join("Bilder/p.png")));
    }
}
