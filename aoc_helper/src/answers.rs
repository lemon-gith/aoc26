//! `inputs/answers.toml`: the answers AoC accepted (and rejected) for *your* inputs.
//! Lives next to the inputs, so it's gitignored along with them. It's used to
//! check `aoc run` results and to avoid resubmitting answers that were already wrong.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Answers {
    /// year -> day -> answers
    #[serde(default)]
    pub year: BTreeMap<String, BTreeMap<String, DayAnswers>>,
    #[serde(skip)]
    path: PathBuf,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DayAnswers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part2: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrong1: Vec<Wrong>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrong2: Vec<Wrong>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wrong {
    pub answer: String,
    /// "too high" / "too low", when AoC said so
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

pub enum Check {
    /// matches the accepted answer
    Correct,
    /// differs from the accepted answer
    Incorrect(String),
    /// no accepted answer yet, and nothing says this one's wrong
    Unknown,
    /// already rejected, or ruled out by an earlier "too high/low"
    KnownWrong(String),
}

impl DayAnswers {
    pub fn get(&self, part: u8) -> Option<&String> {
        if part == 1 {
            self.part1.as_ref()
        } else {
            self.part2.as_ref()
        }
    }

    fn wrong(&self, part: u8) -> &Vec<Wrong> {
        if part == 1 {
            &self.wrong1
        } else {
            &self.wrong2
        }
    }

    pub fn check(&self, part: u8, answer: &str) -> Check {
        if let Some(correct) = self.get(part) {
            return if correct == answer {
                Check::Correct
            } else {
                Check::Incorrect(correct.clone())
            };
        }
        let wrong = self.wrong(part);
        if let Some(w) = wrong.iter().find(|w| w.answer == answer) {
            let hint = w
                .hint
                .as_deref()
                .map(|h| format!(" as {h}"))
                .unwrap_or_default();
            return Check::KnownWrong(format!("already rejected{hint}"));
        }
        // use earlier "too high" / "too low" hints as bounds for numeric answers
        if let Ok(n) = answer.parse::<i128>() {
            for w in wrong {
                let Ok(m) = w.answer.parse::<i128>() else {
                    continue;
                };
                match w.hint.as_deref() {
                    Some("too high") if n >= m => {
                        return Check::KnownWrong(format!("{m} was already too high"));
                    }
                    Some("too low") if n <= m => {
                        return Check::KnownWrong(format!("{m} was already too low"));
                    }
                    _ => {}
                }
            }
        }
        Check::Unknown
    }
}

const HEADER: &str = "\
# Answers AoC accepted/rejected for YOUR inputs. Gitignored along with the inputs.
# Edited automatically by `aoc`.

";

impl Answers {
    pub fn load(path: &Path) -> Result<Answers> {
        let mut answers: Answers = match fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?
            }
            Err(_) => Answers::default(),
        };
        answers.path = path.to_owned();
        Ok(answers)
    }

    pub fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.path, format!("{HEADER}{}", toml::to_string(self)?))?;
        Ok(())
    }

    pub fn day(&self, year: i32, day: u8) -> DayAnswers {
        self.year
            .get(&year.to_string())
            .and_then(|y| y.get(&day.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    fn day_mut(&mut self, year: i32, day: u8) -> &mut DayAnswers {
        self.year
            .entry(year.to_string())
            .or_default()
            .entry(day.to_string())
            .or_default()
    }

    pub fn set_correct(&mut self, year: i32, day: u8, part: u8, answer: &str) {
        let d = self.day_mut(year, day);
        if part == 1 {
            d.part1 = Some(answer.to_owned());
        } else {
            d.part2 = Some(answer.to_owned());
        }
    }

    pub fn add_wrong(&mut self, year: i32, day: u8, part: u8, answer: &str, hint: Option<String>) {
        let d = self.day_mut(year, day);
        let list = if part == 1 {
            &mut d.wrong1
        } else {
            &mut d.wrong2
        };
        if !list.iter().any(|w| w.answer == answer) {
            list.push(Wrong {
                answer: answer.to_owned(),
                hint,
            });
        }
    }
}
