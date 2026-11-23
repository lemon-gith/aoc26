//! The simpler commands. `aoc watch` lives in watch.rs.

use anyhow::{Result, bail};
use chrono::Utc;
use crossterm::style::Stylize;

use crate::actions::{self, Outcome, Submitted};
use crate::answers::Check;
use crate::ctx::Ctx;
use crate::{spec, ui};

fn default_specs(ctx: &mut Ctx, specs: Vec<String>) -> Vec<String> {
    if specs.is_empty() {
        vec![ctx.current_day().to_string()]
    } else {
        specs
    }
}

pub fn pull(ctx: &mut Ctx, specs: Vec<String>) -> Result<()> {
    let specs = default_specs(ctx, specs);
    let all = ctx.unlocked_days();
    let days = spec::days(&specs, ctx.cfg.days, &all)?;
    let mut failed = false;
    for day in days {
        if ctx.input_path(day).is_file() {
            ui::note(&format!("day {day}: already have inputs/{day:02}.txt"));
        } else {
            failed |= !actions::pull_verbose(ctx, day);
        }
    }
    if failed {
        bail!("some inputs couldn't be downloaded");
    }
    Ok(())
}

pub fn day(ctx: &mut Ctx, day: Option<u8>) -> Result<()> {
    match day {
        Some(d) => actions::set_day(ctx, d),
        None => {
            let d = ctx.current_day();
            let s = ctx.ys().stars(d);
            println!("Day {d} {}  ({})", ui::stars(s), ctx.year());
            Ok(())
        }
    }
}

pub fn run(ctx: &mut Ctx, specs: Vec<String>) -> Result<()> {
    let specs = default_specs(ctx, specs);
    // `all` means every day that has a module
    let implemented: Vec<u8> = (1..=ctx.cfg.days)
        .filter(|&d| ctx.day_file(d).is_file() || ctx.day_dir(d).is_dir())
        .collect();
    let jobs = spec::parse(&specs, ctx.cfg.days, &implemented)?;
    if jobs.is_empty() {
        bail!("nothing to run: no days have been set up yet (try `aoc day 1`)");
    }
    let mut days: Vec<u8> = jobs.iter().map(|j| j.0).collect();
    days.dedup();
    let have_session = ctx.has_session();
    for &d in &days {
        if !ctx.input_path(d).is_file() && have_session && ctx.is_unlocked(d) {
            actions::pull_verbose(ctx, d);
        }
    }
    let results = actions::run_solution(ctx, &jobs, None)?.unwrap_or_default();
    println!();
    let mut total = 0;
    for r in &results {
        println!("{}", actions::describe(ctx, r).0);
        total += r.micros;
    }
    if results.len() > 1 {
        println!(
            "{}",
            format!("  total {}", actions::fmt_time(total)).dark_grey()
        );
    }
    Ok(())
}

pub fn submit(ctx: &mut Ctx, spec_str: String, answer: Option<String>, yes: bool) -> Result<()> {
    let (day, part) =
        match spec::parse(std::slice::from_ref(&spec_str), ctx.cfg.days, &[])?.as_slice() {
            [(d, p)] => (*d, *p),
            _ => bail!("submit one part at a time, like `aoc submit 5.1`"),
        };
    ctx.ensure_unlocked(day)?;
    let year = ctx.year();
    if part == 2 && ctx.ys().stars(day) == 0 {
        bail!("part 2 opens once part 1 is solved");
    }
    let answer = match answer {
        Some(a) => a,
        None => {
            if !ctx.input_path(day).is_file() {
                actions::pull_verbose(ctx, day);
            }
            let results = actions::run_solution(ctx, &[(day, part)], None)?.unwrap_or_default();
            match results.into_iter().next().map(|r| r.outcome) {
                Some(Outcome::Answer(a)) => a,
                Some(Outcome::Unsolved) => {
                    bail!("day {day} part {part} still returns Answer::Unsolved")
                }
                Some(Outcome::Error(e)) => bail!("day {day} part {part} failed: {e}"),
                None => bail!("the solution didn't report a result"),
            }
        }
    };
    match ctx.answers.day(year, day).check(part, &answer) {
        Check::Correct => {
            ui::ok(&format!(
                "{answer} is already the accepted answer for day {day} part {part}"
            ));
            return Ok(());
        }
        Check::Incorrect(right) => bail!("day {day} part {part} is already solved (with {right})"),
        Check::KnownWrong(why) => bail!("not submitting {answer}: {why}"),
        Check::Unknown => {}
    }
    if !yes
        && ui::confirm(
            &format!(
                "Submit {} for day {day} part {part}?",
                answer.clone().bold()
            ),
            None,
        )? != Some(true)
    {
        return Ok(());
    }
    if let Submitted::Correct = actions::submit(ctx, day, part, &answer)?
        && part == 2
        && let Some(next) = actions::after_day_complete(ctx, day, None)?
    {
        actions::set_day(ctx, next)?;
    }
    Ok(())
}

