//! `aoc watch`: on every save of the day's files, run clippy → tests → (optionally)
//! the real input → submission, asking before each step that needs a decision.

use std::collections::HashSet;
use std::io::{Read, Write, stdout};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::cursor::MoveTo;
use crossterm::style::Stylize;
use crossterm::terminal::{Clear, ClearType};
use notify::{EventKind, RecursiveMode, Watcher};
use regex::Regex;
use serde_json::Value;

use crate::actions::{self, Outcome, Submitted};
use crate::answers::Check;
use crate::config::{self, WarnMode};
use crate::ctx::Ctx;
use crate::ui::{self, Key, Pressed};

enum Flow {
    /// wait for the next save
    Wait,
    /// something changed mid-run: start again
    Rerun,
    Switch(u8),
}

struct Session {
    day: u8,
    /// whether we've already offered to move on from this (completed) day
    offered_move: bool,
}

pub fn watch(ctx: &mut Ctx, day: Option<u8>) -> Result<()> {
    let day = match day {
        Some(d) => d,
        None => ctx.current_day(),
    };
    ctx.check_day(day)?;
    if actions::scaffold(ctx, day)? {
        ui::ok(&format!("created src/days/d{day:02}.rs"));
    }

    let target = Arc::new(Mutex::new(day));
    let (tx, rx) = channel::<()>();
    let days_dir = ctx.root.join("src").join("days");
    let handler = {
        let target = target.clone();
        let days_dir = days_dir.clone();
        move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
                return; // cargo reading the files shouldn't count as a change
            }
            let day = *target.lock().unwrap();
            if event.paths.iter().any(|p| is_day_path(&days_dir, day, p)) {
                let _ = tx.send(());
            }
        }
    };
    let mut watcher: Box<dyn Watcher> = if ctx.cfg.watch.poll {
        let cfg = notify::Config::default()
            .with_poll_interval(Duration::from_millis(ctx.cfg.watch.poll_interval_ms));
        Box::new(notify::PollWatcher::new(handler, cfg)?)
    } else {
        Box::new(notify::recommended_watcher(handler)?)
    };
    watcher
        .watch(&days_dir, RecursiveMode::Recursive)
        .context("watching src/days")?;

    let mut session = Session {
        day,
        offered_move: false,
    };
    loop {
        let flow = pipeline(ctx, &mut session, &rx)?;
        match flow {
            Flow::Rerun => debounce(ctx, &rx),
            Flow::Switch(d) => {
                actions::set_day(ctx, d)?;
                *target.lock().unwrap() = d;
                session = Session {
                    day: d,
                    offered_move: false,
                };
                thread::sleep(Duration::from_millis(800));
            }
            Flow::Wait => {
                println!(
                    "\n{}",
                    format!(
                        "watching src/days/d{:02}… · save to re-run · [r]e-run · [q]uit",
                        session.day
                    )
                    .dark_grey()
                );
                loop {
                    match ui::read_key(Some(&rx))? {
                        Pressed::Changed => {
                            debounce(ctx, &rx);
                            break;
                        }
                        Pressed::Key(Key::Char('r') | Key::Enter) => break,
                        Pressed::Key(Key::Char('q') | Key::Quit) => return Ok(()),
                        Pressed::Key(_) => {}
                    }
                }
            }
        }
    }
}

fn is_day_path(days_dir: &Path, day: u8, path: &Path) -> bool {
    let file = days_dir.join(format!("d{day:02}.rs"));
    let dir = days_dir.join(format!("d{day:02}"));
    path == file || (path.starts_with(&dir) && path.extension().is_some_and(|e| e == "rs"))
}

/// Let a burst of saves settle, then forget them.
fn debounce(ctx: &Ctx, rx: &Receiver<()>) {
    thread::sleep(Duration::from_millis(ctx.cfg.watch.debounce_ms));
    while rx.try_recv().is_ok() {}
}

fn header(ctx: &Ctx, day: u8) {
    let mut out = stdout();
    let _ = crossterm::execute!(out, Clear(ClearType::All), MoveTo(0, 0));
    let stars = {
        let y = ctx
            .state
            .year
            .get(&ctx.year().to_string())
            .map(|y| y.stars(day))
            .unwrap_or(0);
        ui::stars(y)
    };
    println!(
        "{} {}  {}  {}",
        "aoc watch".bold().cyan(),
        format!("day {day}").bold(),
        stars,
        chrono::Local::now()
            .format("%H:%M:%S")
            .to_string()
            .dark_grey()
    );
}

