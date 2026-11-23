//! Building blocks shared by several commands.

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use crossterm::style::Stylize;
use serde_json::Value;

use crate::answers::Check;
use crate::api::Submission;
use crate::ctx::Ctx;
use crate::ui::{self, Key};

// ---------------------------------------------------------------- inputs

/// Make sure the input for `day` is on disk, downloading it if needed. Never re-downloads.
pub fn pull(ctx: &mut Ctx, day: u8) -> Result<bool> {
    ctx.sync_inputs_year()?;
    let path = ctx.input_path(day);
    if path.is_file() {
        return Ok(false);
    }
    ctx.ensure_unlocked(day)?;
    let input = with_client(ctx, |c, cfg| c.fetch_input(cfg, day))?;
    fs::write(&path, input).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

/// Pull, printing what happened; returns whether the input is now available.
pub fn pull_verbose(ctx: &mut Ctx, day: u8) -> bool {
    match pull(ctx, day) {
        Ok(true) => {
            ui::ok(&format!(
                "downloaded input for day {day} → inputs/{day:02}.txt"
            ));
            true
        }
        Ok(false) => true,
        Err(e) => {
            ui::bad(&format!("couldn't get the input for day {day}: {e:#}"));
            false
        }
    }
}

// ---------------------------------------------------------------- AoC calls needing the config

/// Run `f` with the client and config at the same time.
fn with_client<T>(
    ctx: &mut Ctx,
    f: impl FnOnce(&mut crate::api::Client, &crate::config::Config) -> Result<T>,
) -> Result<T> {
    let (client, cfg) = ctx.api();
    f(client, cfg)
}

/// Read the puzzle page to learn which parts are solved and their answers.
pub fn sync_day(ctx: &mut Ctx, day: u8) -> Result<u8> {
    let page = with_client(ctx, |c, cfg| c.fetch_puzzle(cfg, day))?;
    let year = ctx.year();
    for (i, ans) in page.answers.iter().enumerate().take(2) {
        ctx.answers.set_correct(year, day, i as u8 + 1, ans);
    }
    let stars = page.answers.len().min(2) as u8;
    ctx.ys().set_stars(day, stars);
    ctx.answers.save()?;
    ctx.state.save()?;
    Ok(stars)
}

// ---------------------------------------------------------------- leaderboard

pub struct Board {
    pub data: Value,
    pub fetched_at: u64,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The private leaderboard, from cache unless it's older than the refresh interval
/// (never less than 15 minutes). `None` if no leaderboard is configured.
pub fn leaderboard(ctx: &mut Ctx) -> Result<Option<Board>> {
    if ctx.cfg.leaderboard.id.trim().is_empty() {
        return Ok(None);
    }
    let cache = ctx.cache_dir().join(format!(
        "leaderboard-{}-{}.json",
        ctx.year(),
        ctx.cfg.leaderboard.id.trim()
    ));
    let cached: Option<Board> = fs::read_to_string(&cache).ok().and_then(|t| {
        let v: Value = serde_json::from_str(&t).ok()?;
        Some(Board {
            fetched_at: v["fetched_at"].as_u64()?,
            data: v["data"].clone(),
        })
    });
    let max_age = ctx.cfg.leaderboard.refresh_minutes.max(15) * 60;
    if let Some(b) = &cached
        && now().saturating_sub(b.fetched_at) < max_age
    {
        return Ok(cached);
    }
    if !ctx.has_session() {
        return Ok(cached);
    }
    match with_client(ctx, |c, cfg| c.fetch_leaderboard(cfg)) {
        Ok(data) => {
            let board = Board {
                data,
                fetched_at: now(),
            };
            fs::create_dir_all(ctx.cache_dir())?;
            fs::write(
                &cache,
                serde_json::json!({ "fetched_at": board.fetched_at, "data": board.data })
                    .to_string(),
            )?;
            absorb_board(ctx, &board);
            Ok(Some(board))
        }
        Err(e) if cached.is_some() => {
            ui::warn(&format!(
                "couldn't refresh the leaderboard ({e:#}); showing the cached one"
            ));
            Ok(cached)
        }
        Err(e) => Err(e),
    }
}

/// Your own entry on the leaderboard.
pub fn me<'a>(ctx: &Ctx, board: &'a Board) -> Option<&'a Value> {
    let id = if ctx.cfg.leaderboard.user_id.trim().is_empty() {
        match &board.data["owner_id"] {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => return None,
        }
    } else {
        ctx.cfg.leaderboard.user_id.trim().to_owned()
    };
    board.data["members"].get(&id)
}

