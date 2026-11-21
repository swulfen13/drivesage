//! AI planner: Ollama (local), OpenAI(-compatible) or Anthropic Claude.
//!
//! Privacy: by default only file *names*, sizes and dates are sent.
//! Short text snippets are only sent when `send_snippets` is enabled.
//! The model's answer is strictly validated: it may only move files that we
//! listed, and only to relative paths inside the chosen folder.

use crate::config::{AiConfig, AiProvider};
use crate::plan::Plan;
use crate::protect::inside_project;
use crate::rules::fmt_bytes;
use crate::scan::ScanIndex;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const MAX_FILES: usize = 400;

const SYSTEM_PROMPT: &str = r#"You are DriveSage, a careful file-organisation assistant.
You receive a list of files inside ONE root folder plus its existing sub-folders.
Propose a clean, simple folder structure and the moves to get there.

Rules:
- "from" must be copied exactly from the file list (relative path).
- "to" is the new relative path INSIDE the root, including the file name. No "..", no absolute paths, no drive letters.
- Keep the original file name unless it is clearly meaningless; then you may rename but keep the extension.
- Leave files that already sit in a sensible folder where they are (just omit them).
- Group by topic / project / document type / year when it is obvious (e.g. "Rechnungen/2024", "Bewerbungen", "Screenshots", "Uni/Semester 3", "Installer").
- Prefer few, clear top-level folders. Re-use existing folders when they fit.
- Name folders in the language the file names are written in.
- "reason": short German explanation, max 12 words.
- At most 200 moves.

Answer with JSON only, no markdown:
{"summary": "<2 sentences in German about the proposed structure>", "moves": [{"from": "...", "to": "...", "reason": "..."}]}"#;

#[derive(Debug, Deserialize)]
struct AiAnswer {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    moves: Vec<AiMove>,
}

#[derive(Debug, Deserialize)]
struct AiMove {
    from: String,
    to: String,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
}

pub struct AiClient {
    cfg: AiConfig,
    http: reqwest::Client,
}

impl AiClient {
    pub fn new(cfg: AiConfig) -> Result<Self> {
        if cfg.provider == AiProvider::None {
            bail!("Kein KI-Anbieter eingerichtet");
        }
        if cfg.model.trim().is_empty() {
            bail!("Kein Modell ausgewählt");
        }
        if matches!(cfg.provider, AiProvider::OpenAi | AiProvider::Anthropic)
            && cfg.api_key.as_deref().unwrap_or("").is_empty()
        {
            bail!("API-Key fehlt");
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;
        Ok(Self { cfg, http })
    }

    pub fn label(&self) -> String {
        let p = match self.cfg.provider {
            AiProvider::Ollama => "ollama",
            AiProvider::OpenAi => "openai",
            AiProvider::Anthropic => "anthropic",
            AiProvider::None => "none",
        };
        format!("ai:{p}/{}", self.cfg.model)
    }

    /// Send a system + user prompt, expect a JSON object as text.
    pub async fn chat_json(&self, system: &str, user: &str) -> Result<String> {
        let base = self.cfg.effective_base_url();
        let key = self.cfg.api_key.clone().unwrap_or_default();
        let model = &self.cfg.model;
        match self.cfg.provider {
            AiProvider::Ollama => {
                let body = json!({
                    "model": model, "stream": false, "format": "json",
                    "options": {"temperature": 0.2, "num_ctx": 16384},
                    "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]
                });
                let v: serde_json::Value = self
                    .http
                    .post(format!("{base}/api/chat"))
                    .json(&body)
                    .send()
                    .await
                    .context("Ollama nicht erreichbar — läuft Ollama?")?
                    .error_for_status()?
                    .json()
                    .await?;
                v["message"]["content"]
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("Leere Antwort von Ollama"))
            }
            AiProvider::OpenAi => {
                let body = json!({
                    "model": model,
                    "response_format": {"type": "json_object"},
                    "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]
                });
                let resp = self
                    .http
                    .post(format!("{base}/chat/completions"))
                    .bearer_auth(key)
                    .json(&body)
                    .send()
                    .await?;
                let status = resp.status();
                let v: serde_json::Value = resp.json().await?;
                if !status.is_success() {
                    bail!(
                        "OpenAI-Fehler {status}: {}",
                        v["error"]["message"].as_str().unwrap_or("unbekannt")
                    );
                }
                v["choices"][0]["message"]["content"]
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("Leere Antwort"))
            }
            AiProvider::Anthropic => {
                let body = json!({
                    "model": model, "max_tokens": 8000, "system": system,
                    "messages": [{"role": "user", "content": user}]
                });
                let resp = self
                    .http
                    .post(format!("{base}/v1/messages"))
                    .header("x-api-key", key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&body)
                    .send()
                    .await?;
                let status = resp.status();
                let v: serde_json::Value = resp.json().await?;
                if !status.is_success() {
                    bail!(
                        "Claude-Fehler {status}: {}",
                        v["error"]["message"].as_str().unwrap_or("unbekannt")
                    );
                }
                v["content"][0]["text"]
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("Leere Antwort"))
            }
            AiProvider::None => bail!("Kein KI-Anbieter"),
        }
    }

    /// Quick connectivity test used by the setup wizard.
    pub async fn test(&self) -> Result<String> {
        let out = self
            .chat_json(
                "Answer with JSON only.",
                r#"Reply with {"ok": true, "hello": "<one short German greeting>"}"#,
            )
            .await?;
        let v: serde_json::Value =
            serde_json::from_str(extract_json(&out)).unwrap_or(json!({"hello": out}));
        Ok(v["hello"].as_str().unwrap_or("ok").to_string())
    }

    /// Ask the model for a plan for `target`, validate it and turn it into a [`Plan`].
    pub async fn build_plan(&self, idx: &ScanIndex, target: &Path) -> Result<Plan> {
        let (prompt, allowed) = build_prompt(idx, target, self.cfg.send_snippets);
        if allowed.is_empty() {
            bail!("In diesem Ordner gibt es keine Dateien, die DriveSage verschieben darf");
        }
        let raw = self.chat_json(SYSTEM_PROMPT, &prompt).await?;
        let mut plan = Plan::new(
            format!("KI-Vorschlag: {}", target.display()),
            self.label(),
            target.to_path_buf(),
        );
        let (summary, rejected) = apply_answer(&mut plan, &raw, target, &allowed)?;
        plan.summary = if rejected > 0 {
            format!("{summary} ({rejected} ungültige Vorschläge der KI wurden verworfen.)")
        } else {
            summary
        };
        Ok(plan)
    }
}

