//! Everything a command needs: repo paths, config, progress state and the AoC client.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::{DateTime, TimeZone, Utc};

use crate::answers::Answers;
use crate::api::Client;
use crate::config::Config;
use crate::state::{State, YearState};
use crate::ui;

pub struct Ctx {
    pub root: PathBuf,
    pub helper_dir: PathBuf,
    pub config_path: PathBuf,
    pub cfg: Config,
    pub state: State,
    pub answers: Answers,
    client: Option<Client>,
}

impl Ctx {
    pub fn load() -> Result<Ctx> {
        let root = find_root()?;
        let helper_dir = root.join("aoc_helper");
        let config_path = helper_dir.join("config.toml");
        let cfg = Config::load(&config_path)?;
        let state = State::load(&helper_dir.join("state.toml"))?;
        let answers = Answers::load(&root.join("inputs").join("answers.toml"))?;
        Ok(Ctx {
            root,
            helper_dir,
            config_path,
            cfg,
            state,
            answers,
            client: None,
        })
    }

    pub fn reload_config(&mut self) -> Result<()> {
        self.cfg = Config::load(&self.config_path)?;
        Ok(())
    }

    pub fn year(&self) -> i32 {
        self.cfg.year
    }

    pub fn ys(&mut self) -> &mut YearState {
        let y = self.cfg.year;
        self.state.year(y)
    }

    pub fn current_day(&mut self) -> u8 {
        self.ys().current_day
    }

    /// The AoC client (created on first use) alongside the config it needs.
    pub fn api(&mut self) -> (&mut Client, &Config) {
        let client = self
            .client
            .get_or_insert_with(|| Client::new(&self.cfg, &self.helper_dir));
        (client, &self.cfg)
    }

    pub fn has_session(&mut self) -> bool {
        self.api().0.has_session()
    }

    pub fn inputs_dir(&self) -> PathBuf {
        self.root.join("inputs")
    }

    pub fn input_path(&self, day: u8) -> PathBuf {
        self.inputs_dir().join(format!("{day:02}.txt"))
    }

    pub fn day_file(&self, day: u8) -> PathBuf {
        self.root
            .join("src")
            .join("days")
            .join(format!("d{day:02}.rs"))
    }

    pub fn day_dir(&self, day: u8) -> PathBuf {
        self.root
            .join("src")
            .join("days")
            .join(format!("d{day:02}"))
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.helper_dir.join(".cache")
    }

    pub fn check_day(&self, day: u8) -> Result<()> {
        if day == 0 || day > self.cfg.days {
            bail!(
                "{} only has days 1-{} (see `days` in config.toml)",
                self.cfg.year,
                self.cfg.days
            );
        }
        Ok(())
    }

    pub fn unlock_time(&self, day: u8) -> DateTime<Utc> {
        // puzzles unlock at midnight US-Eastern (UTC-5) on each day of December
        Utc.with_ymd_and_hms(self.cfg.year, 12, day as u32, 5, 0, 0)
            .single()
            .expect("valid date")
    }

    pub fn is_unlocked(&self, day: u8) -> bool {
        !self.cfg.check_unlock || Utc::now() >= self.unlock_time(day)
    }

    pub fn ensure_unlocked(&self, day: u8) -> Result<()> {
        if !self.is_unlocked(day) {
            bail!(
                "day {day} of {} unlocks in {} (set check_unlock = false in config.toml to override)",
                self.cfg.year,
                ui::countdown(self.unlock_time(day) - Utc::now())
            );
        }
        Ok(())
    }

    pub fn unlocked_days(&self) -> Vec<u8> {
        (1..=self.cfg.days)
            .filter(|&d| self.is_unlocked(d))
            .collect()
    }

    /// Inputs are kept flat in `inputs/` so the solution crate can find them. If the
    /// configured year changes, move the old year's inputs aside so they're never
    /// mistaken for the new year's.
    pub fn sync_inputs_year(&self) -> Result<()> {
        let dir = self.inputs_dir();
        fs::create_dir_all(&dir)?;
        let marker = dir.join(".year");
        let year = self.cfg.year.to_string();
        let old = fs::read_to_string(&marker)
            .ok()
            .map(|s| s.trim().to_owned());
        if let Some(old) = old.filter(|o| *o != year) {
            let archive = dir.join(&old);
            let mut moved = 0;
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().is_some_and(|e| e == "txt") {
                    fs::create_dir_all(&archive)?;
                    fs::rename(&path, archive.join(path.file_name().unwrap()))?;
                    moved += 1;
                }
            }
            if moved > 0 {
                ui::note(&format!(
                    "moved {moved} input(s) from {old} into inputs/{old}/"
                ));
            }
        }
        fs::write(marker, year)?;
        Ok(())
    }
}

fn find_root() -> Result<PathBuf> {
    if let Ok(root) = env::var("AOC_ROOT") {
        return Ok(root.into());
    }
    let mut dir: &Path = &env::current_dir()?;
    loop {
        if dir.join("aoc_helper").join("config.toml").is_file() {
            return Ok(dir.to_owned());
        }
        if let Some(stripped) = dir
            .file_name()
            .filter(|n| *n == "aoc_helper")
            .and(dir.parent())
            && stripped.join("aoc_helper").join("config.toml").is_file()
        {
            return Ok(stripped.to_owned());
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => bail!(
                "couldn't find the repo (no aoc_helper/config.toml here or above; set AOC_ROOT?)"
            ),
        }
    }
}
