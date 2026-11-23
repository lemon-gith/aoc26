//! Runs puzzle solutions against your real inputs.
//!
//! Usually driven by `aoc run`, but works standalone too:
//!
//! ```text
//! cargo run --release -- 5 6.1 all.2
//! ```
//!
//! `N` runs both parts of day N, `N.P` runs part P, `all` / `all.P` runs every implemented day.
//! Inputs are read from `inputs/NN.txt` (override the folder with `--input-dir DIR`).
//! `--machine` prints tab-separated result lines for the `aoc` helper instead of pretty output.

mod answer;
mod days;

use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{env, fs, process};

use answer::Answer;

fn main() {
    let mut machine = false;
    let mut input_dir = PathBuf::from("inputs");
    let mut jobs: Vec<(u8, u8)> = vec![];

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--machine" => machine = true,
            "--input-dir" => match args.next() {
                Some(dir) => input_dir = dir.into(),
                None => fail("--input-dir needs a value"),
            },
            spec => match parse_spec(spec) {
                Ok(mut parsed) => jobs.append(&mut parsed),
                Err(e) => fail(&e),
            },
        }
    }
    if jobs.is_empty() {
        fail("usage: aoc26 [--machine] [--input-dir DIR] <N | N.P | all | all.P>...");
    }

    // we report panics from puzzle code ourselves, so silence the default hook
    panic::set_hook(Box::new(|_| {}));

    let mut last_day = 0;
    for (day, part) in jobs {
        let path = input_dir.join(format!("{day:02}.txt"));
        let result = match fs::read_to_string(&path) {
            Ok(input) => run_part(day, part, &input),
            Err(_) => Err((
                format!("no input at {} (try `aoc pull {day}`)", path.display()),
                Duration::ZERO,
            )),
        };

        if machine {
            match result {
                Ok((Answer::Unsolved, t)) => {
                    println!("AOC\t{day}\t{part}\tunsolved\t\t{}", t.as_micros())
                }
                Ok((ans, t)) => println!(
                    "AOC\t{day}\t{part}\tok\t{}\t{}",
                    escape(&ans.to_string()),
                    t.as_micros()
                ),
                Err((e, t)) => {
                    println!("AOC\t{day}\t{part}\terr\t{}\t{}", escape(&e), t.as_micros())
                }
            }
        } else {
            if day != last_day {
                println!("\nDay {day}");
                last_day = day;
            }
            match result {
                Ok((ans, t)) => println!("  Part {part}: {ans}  ({t:.2?})"),
                Err((e, _)) => println!("  Part {part}: error: {e}"),
            }
        }
    }
}

fn run_part(day: u8, part: u8, input: &str) -> Result<(Answer, Duration), (String, Duration)> {
    let start = Instant::now();
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| days::solve(day, part, input)));
    let elapsed = start.elapsed();
    match outcome {
        Ok(Ok(ans)) => Ok((ans, elapsed)),
        Ok(Err(e)) => Err((e, elapsed)),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err((format!("panicked: {msg}"), elapsed))
        }
    }
}

fn parse_spec(spec: &str) -> Result<Vec<(u8, u8)>, String> {
    let (day, part) = match spec.split_once('.') {
        Some((d, p)) => (d, Some(p)),
        None => (spec, None),
    };
    let parts: Vec<u8> = match part {
        None => vec![1, 2],
        Some("1") => vec![1],
        Some("2") => vec![2],
        Some(p) => return Err(format!("'{p}' isn't a valid part (use 1 or 2)")),
    };
    let days: Vec<u8> = if day == "all" {
        days::implemented().to_vec()
    } else {
        vec![
            day.parse()
                .map_err(|_| format!("'{spec}' isn't a valid day spec"))?,
        ]
    };
    Ok(days
        .into_iter()
        .flat_map(|d| parts.iter().map(move |&p| (d, p)))
        .collect())
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    process::exit(2);
}