fn status(msg: &str) {
    print!("{} {}", "…".dark_grey(), msg.dark_grey());
    let _ = stdout().flush();
}

fn clear_status() {
    print!("\r");
    let _ = crossterm::execute!(stdout(), Clear(ClearType::CurrentLine));
}

fn pipeline(ctx: &mut Ctx, s: &mut Session, rx: &Receiver<()>) -> Result<Flow> {
    let day = s.day;
    header(ctx, day);

    // ------------------------------------------------------------ 1. clippy
    status("clippy");
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&ctx.root).args([
        "clippy",
        "--all-targets",
        "--message-format=json-diagnostic-rendered-ansi",
        "--color",
        "always",
    ]);
    let Some(out) = run_interruptible(cmd, rx)? else {
        return Ok(Flow::Rerun);
    };
    clear_status();
    let diags = parse_clippy(&out.stdout);
    let (errors, warnings): (Vec<&Diag>, Vec<&Diag>) =
        diags.iter().partition(|d| d.level == "error");
    let (mine, elsewhere): (Vec<&Diag>, Vec<&Diag>) = warnings
        .into_iter()
        .partition(|d| d.file.as_deref().is_some_and(|f| is_day_file(f, day)));

    if !errors.is_empty() || (!out.status.success() && diags.is_empty()) {
        for d in &errors {
            print!("{}", d.rendered);
        }
        if errors.is_empty() {
            print!("{}", out.stderr);
        }
        println!();
        ui::bad(&format!(
            "clippy: {} error{}",
            errors.len(),
            plural(errors.len())
        ));
        return Ok(Flow::Wait);
    }

    if !mine.is_empty() {
        for d in &mine {
            print!("{}", d.rendered);
        }
        println!();
        let n = mine.len();
        ui::warn(&format!(
            "clippy: {n} warning{} in day {day}{}",
            plural(n),
            hidden(&elsewhere)
        ));
        match ctx.cfg.watch.warnings {
            WarnMode::Never => ui::note("testing anyway (warnings = \"never\" in config.toml)"),
            mode => {
                let mut options = vec![
                    (Key::Char('f'), "[f]ix (esc)"),
                    (Key::Esc, ""),
                    (Key::Char('t'), "[t]est anyways"),
                ];
                if mode == WarnMode::Ask {
                    options.push((Key::Space, "[space] don't ask again"));
                }
                match ui::choose("Fix the warnings, or test anyway?", &options, Some(rx))? {
                    None => return Ok(Flow::Rerun),
                    Some(Key::Char('t')) => {}
                    Some(Key::Space) => {
                        config::set_value(
                            &ctx.config_path,
                            Some("watch"),
                            "warnings",
                            "never".into(),
                        )?;
                        ctx.reload_config()?;
                        ui::note("saved warnings = \"never\" in aoc_helper/config.toml");
                    }
                    Some(_) => return Ok(Flow::Wait),
                }
            }
        }
    } else {
        ui::ok(&format!("clippy: clean{}", hidden(&elsewhere)));
    }

    // ------------------------------------------------------------ 2. tests
    status("tests");
    let module = format!("days::d{day:02}::");
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&ctx.root).args([
        "test", "--color", "always", "--", &module, "--color", "never",
    ]);
    let Some(out) = run_interruptible(cmd, rx)? else {
        return Ok(Flow::Rerun);
    };
    clear_status();
    let report = parse_tests(&out.stdout);
    if report.tests.is_empty() && !out.status.success() {
        print!("{}", out.stderr);
        ui::bad("tests failed to build");
        return Ok(Flow::Wait);
    }
    ui::heading(&format!("Tests for day {day}"));
    for t in &report.tests {
        let name = t.name.strip_prefix(&module).unwrap_or(&t.name);
        match t.result.as_str() {
            "ok" => println!("  {} {name}", "✓".green().bold()),
            "FAILED" => println!("  {} {}", "✗".red().bold(), name.red()),
            _ => println!(
                "  {} {}{}",
                "○".dark_grey(),
                name.dark_grey(),
                t.note
                    .as_deref()
                    .map(|n| format!("  ({n})"))
                    .unwrap_or_default()
                    .dark_grey()
            ),
        }
    }
    let failed: Vec<_> = report
        .tests
        .iter()
        .filter(|t| t.result == "FAILED")
        .collect();
    let passed = report.tests.iter().filter(|t| t.result == "ok").count();
    if !failed.is_empty() {
        for t in &failed {
            if let Some(output) = report
                .failures
                .iter()
                .find(|(n, _)| *n == t.name)
                .map(|(_, o)| o)
            {
                println!(
                    "\n{} {}",
                    "──".red(),
                    t.name.strip_prefix(&module).unwrap_or(&t.name).bold()
                );
                println!("{}", output.trim_end());
            }
        }
        println!();
        ui::bad(&format!(
            "{} of {} tests failed",
            failed.len(),
            failed.len() + passed
        ));
        return Ok(Flow::Wait);
    }
    if passed == 0 {
        println!("  {}", "no tests ran yet".dark_grey());
        println!();
        ui::warn("no passing tests (add your examples to the tests at the bottom of the day file)");
    } else {
        println!();
        ui::ok(&format!("all {passed} test{} passed", plural(passed)));
    }

    // ------------------------------------------------------------ 3. real input
    match ui::confirm("Run against your real input?", Some(rx))? {
        None => return Ok(Flow::Rerun),
        Some(false) => return Ok(Flow::Wait),
        Some(true) => {}
    }
    if !ctx.input_path(day).is_file() && !actions::pull_verbose(ctx, day) {
        return Ok(Flow::Wait);
    }
    status("running");
    let Some(results) = actions::run_solution(ctx, &[(day, 1), (day, 2)], Some(rx))? else {
        return Ok(Flow::Rerun);
    };
    clear_status();
    println!();
    let mut checks = vec![];
    for r in &results {
        let (line, check) = actions::describe(ctx, r);
        println!("{line}");
        checks.push((r, check));
    }

    // ------------------------------------------------------------ 4. submit
    let stars_before = ctx.ys().stars(day);
    let year = ctx.year();
    for (r, check) in &checks {
        let part = r.part;
        let Outcome::Answer(answer) = &r.outcome else {
            continue;
        };
        if !matches!(check, Some(Check::Unknown)) {
            continue;
        }
        if part == 2 && ctx.ys().stars(day) == 0 {
            break; // part 2 opens after part 1
        }
        if !ctx.has_session() {
            ui::note("add AOC_SESSION to aoc_helper/.env to submit from here");
            break;
        }
        if ctx.cfg.check_unlock && !ctx.is_unlocked(day) {
            break;
        }
        match ui::confirm(
            &format!("Submit {} for part {part}?", answer.clone().bold()),
            Some(rx),
        )? {
            None => return Ok(Flow::Rerun),
            Some(false) => break,
            Some(true) => match actions::submit(ctx, day, part, answer) {
                Ok(Submitted::Correct) => {}
                Ok(_) => break,
                Err(e) => {
                    ui::bad(&format!("submitting failed: {e:#}"));
                    break;
                }
            },
        }
    }

    // ------------------------------------------------------------ 5. day complete?
    let both = (1..=2).all(|p| {
        results.iter().any(|r| {
            r.part == p
                && matches!(&r.outcome, Outcome::Answer(a)
                    if matches!(ctx.answers.day(year, day).check(p, a), Check::Correct))
        })
    });
    if both {
        let just_now = stars_before < 2;
        if just_now || !s.offered_move {
            s.offered_move = true;
            if let Some(next) = actions::after_day_complete(ctx, day, Some(rx))? {
                return Ok(Flow::Switch(next));
            }
        } else {
            ui::ok(&format!("day {day} is complete {}", ui::stars(2)));
        }
    }
    Ok(Flow::Wait)
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn hidden(elsewhere: &[&Diag]) -> String {
    let n = elsewhere.len();
    if n == 0 {
        String::new()
    } else {
        format!(" ({n} warning{} in other files hidden)", plural(n))
    }
}