pub async fn list_ollama_models(base_url: &str) -> Result<Vec<OllamaModel>> {
    let v: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/api/tags", base_url.trim_end_matches('/')))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .context("Ollama nicht erreichbar")?
        .json()
        .await?;
    Ok(v["models"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|m| OllamaModel {
            name: m["name"].as_str().unwrap_or("").into(),
            size: m["size"].as_u64().unwrap_or(0),
        })
        .collect())
}

/// Download a model through Ollama (blocking until done).
pub async fn ollama_pull(base_url: &str, model: &str) -> Result<()> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()?
        .post(format!("{}/api/pull", base_url.trim_end_matches('/')))
        .json(&json!({"model": model, "stream": false}))
        .send()
        .await
        .context("Ollama nicht erreichbar")?
        .error_for_status()?;
    Ok(())
}

fn snippet(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "txt" | "md" | "csv" | "log" | "json" | "xml" | "html" | "ini"
    ) {
        return None;
    }
    let mut buf = [0u8; 240];
    let n = std::fs::File::open(path).ok()?.read(&mut buf).ok()?;
    let s = String::from_utf8_lossy(&buf[..n]).replace(['\n', '\r', '\t'], " ");
    Some(s.trim().to_string())
}

/// Build the user prompt and the set of files the model is allowed to move.
pub fn build_prompt(
    idx: &ScanIndex,
    target: &Path,
    snippets: bool,
) -> (String, HashMap<String, (PathBuf, u64)>) {
    let mut files: Vec<_> = idx
        .files
        .iter()
        .filter(|f| f.path.starts_with(target) && !f.protected && !inside_project(&f.path, target))
        .collect();
    // shallow files first (most likely to be messy), then by size
    files.sort_by_key(|f| (f.path.components().count(), std::cmp::Reverse(f.size)));
    files.truncate(MAX_FILES);

    let mut allowed = HashMap::new();
    let mut lines = Vec::new();
    for f in files {
        let rel = f
            .path
            .strip_prefix(target)
            .unwrap_or(&f.path)
            .to_string_lossy()
            .replace('\\', "/");
        let date = f
            .modified
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let mut line = format!("- {rel} | {} | {date}", fmt_bytes(f.size));
        if snippets {
            if let Some(s) = snippet(&f.path) {
                line.push_str(&format!(" | \"{s}\""));
            }
        }
        lines.push(line);
        allowed.insert(rel, (f.path.clone(), f.size));
    }

    let mut folders: Vec<String> = idx
        .dirs
        .keys()
        .filter(|p| p.starts_with(target) && p.as_path() != target)
        .filter(|p| p.components().count() - target.components().count() <= 2)
        .map(|p| {
            p.strip_prefix(target)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    folders.sort();
    folders.truncate(150);

    let prompt = format!(
        "Root folder: {}\n\nExisting sub-folders:\n{}\n\nFiles (relative path | size | last modified{}):\n{}",
        target.display(),
        if folders.is_empty() { "(none)".into() } else { folders.iter().map(|f| format!("- {f}/")).collect::<Vec<_>>().join("\n") },
        if snippets { " | text preview" } else { "" },
        lines.join("\n")
    );
    (prompt, allowed)
}

/// Strip markdown fences / chatter around a JSON object.
pub fn extract_json(s: &str) -> &str {
    match (s.find('{'), s.rfind('}')) {
        (Some(a), Some(b)) if b > a => &s[a..=b],
        _ => s,
    }
}

fn safe_relative(p: &str) -> Option<PathBuf> {
    let p = p.trim().replace('\\', "/");
    if p.is_empty() || p.contains(':') || p.starts_with('/') {
        return None;
    }
    let pb = PathBuf::from(&p);
    if pb.components().all(|c| matches!(c, Component::Normal(_))) {
        Some(pb)
    } else {
        None
    }
}

/// Validate the model answer and add the accepted moves to `plan`.
/// Returns (summary, number of rejected moves).
pub fn apply_answer(
    plan: &mut Plan,
    raw: &str,
    target: &Path,
    allowed: &HashMap<String, (PathBuf, u64)>,
) -> Result<(String, usize)> {
    let ans: AiAnswer = serde_json::from_str(extract_json(raw))
        .context("Die KI hat kein gültiges JSON geliefert")?;
    let mut rejected = 0;
    let mut used_from = HashSet::new();
    let mut used_to = HashSet::new();
    for m in ans.moves {
        let from_key = m.from.trim().trim_start_matches("./").replace('\\', "/");
        let Some((from_abs, size)) = allowed.get(&from_key) else {
            rejected += 1;
            continue;
        };
        let Some(to_rel) = safe_relative(&m.to) else {
            rejected += 1;
            continue;
        };
        let to_abs = target.join(&to_rel);
        // keep the original extension
        if from_abs.extension() != to_abs.extension() {
            rejected += 1;
            continue;
        }
        if to_abs == *from_abs
            || !used_from.insert(from_abs.clone())
            || !used_to.insert(to_abs.clone())
        {
            rejected += 1;
            continue;
        }
        let reason = if m.reason.trim().is_empty() {
            "KI-Vorschlag".to_string()
        } else {
            m.reason.trim().to_string()
        };
        plan.push(from_abs.clone(), to_abs, *size, reason, "ai");
    }
    Ok((ans.summary, rejected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ai_answer() {
        let target = PathBuf::from("/home/u/Downloads");
        let mut allowed = HashMap::new();
        allowed.insert(
            "rechnung.pdf".to_string(),
            (target.join("rechnung.pdf"), 10),
        );
        allowed.insert("foto.jpg".to_string(), (target.join("foto.jpg"), 20));
        let raw = r#"```json
        {"summary":"ok","moves":[
          {"from":"rechnung.pdf","to":"Rechnungen/2024/rechnung.pdf","reason":"Rechnung"},
          {"from":"foto.jpg","to":"../../etc/foto.jpg","reason":"evil"},
          {"from":"notlisted.txt","to":"x/notlisted.txt","reason":"?"},
          {"from":"foto.jpg","to":"C:/Windows/foto.jpg","reason":"evil"},
          {"from":"foto.jpg","to":"Bilder/foto.exe","reason":"ext change"}
        ]}
        ```"#;
        let mut plan = Plan::new("t", "ai", target.clone());
        let (s, rej) = apply_answer(&mut plan, raw, &target, &allowed).unwrap();
        assert_eq!(s, "ok");
        assert_eq!(rej, 4);
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(
            plan.actions[0].to,
            target.join("Rechnungen/2024/rechnung.pdf")
        );
    }
}
