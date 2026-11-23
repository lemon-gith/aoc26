//! Talking to adventofcode.com. There's no official API apart from the private
//! leaderboard JSON, so submissions and puzzle pages are parsed from HTML.
//!
//! Following the AoC automation guidelines: inputs are cached and never re-downloaded,
//! requests are spaced out, puzzles aren't fetched before they unlock, the leaderboard
//! is fetched at most every 15 minutes, and the User-Agent comes from config.toml.

use std::path::Path;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};
use std::{env, fs};

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use ureq::Agent;

use crate::config::Config;

const MIN_GAP: Duration = Duration::from_millis(1500);

pub struct Client {
    agent: Agent,
    session: Option<String>,
    last_request: Option<Instant>,
}

pub enum Submission {
    Correct,
    Wrong { hint: Option<String> },
    TooSoon { wait: String },
    AlreadySolved,
    Unknown(String),
}

/// What a puzzle page tells us about your progress on that day.
pub struct PuzzlePage {
    /// accepted answers, in part order ("Your puzzle answer was ...")
    pub answers: Vec<String>,
}

impl Client {
    pub fn new(cfg: &Config, helper_dir: &Path) -> Client {
        let ua = if cfg.user_agent.trim().is_empty() {
            concat!("aoc_helper/", env!("CARGO_PKG_VERSION")).to_owned()
        } else {
            cfg.user_agent.trim().to_owned()
        };
        let config = Agent::config_builder()
            .user_agent(ua)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Client {
            agent: Agent::new_with_config(config),
            session: load_session(helper_dir),
            last_request: None,
        }
    }

    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    fn session(&self) -> Result<&str> {
        self.session.as_deref().ok_or_else(|| {
            anyhow!("no AoC session cookie: copy aoc_helper/.env.example to aoc_helper/.env and fill in AOC_SESSION")
        })
    }

    fn pace(&mut self) {
        if let Some(last) = self.last_request {
            let since = last.elapsed();
            if since < MIN_GAP {
                thread::sleep(MIN_GAP - since);
            }
        }
        self.last_request = Some(Instant::now());
    }

    fn get(&mut self, url: &str) -> Result<(u16, String)> {
        let cookie = format!("session={}", self.session()?);
        self.pace();
        let mut resp = self
            .agent
            .get(url)
            .header("Cookie", &cookie)
            .call()
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .read_to_string()
            .with_context(|| format!("reading {url}"))?;
        Ok((status, body))
    }

    pub fn fetch_input(&mut self, cfg: &Config, day: u8) -> Result<String> {
        let url = cfg.input_url(day);
        let (status, body) = self.get(&url)?;
        match status {
            200 => Ok(body),
            400 | 500 => bail!(
                "AoC rejected the session cookie (HTTP {status}); it may have expired, so grab a fresh one"
            ),
            404 => bail!("{url} doesn't exist (yet?)"),
            s => bail!("HTTP {s} from {url}: {}", body.trim()),
        }
    }

    pub fn fetch_puzzle(&mut self, cfg: &Config, day: u8) -> Result<PuzzlePage> {
        let url = cfg.puzzle_url(day);
        let (status, body) = self.get(&url)?;
        if status != 200 {
            bail!("HTTP {status} from {url}");
        }
        if !body.contains("[Log Out]")
            && !body.contains("/auth/logout")
            && body.contains("/auth/login")
        {
            bail!("AoC doesn't think you're logged in; the session cookie may have expired");
        }
        static RE: OnceLock<Regex> = OnceLock::new();
        let re =
            RE.get_or_init(|| Regex::new(r"Your puzzle answer was <code>([^<]*)</code>").unwrap());
        let answers = re.captures_iter(&body).map(|c| unescape(&c[1])).collect();
        Ok(PuzzlePage { answers })
    }

    pub fn submit(&mut self, cfg: &Config, day: u8, part: u8, answer: &str) -> Result<Submission> {
        let url = cfg.answer_url(day);
        let cookie = format!("session={}", self.session()?);
        self.pace();
        let level = part.to_string();
        let mut resp = self
            .agent
            .post(&url)
            .header("Cookie", &cookie)
            .send_form([("level", level.as_str()), ("answer", answer)])
            .with_context(|| format!("POST {url}"))?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string()?;
        if status != 200 {
            bail!("HTTP {status} from {url}");
        }
        Ok(parse_submission(&body))
    }

