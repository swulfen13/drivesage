use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    /// Rule-based only, no AI.
    #[default]
    None,
    Ollama,
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub provider: AiProvider,
    /// Model name, e.g. `qwen2.5:7b`, `gpt-4o-mini`, `claude-haiku-4-5`.
    pub model: String,
    /// Base URL. Defaults per provider; can point to any OpenAI-compatible server.
    pub base_url: Option<String>,
    /// API key for cloud providers. Stored locally in the config file.
    pub api_key: Option<String>,
    /// Allow sending short text snippets of file contents to the model.
    /// Off by default for cloud providers.
    pub send_snippets: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: AiProvider::None,
            model: String::new(),
            base_url: None,
            api_key: None,
            send_snippets: false,
        }
    }
}

impl AiConfig {
    pub fn effective_base_url(&self) -> String {
        if let Some(u) = self.base_url.as_ref().filter(|u| !u.trim().is_empty()) {
            return u.trim_end_matches('/').to_string();
        }
        match self.provider {
            AiProvider::Ollama => "http://127.0.0.1:11434".into(),
            AiProvider::OpenAi => "https://api.openai.com/v1".into(),
            AiProvider::Anthropic => "https://api.anthropic.com".into(),
            AiProvider::None => String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Set to true once the setup wizard has been completed.
    pub setup_done: bool,
    pub ai: AiConfig,
    /// Folders the user wants DriveSage to look at.
    pub scan_roots: Vec<PathBuf>,
    /// Extra folders the user excluded.
    pub excluded: Vec<PathBuf>,
    /// Files untouched for this many days count as "old".
    pub old_after_days: u32,
    /// Size threshold (MB) for the "large & old → slower drive" rule.
    pub large_file_mb: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            setup_done: false,
            ai: AiConfig::default(),
            scan_roots: default_scan_roots(),
            excluded: vec![],
            old_after_days: 180,
            large_file_mb: 500,
        }
    }
}

/// Typical user folders that are safe and useful to organise.
pub fn default_scan_roots() -> Vec<PathBuf> {
    [
        dirs::download_dir(),
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::picture_dir(),
        dirs::video_dir(),
    ]
    .into_iter()
    .flatten()
    .filter(|p| p.exists())
    .collect()
}

/// Directory for config, journals and caches.
pub fn data_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("DriveSage")
}

impl AppConfig {
    pub fn path() -> PathBuf {
        data_dir().join("config.json")
    }

    pub fn load() -> AppConfig {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Copy of the config that is safe to send to the UI (API key masked).
    pub fn redacted(&self) -> AppConfig {
        let mut c = self.clone();
        if let Some(k) = &c.ai.api_key {
            if !k.is_empty() {
                let tail: String = k
                    .chars()
                    .rev()
                    .take(4)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                c.ai.api_key = Some(format!("••••{tail}"));
            }
        }
        c
    }

    /// Merge an update from the UI. A masked key (starting with "••••") keeps the stored key.
    pub fn merge_from_ui(&mut self, mut incoming: AppConfig) {
        if let Some(k) = &incoming.ai.api_key {
            if k.starts_with("••••") {
                incoming.ai.api_key = self.ai.api_key.clone();
            }
        }
        *self = incoming;
    }
}
