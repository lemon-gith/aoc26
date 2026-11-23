//! `aoc`: a helper for solving Advent of Code in this repo.
//! Run `aoc help` (or `aoc <command> --help`) for usage.

mod actions;
mod answers;
mod api;
mod commands;
mod config;
mod ctx;
mod spec;
mod state;
mod ui;
mod watch;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "aoc",
    version,
    about = "Advent of Code helper: watch, run, pull, submit",
    infer_subcommands = true,
    after_help = "Any unambiguous prefix of a command works too (e.g. `aoc wa`, `aoc st`).\n\
                  DAY SPECS: 5 (both parts), 5.1 (part 1), 3-7, all, all.2"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Re-check a day on every save: clippy → tests → real input → submit
    #[command(visible_alias = "w")]
    Watch {
        /// day to watch (default: the current day)
        day: Option<u8>,
    },
    /// Run solutions against your real inputs
    #[command(visible_alias = "r")]
    Run {
        /// what to run, e.g. `5`, `5.1 6`, `all`, `all.1` (default: the current day)
        specs: Vec<String>,
    },
    /// Download puzzle inputs into inputs/ (never re-downloads)
    #[command(visible_alias = "p")]
    Pull {
        /// days to download, e.g. `5`, `1-4`, `all` (default: the current day)
        specs: Vec<String>,
    },
    /// Show the current day, or switch to a day (creates its file and pulls its input)
    #[command(visible_alias = "d")]
    Day { day: Option<u8> },
    /// Set things, e.g. `aoc set day 5` (same as `aoc day 5`)
    Set {
        #[command(subcommand)]
        what: SetCmd,
    },
    /// Calendar of your stars, plus the private leaderboard if configured
    #[command(visible_alias = "s")]
    Status,
    /// Submit an answer: computes it from your solution unless you give one
    #[command(visible_alias = "sub")]
    Submit {
        /// which part, e.g. `5.1`
        part: String,
        /// the answer to submit (default: run your solution)
        answer: Option<String>,
        /// don't ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Re-read which parts you've solved (and their answers) from the AoC site
    Sync {
        /// days to sync (default: all unlocked days)
        specs: Vec<String>,
    },
}

#[derive(Subcommand)]
enum SetCmd {
    /// Switch to a day
    #[command(visible_alias = "d")]
    Day { day: u8 },
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        ui::bad(&format!("{e:#}"));
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let mut ctx = ctx::Ctx::load()?;
    if ctx.cfg.user_agent.trim().is_empty() && needs_network(&cli.cmd) {
        ui::note(
            "tip: set user_agent in aoc_helper/config.toml (AoC asks tools to include contact info)",
        );
    }
    match cli.cmd {
        Cmd::Watch { day } => watch::watch(&mut ctx, day),
        Cmd::Run { specs } => commands::run(&mut ctx, specs),
        Cmd::Pull { specs } => commands::pull(&mut ctx, specs),
        Cmd::Day { day } => commands::day(&mut ctx, day),
        Cmd::Set {
            what: SetCmd::Day { day },
        } => commands::day(&mut ctx, Some(day)),
        Cmd::Status => commands::status(&mut ctx),
        Cmd::Submit { part, answer, yes } => commands::submit(&mut ctx, part, answer, yes),
        Cmd::Sync { specs } => commands::sync(&mut ctx, specs),
    }
}

fn needs_network(cmd: &Cmd) -> bool {
    matches!(
        cmd,
        Cmd::Pull { .. }
            | Cmd::Submit { .. }
            | Cmd::Sync { .. }
            | Cmd::Day { day: Some(_) }
            | Cmd::Set { .. }
    )
}