    pub fn fetch_leaderboard(&mut self, cfg: &Config) -> Result<serde_json::Value> {
        let url = cfg.leaderboard_url();
        let (status, body) = self.get(&url)?;
        if status != 200 {
            bail!("HTTP {status} from {url}");
        }
        serde_json::from_str(&body)
            .with_context(|| format!("{url} didn't return JSON (is the leaderboard id right?)"))
    }
}

fn parse_submission(html: &str) -> Submission {
    let text = article_text(html);
    let lower = text.to_lowercase();
    if lower.contains("that's the right answer") {
        Submission::Correct
    } else if lower.contains("that's not the right answer") {
        let hint = if lower.contains("too high") {
            Some("too high".to_owned())
        } else if lower.contains("too low") {
            Some("too low".to_owned())
        } else {
            None
        };
        Submission::Wrong { hint }
    } else if lower.contains("you gave an answer too recently") {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"(?i)you have ([^.]*?) left to wait").unwrap());
        let wait = re
            .captures(&text)
            .map(|c| c[1].to_owned())
            .unwrap_or_else(|| "a little while".into());
        Submission::TooSoon { wait }
    } else if lower.contains("you don't seem to be solving the right level") {
        Submission::AlreadySolved
    } else {
        Submission::Unknown(text)
    }
}

/// The text of the first <article>, without tags.
fn article_text(html: &str) -> String {
    static ART: OnceLock<Regex> = OnceLock::new();
    static TAG: OnceLock<Regex> = OnceLock::new();
    let art = ART.get_or_init(|| Regex::new(r"(?s)<article[^>]*>(.*?)</article>").unwrap());
    let tag = TAG.get_or_init(|| Regex::new(r"<[^>]+>").unwrap());
    let inner = art
        .captures(html)
        .map(|c| c[1].to_owned())
        .unwrap_or_else(|| html.to_owned());
    let text = unescape(&tag.replace_all(&inner, ""));
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn load_session(helper_dir: &Path) -> Option<String> {
    let from_file = || {
        let text = fs::read_to_string(helper_dir.join(".env")).ok()?;
        dotenvy::from_read_iter(text.as_bytes())
            .filter_map(|r| r.ok())
            .find(|(k, _)| k == "AOC_SESSION")
            .map(|(_, v)| v)
    };
    let raw = env::var("AOC_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(from_file)?;
    clean_session(&raw)
}

/// Accept `abc`, `session=abc`, or a whole pasted cookie header.
fn clean_session(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_matches(['"', '\'']);
    let value = raw
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("session="))
        .unwrap_or(raw)
        .trim();
    (!value.is_empty() && !value.contains('=')).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions() {
        assert_eq!(clean_session("abc123").as_deref(), Some("abc123"));
        assert_eq!(clean_session("session=abc123").as_deref(), Some("abc123"));
        assert_eq!(
            clean_session("_ga=GA1.2.3; session=abc123").as_deref(),
            Some("abc123")
        );
        assert_eq!(clean_session(" \"abc\" ").as_deref(), Some("abc"));
        assert_eq!(clean_session(""), None);
        assert_eq!(clean_session("_ga=x"), None);
    }

    #[test]
    fn submissions() {
        let page = |s: &str| format!("<html><main><article><p>{s}</p></article></main></html>");
        assert!(matches!(
            parse_submission(&page(
                "That's the right answer! You are one gold star closer."
            )),
            Submission::Correct
        ));
        match parse_submission(&page(
            "That's not the right answer; your answer is too high. If you're stuck...",
        )) {
            Submission::Wrong { hint } => assert_eq!(hint.as_deref(), Some("too high")),
            _ => panic!(),
        }
        match parse_submission(&page(
            "You gave an answer too recently; you have to wait after submitting an answer before trying again.  You have 34s left to wait.",
        )) {
            Submission::TooSoon { wait } => assert_eq!(wait, "34s"),
            _ => panic!(),
        }
        assert!(matches!(
            parse_submission(&page(
                "You don't seem to be solving the right level.  Did you already complete it?"
            )),
            Submission::AlreadySolved
        ));
    }
}
