//! `aoc_helper/state.toml`: which day you're on and which days you've finished.
//! Contains no answers, so it's fine to commit.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub year: BTreeMap<String, YearState>,
    #[serde(skip)]
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YearState {
    pub current_day: u8,
    /// Days with both parts solved.
    #[serde(default)]
    pub completed: Vec<u8>,
    /// Days with only part 1 solved.
    #[serde(default)]
    pub part1_only: Vec<u8>,
}

impl Default for YearState {
    fn default() -> Self {
        YearState {
            current_day: 1,
            completed: vec![],
            part1_only: vec![],
        }
    }
}

impl YearState {
    /// Stars earned on a day: 0, 1 or 2.
    pub fn stars(&self, day: u8) -> u8 {
        if self.completed.contains(&day) {
            2
        } else if self.part1_only.contains(&day) {
            1
        } else {
            0
        }
    }

    pub fn total_stars(&self) -> u32 {
        self.completed.len() as u32 * 2 + self.part1_only.len() as u32
    }

    /// Record that `stars` (0-2) are earned on `day`. Never lowers a day's stars.
    pub fn set_stars(&mut self, day: u8, stars: u8) {
        if stars <= self.stars(day) {
            return;
        }
        self.part1_only.retain(|&d| d != day);
        self.completed.retain(|&d| d != day);
        match stars {
            1 => self.part1_only.push(day),
            2 => self.completed.push(day),
            _ => {}
        }
        self.part1_only.sort_unstable();
        self.completed.sort_unstable();
    }
}

const HEADER: &str = "\
# Progress tracked by `aoc` (current day and finished days, no answers).
# Safe to commit. Edited automatically, so comments here won't survive.

";

impl State {
    pub fn load(path: &Path) -> Result<State> {
        let mut state: State = match fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?
            }
            Err(_) => State::default(),
        };
        state.path = path.to_owned();
        Ok(state)
    }

    pub fn save(&self) -> Result<()> {
        fs::write(&self.path, format!("{HEADER}{}", toml::to_string(self)?))?;
        Ok(())
    }

    pub fn year(&mut self, year: i32) -> &mut YearState {
        self.year.entry(year.to_string()).or_default()
    }
}
