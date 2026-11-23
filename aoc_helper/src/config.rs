use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub year: i32,
    pub days: u8,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default = "yes")]
    pub check_unlock: bool,
    pub urls: Urls,
    #[serde(default)]
    pub watch: Watch,
    #[serde(default)]
    pub run: Run,
    #[serde(default)]
    pub leaderboard: Leaderboard,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct Urls {
    pub base: String,
    pub input: String,
    pub puzzle: String,
    pub answer: String,
    pub leaderboard: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarnMode {
    Ask,
    Always,
    Never,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Watch {
    pub warnings: WarnMode,
    pub debounce_ms: u64,
    pub poll: bool,
    pub poll_interval_ms: u64,
}

impl Default for Watch {
    fn default() -> Self {
        Watch {
            warnings: WarnMode::Ask,
            debounce_ms: 300,
            poll: false,
            poll_interval_ms: 500,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Run {
    pub release: bool,
}

impl Default for Run {
    fn default() -> Self {
        Run { release: true }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Leaderboard {
    pub id: String,
    pub user_id: String,
    pub refresh_minutes: u64,
}

impl Default for Leaderboard {
    fn default() -> Self {
        Leaderboard {
            id: String::new(),
            user_id: String::new(),
            refresh_minutes: 15,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        if cfg.days == 0 || cfg.days > 25 {
            bail!("`days` in {} should be between 1 and 25", path.display());
        }
        Ok(cfg)
    }

    fn url(&self, path: &str, day: u8, id: &str) -> String {
        format!("{}{}", self.urls.base, path)
            .replace("{year}", &self.year.to_string())
            .replace("{day}", &day.to_string())
            .replace("{id}", id)
    }

    pub fn input_url(&self, day: u8) -> String {
        self.url(&self.urls.input, day, "")
    }
    pub fn puzzle_url(&self, day: u8) -> String {
        self.url(&self.urls.puzzle, day, "")
    }
    pub fn answer_url(&self, day: u8) -> String {
        self.url(&self.urls.answer, day, "")
    }
    pub fn leaderboard_url(&self) -> String {
        self.url(&self.urls.leaderboard, 0, &self.leaderboard.id)
    }
}

/// Change a single value in config.toml, keeping the user's comments and layout.
pub fn set_value(
    path: &PathBuf,
    table: Option<&str>,
    key: &str,
    value: toml_edit::Value,
) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut doc: toml_edit::DocumentMut = text.parse()?;
    match table {
        Some(t) => doc[t][key] = toml_edit::Item::Value(value),
        None => doc[key] = toml_edit::Item::Value(value),
    }
    fs::write(path, doc.to_string())?;
    Ok(())
}