pub fn sync(ctx: &mut Ctx, specs: Vec<String>) -> Result<()> {
    let all = ctx.unlocked_days();
    let specs = if specs.is_empty() {
        vec!["all".to_owned()]
    } else {
        specs
    };
    let days = spec::days(&specs, ctx.cfg.days, &all)?;
    for day in days {
        if !ctx.is_unlocked(day) {
            continue;
        }
        match actions::sync_day(ctx, day) {
            Ok(s) => println!("Day {day:>2} {}", ui::stars(s)),
            Err(e) => bail!("day {day}: {e:#}"),
        }
    }
    Ok(())
}

pub fn status(ctx: &mut Ctx) -> Result<()> {
    let board = match actions::leaderboard(ctx) {
        Ok(b) => b,
        Err(e) => {
            ui::warn(&format!("couldn't fetch the leaderboard: {e:#}"));
            None
        }
    };
    let year = ctx.year();
    let current = ctx.current_day();
    ui::heading(&format!("Advent of Code {year}"));
    for day in 1..=ctx.cfg.days {
        let marker = if day == current {
            "▶".cyan().bold().to_string()
        } else {
            " ".into()
        };
        let line = if ctx.is_unlocked(day) {
            let s = ctx.ys().stars(day);
            let file = if ctx.day_file(day).is_file() || ctx.day_dir(day).is_dir() {
                ""
            } else {
                "  (not started)"
            };
            format!(
                "{marker} Day {day:>2}  {}{}",
                ui::stars(s),
                file.dark_grey()
            )
        } else {
            let wait = ui::countdown(ctx.unlock_time(day) - Utc::now());
            format!(
                "{marker} Day {day:>2}  {}",
                format!("unlocks in {wait}").dark_grey()
            )
        };
        println!("{line}");
    }
    let stars = actions::star_count(ctx);
    println!(
        "\n  {} {} of {}",
        "★".yellow().bold(),
        stars.to_string().bold(),
        ctx.cfg.days as u32 * 2
    );

    if let Some(board) = board {
        let mut members: Vec<&serde_json::Value> = board.data["members"]
            .as_object()
            .map(|m| m.values().collect())
            .unwrap_or_default();
        members.sort_by_key(|m| {
            std::cmp::Reverse((
                m["local_score"].as_u64().unwrap_or(0),
                m["stars"].as_u64().unwrap_or(0),
            ))
        });
        let me_id = actions::me(ctx, &board).and_then(|m| m["id"].as_u64());
        let age = (chrono::Utc::now().timestamp() as u64).saturating_sub(board.fetched_at) / 60;
        ui::heading(&format!(
            "Leaderboard {}",
            format!("(updated {age} min ago)").dark_grey()
        ));
        for (i, m) in members.iter().enumerate() {
            let name = m["name"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("(anonymous #{})", m["id"]));
            let line = format!(
                "{:>3}. {:>5}  {:>3}★  {name}",
                i + 1,
                m["local_score"].as_u64().unwrap_or(0),
                m["stars"].as_u64().unwrap_or(0),
            );
            if m["id"].as_u64().is_some() && m["id"].as_u64() == me_id {
                println!("{}", line.bold());
            } else {
                println!("{line}");
            }
        }
    }
    Ok(())
}