/// Learn about solved days from the leaderboard (e.g. solved on another machine).
fn absorb_board(ctx: &mut Ctx, board: &Board) {
    let Some(levels) = me(ctx, board)
        .and_then(|m| m["completion_day_level"].as_object())
        .cloned()
    else {
        return;
    };
    for (day, parts) in levels {
        if let (Ok(day), Some(parts)) = (day.parse::<u8>(), parts.as_object()) {
            ctx.ys().set_stars(day, parts.len().min(2) as u8);
        }
    }
    let _ = ctx.state.save();
}

/// Star count: the leaderboard's if it's configured (and fresh enough), else ours.
pub fn star_count(ctx: &mut Ctx) -> u32 {
    let local = ctx.ys().total_stars();
    let board = match leaderboard(ctx) {
        Ok(Some(b)) => b,
        _ => return local,
    };
    let remote = me(ctx, &board)
        .and_then(|m| m["stars"].as_u64())
        .unwrap_or(0) as u32;
    remote.max(local)
}

// ---------------------------------------------------------------- running solutions

pub struct PartResult {
    pub day: u8,
    pub part: u8,
    pub outcome: Outcome,
    pub micros: u128,
}

pub enum Outcome {
    Answer(String),
    Unsolved,
    Error(String),
}

/// Build and run the solution crate for these (day, part)s.
/// Lines the solution prints itself (debug output) are passed straight through.
/// Returns `None` if a watched file changed while it ran.
pub fn run_solution(
    ctx: &Ctx,
    jobs: &[(u8, u8)],
    changes: Option<&Receiver<()>>,
) -> Result<Option<Vec<PartResult>>> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&ctx.root)
        .args(["run", "--quiet", "--color", "always", "--bin", "aoc26"]);
    if ctx.cfg.run.release {
        cmd.arg("--release");
    }
    cmd.args(["--", "--machine"]);
    cmd.args(jobs.iter().map(|(d, p)| format!("{d}.{p}")));
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());
    let mut child = cmd.spawn().context("running cargo (is it installed?)")?;

    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut results = vec![];
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match parse_result(&line) {
                Some(r) => results.push(r),
                None => println!("{line}"),
            }
        }
        results
    });

    loop {
        if let Some(status) = child.try_wait()? {
            let results = reader.join().unwrap_or_default();
            if !status.success() && results.is_empty() {
                bail!("the solution crate failed to build or run (see above)");
            }
            return Ok(Some(results));
        }
        if let Some(c) = changes
            && c.try_recv().is_ok()
        {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn parse_result(line: &str) -> Option<PartResult> {
    let mut f = line.strip_prefix("AOC\t")?.split('\t');
    let day = f.next()?.parse().ok()?;
    let part = f.next()?.parse().ok()?;
    let kind = f.next()?;
    let value = unescape(f.next()?);
    let micros = f.next()?.parse().ok()?;
    let outcome = match kind {
        "ok" => Outcome::Answer(value),
        "unsolved" => Outcome::Unsolved,
        _ => Outcome::Error(value),
    };
    Some(PartResult {
        day,
        part,
        outcome,
        micros,
    })
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn fmt_time(micros: u128) -> String {
    let t = if micros == 0 {
        "<1µs".to_owned()
    } else if micros < 1_000 {
        format!("{micros}µs")
    } else if micros < 1_000_000 {
        format!("{:.1}ms", micros as f64 / 1e3)
    } else {
        format!("{:.2}s", micros as f64 / 1e6)
    };
    format!("{}", t.dark_grey())
}

/// One line describing a result, checked against known answers.
pub fn describe(ctx: &Ctx, r: &PartResult) -> (String, Option<Check>) {
    let label = format!("Day {:>2} part {}", r.day, r.part);
    let time = fmt_time(r.micros);
    match &r.outcome {
        Outcome::Unsolved => (
            format!("{} {label}: {}", "·".dark_grey(), "unsolved".dark_grey()),
            None,
        ),
        Outcome::Error(e) => (
            format!("{} {label}: {}", "✗".red().bold(), e.clone().red()),
            None,
        ),
        Outcome::Answer(a) => {
            let check = ctx.answers.day(ctx.year(), r.day).check(r.part, a);
            let line = match &check {
                Check::Correct => format!(
                    "{} {label}: {}  {time}",
                    "✓".green().bold(),
                    a.clone().bold()
                ),
                Check::Incorrect(right) => {
                    format!(
                        "{} {label}: {}  {time}  {}",
                        "✗".red().bold(),
                        a.clone().bold(),
                        format!("(accepted answer was {right})").red()
                    )
                }
                Check::KnownWrong(why) => format!(
                    "{} {label}: {}  {time}  {}",
                    "✗".red().bold(),
                    a.clone().bold(),
                    format!("({why})").red()
                ),
                Check::Unknown => format!(
                    "{} {label}: {}  {time}  {}",
                    "?".cyan().bold(),
                    a.clone().bold(),
                    "(not checked yet)".dark_grey()
                ),
            };
            (line, Some(check))
        }
    }
}

// ---------------------------------------------------------------- submitting

pub enum Submitted {
    Correct,
    NotCorrect,
    Skipped,
}

/// Submit one answer (after the caller has confirmed), recording what AoC says.
pub fn submit(ctx: &mut Ctx, day: u8, part: u8, answer: &str) -> Result<Submitted> {
    let year = ctx.year();
    let result = with_client(ctx, |c, cfg| c.submit(cfg, day, part, answer))?;
    let outcome = match result {
        Submission::Correct => {
            ui::ok(&format!(
                "{} Day {day} part {part} is correct!",
                "★".yellow().bold()
            ));
            ctx.answers.set_correct(year, day, part, answer);
            let stars = if part == 1 { 1 } else { 2 };
            ctx.ys().set_stars(day, stars);
            Submitted::Correct
        }
        Submission::Wrong { hint } => {
            let h = hint
                .as_deref()
                .map(|h| format!(", it's {h}"))
                .unwrap_or_default();
            ui::bad(&format!(
                "{answer} isn't right{h}. AoC will make you wait a bit before the next try."
            ));
            ctx.answers.add_wrong(year, day, part, answer, hint);
            Submitted::NotCorrect
        }
        Submission::TooSoon { wait } => {
            ui::warn(&format!(
                "AoC says you answered too recently: {wait} left to wait."
            ));
            Submitted::Skipped
        }
        Submission::AlreadySolved => {
            ui::note("AoC says that part's already solved; syncing from the puzzle page…");
            sync_day(ctx, day)?;
            return Ok(match ctx.answers.day(year, day).check(part, answer) {
                Check::Correct => Submitted::Correct,
                _ => Submitted::NotCorrect,
            });
        }
        Submission::Unknown(text) => {
            ui::warn(&format!(
                "Couldn't tell what AoC said: {}",
                text.chars().take(300).collect::<String>()
            ));
            Submitted::Skipped
        }
    };
    ctx.answers.save()?;
    ctx.state.save()?;
    Ok(outcome)
}

// ---------------------------------------------------------------- day setup & what next

/// Create the day's module from the template and register it in days/mod.rs.
pub fn scaffold(ctx: &Ctx, day: u8) -> Result<bool> {
    let file = ctx.day_file(day);
    let module = format!("d{day:02}");
    let mod_rs = ctx.root.join("src/days/mod.rs");
    let mut created = false;

    if !file.is_file() && !ctx.day_dir(day).join("mod.rs").is_file() {
        let template_path = ctx.root.join("src/days/template.rs.txt");
        let template = fs::read_to_string(&template_path)
            .with_context(|| format!("reading {}", template_path.display()))?;
        let body = template
            .replace("{day}", &day.to_string())
            .replace("{year}", &ctx.year().to_string());
        fs::write(&file, body)?;
        created = true;
    }

    let text = fs::read_to_string(&mod_rs).context("reading src/days/mod.rs")?;
    let entry = regex::Regex::new(&format!(r"(?m)^\s*{day}\s*=>\s*{module}\s*,"))?;
    if !entry.is_match(&text) {
        let marker = "// aoc:days";
        let Some(pos) = text.find(marker) else {
            bail!(
                "src/days/mod.rs is missing the `{marker}` line; add `{day} => {module},` to days! by hand"
            );
        };
        let line_start = text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let indent = &text[line_start..pos];
        let new = format!(
            "{}{indent}{day} => {module},\n{}",
            &text[..line_start],
            &text[line_start..]
        );
        fs::write(&mod_rs, new)?;
        created = true;
    }
    Ok(created)
}

/// Switch to a day: scaffold it, pull its input, sync its progress, make it current.
pub fn set_day(ctx: &mut Ctx, day: u8) -> Result<()> {
    ctx.check_day(day)?;
    ctx.ensure_unlocked(day)?;
    if scaffold(ctx, day)? {
        ui::ok(&format!("created src/days/d{day:02}.rs"));
    }
    if ctx.has_session() {
        pull_verbose(ctx, day);
        if let Err(e) = sync_day(ctx, day) {
            ui::warn(&format!("couldn't read the day {day} puzzle page: {e:#}"));
        }
        if let Err(e) = leaderboard(ctx) {
            ui::warn(&format!("couldn't fetch the leaderboard: {e:#}"));
        }
    } else if !ctx.input_path(day).is_file() {
        ui::warn("no session cookie in aoc_helper/.env, so the input wasn't downloaded");
    }
    ctx.ys().current_day = day;
    ctx.state.save()?;
    let stars = ctx.ys().stars(day);
    ui::ok(&format!(
        "now on day {day} {}  ·  {}",
        ui::stars(stars),
        ctx.cfg.puzzle_url(day).underlined()
    ));
    Ok(())
}

/// Days that are unlocked but not finished.
pub fn unfinished(ctx: &mut Ctx) -> Vec<u8> {
    let days = ctx.unlocked_days();
    days.into_iter()
        .filter(|&d| ctx.ys().stars(d) < 2)
        .collect()
}

/// Called when a day has just been completed: congratulate, then offer the next thing.
/// Returns the day to switch to, if the user picked one.
pub fn after_day_complete(
    ctx: &mut Ctx,
    day: u8,
    changes: Option<&Receiver<()>>,
) -> Result<Option<u8>> {
    let stars = star_count(ctx);
    println!();
    println!(
        "🎄  Day {day} complete! You now have {} {}.",
        stars.to_string().yellow().bold(),
        if stars == 1 { "star" } else { "stars" }
    );
    let todo = unfinished(ctx);
    let total = ctx.cfg.days;
    if todo.is_empty() {
        if ctx.unlocked_days().len() == total as usize {
            println!();
            println!(
                "{}",
                format!(
                    "🎉  That's every puzzle of {} done. Congratulations!! 🎉",
                    ctx.year()
                )
                .yellow()
                .bold()
            );
        } else if let Some(next) = (1..=total).find(|&d| !ctx.is_unlocked(d)) {
            let wait = ctx.unlock_time(next) - chrono::Utc::now();
            ui::note(&format!(
                "All caught up! Day {next} unlocks in {}.",
                ui::countdown(wait)
            ));
        }
        return Ok(None);
    }
    let suggestion = todo.iter().copied().find(|&d| d > day).unwrap_or(todo[0]);
    let others: Vec<String> = todo.iter().map(|d| d.to_string()).collect();
    if todo.len() > 1 {
        ui::note(&format!("Unfinished days: {}", others.join(", ")));
    }
    let next_label = format!("[enter] day {suggestion}");
    let mut options = vec![(Key::Enter, next_label.as_str()), (Key::Char('y'), "")];
    if todo.len() > 1 {
        options.push((Key::Char('o'), "[o]ther day"));
    }
    options.push((Key::Char('s'), "[s]tay"));
    options.push((Key::Esc, ""));
    let choice = ui::choose("Move on?", &options, changes)?;
    match choice {
        Some(Key::Enter | Key::Char('y')) => Ok(Some(suggestion)),
        Some(Key::Char('o')) => {
            let line = ui::read_line(&format!("Which day ({})?", others.join(", ")))?;
            match line.parse::<u8>() {
                Ok(d) if todo.contains(&d) || ctx.is_unlocked(d) && d <= total => Ok(Some(d)),
                _ => {
                    ui::warn("not an unlocked day; staying put");
                    Ok(None)
                }
            }
        }
        _ => Ok(None),
    }
}