fn is_day_file(rel: &str, day: u8) -> bool {
    let rel = rel.replace('\\', "/");
    rel == format!("src/days/d{day:02}.rs") || rel.starts_with(&format!("src/days/d{day:02}/"))
}

// ---------------------------------------------------------------- processes

struct Captured {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

/// Run a command to completion, capturing its output, unless a watched file changes
/// first (then it's killed and `None` is returned).
fn run_interruptible(mut cmd: Command, rx: &Receiver<()>) -> Result<Option<Captured>> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().context("running cargo")?;
    let read = |mut r: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut s = String::new();
            let _ = r.read_to_string(&mut s);
            s
        })
    };
    let out = read(Box::new(child.stdout.take().unwrap()));
    let err = read(Box::new(child.stderr.take().unwrap()));
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(Captured {
                status,
                stdout: out.join().unwrap_or_default(),
                stderr: err.join().unwrap_or_default(),
            }));
        }
        if rx.try_recv().is_ok() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(30));
    }
}

// ---------------------------------------------------------------- clippy output

struct Diag {
    level: String,
    rendered: String,
    /// file of the primary span, relative to the repo root
    file: Option<String>,
}

fn parse_clippy(stdout: &str) -> Vec<Diag> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["reason"] != "compiler-message" {
            continue;
        }
        let msg = &v["message"];
        let level = msg["level"].as_str().unwrap_or("").to_owned();
        if level != "error" && level != "warning" {
            continue;
        }
        let spans = msg["spans"].as_array().cloned().unwrap_or_default();
        let text = msg["message"].as_str().unwrap_or("");
        // summaries like "3 warnings emitted" / "aborting due to 2 previous errors"
        if spans.is_empty() && (level == "warning" || text.starts_with("aborting due to")) {
            continue;
        }
        let rendered = msg["rendered"].as_str().unwrap_or(text).to_owned();
        if !seen.insert(rendered.clone()) {
            continue; // same diagnostic from the bin and test targets
        }
        let file = spans
            .iter()
            .find(|s| s["is_primary"] == true)
            .or(spans.first())
            .and_then(|s| s["file_name"].as_str())
            .map(str::to_owned);
        out.push(Diag {
            level,
            rendered,
            file,
        });
    }
    out
}

// ---------------------------------------------------------------- libtest output

struct TestLine {
    name: String,
    result: String,
    note: Option<String>,
}

struct TestReport {
    tests: Vec<TestLine>,
    /// (test name, captured output) for failed tests
    failures: Vec<(String, String)>,
}

fn parse_tests(stdout: &str) -> TestReport {
    let line_re = Regex::new(r"^test (\S+) \.\.\. (ok|FAILED|ignored)(?:, (.*))?$").unwrap();
    let block_re = Regex::new(r"^---- (\S+) stdout ----$").unwrap();
    let mut tests = vec![];
    let mut failures: Vec<(String, String)> = vec![];
    let mut current: Option<(String, String)> = None;
    for line in stdout.lines() {
        if let Some(c) = line_re.captures(line) {
            tests.push(TestLine {
                name: c[1].to_owned(),
                result: c[2].to_owned(),
                note: c.get(3).map(|m| m.as_str().to_owned()),
            });
            continue;
        }
        if let Some(c) = block_re.captures(line) {
            failures.extend(current.take());
            current = Some((c[1].to_owned(), String::new()));
            continue;
        }
        if line == "failures:" || line.starts_with("test result:") {
            failures.extend(current.take());
            continue;
        }
        if let Some((_, buf)) = current.as_mut() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    failures.extend(current.take());
    TestReport { tests, failures }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_paths() {
        let d = Path::new("/r/src/days");
        assert!(is_day_path(d, 5, Path::new("/r/src/days/d05.rs")));
        assert!(is_day_path(d, 5, Path::new("/r/src/days/d05/grid.rs")));
        assert!(!is_day_path(d, 5, Path::new("/r/src/days/d06.rs")));
        assert!(!is_day_path(d, 5, Path::new("/r/src/days/d05.rs.swp")));
        assert!(is_day_file("src/days/d05.rs", 5));
        assert!(is_day_file("src/days/d05/x.rs", 5));
        assert!(!is_day_file("src/days/d055.rs", 5));
    }

    #[test]
    fn libtest() {
        let out = "\
running 3 tests
test days::d05::tests::a ... ok
test days::d05::tests::b ... FAILED
test days::d05::tests::c ... ignored, todo

failures:

---- days::d05::tests::b stdout ----

thread 'days::d05::tests::b' panicked at src/days/d05.rs:20:9:
assertion `left == right` failed

failures:
    days::d05::tests::b

test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 3 filtered out
";
        let r = parse_tests(out);
        assert_eq!(r.tests.len(), 3);
        assert_eq!(r.tests[2].note.as_deref(), Some("todo"));
        assert_eq!(r.failures.len(), 1);
        assert!(r.failures[0].1.contains("assertion"));
    }
}
