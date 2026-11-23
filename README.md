# Advent of Code 2026

[Challenges](https://adventofcode.com/2026)

Solutions in Rust, with an `aoc` helper that re-checks your code every time you save:
**clippy → your tests → your real input → submit**, asking you before each step that needs a decision.

Everything (Rust, clippy, bacon, the `aoc` helper) lives in a Docker container. All you need on
your machine is Docker and a way to run `./start` (macOS/Linux terminal, WSL or Git Bash on Windows).

> [!IMPORTANT]
> **Don't commit your puzzle inputs.** The Advent of Code author asks that inputs aren't
> redistributed, so `inputs/` is gitignored. Its `answers.toml` (the answers to *your* inputs)
> stays local too. Grab your own with `aoc pull`.

## Getting started

```sh
./start            # builds the container the first time (a few minutes), then opens a shell in it
```

Inside that shell, `aoc` is a normal command:

```sh
aoc day 1          # create src/days/d01.rs, download the input, make it the current day
aoc watch          # re-check day 1 every time you save
```

Run `./start` again in another terminal for a second shell in the same container.
The repo is mounted at `/aoc`, so you can keep editing on your own machine with whatever tools
you like. Or attach your editor to the container: `.devcontainer/` is set up for VS Code and other
devcontainer-aware editors, with rust-analyzer inside.

| `./start ...` | |
|---|---|
| `./start` | open a shell in the container (starting/building it if needed) |
| `./start aoc status` | run one command in the container |
| `./start --rebuild` | rebuild the image from scratch |
| `./start stop` | stop the container |
| `./start clean` | remove the container and its cached builds |

Changes to `aoc_helper/` are picked up automatically the next time you run `./start`.

### Logging in to AoC (for downloading inputs and submitting)

1. Log in at [adventofcode.com](https://adventofcode.com), open your browser's dev tools and find the
   `session` cookie (Application/Storage → Cookies).
2. Copy `aoc_helper/.env.example` to `aoc_helper/.env` and paste **just the cookie's value** after
   `AOC_SESSION=`. You don't need the `_ga` cookies. `.env` is gitignored. Treat the cookie like
   a password, because it logs in as you.
3. Put a way to contact you in `user_agent` in `aoc_helper/config.toml`. AoC asks automated
   tools to do this.

The cookie lasts about a month. If downloads start failing with "rejected the session cookie",
grab a fresh one.

## The `aoc` command

Any unambiguous prefix of a command works too (`aoc wa`, `aoc st`), as well as the short aliases.

| Command | Alias | What it does |
|---|---|---|
| `aoc watch [day]` | `w` | the feedback loop (below); defaults to the current day |
| `aoc run [specs]` | `r` | run solutions against your real inputs, checking them against known answers |
| `aoc pull [specs]` | `p` | download inputs into `inputs/` (never re-downloads one you already have) |
| `aoc day [N]` / `aoc set day N` | `d` | show or switch the current day: creates `dNN.rs`, pulls its input, syncs its progress |
| `aoc status` | `s` | your stars calendar, plus your private leaderboard if one is configured |
| `aoc submit N.P [answer]` | `sub` | submit an answer (computed from your solution unless given) |
| `aoc sync [specs]` | | re-read which parts you've solved from the AoC site |

**Day specs:** `5` (both parts), `5.1` (part 1 only), `3-7`, `all`, `all.2`. Combine them freely:
`aoc run 5.1 6`.

### `aoc watch`

Each time you save the day's file (`src/days/dNN.rs`, or anything in `src/days/dNN/`):

1. **clippy** runs. Errors are shown and it waits for your next save.
   If there are only warnings in that day's files, it asks: **[f]ix** (or Esc), **[t]est anyways**,
   or **[space] don't ask again**. This is set by `warnings` in `config.toml`:
   `"ask"` (default) / `"always"` (always ask, no "don't ask again") / `"never"` (go straight to tests).
2. **Tests** for that day run (`cargo test days::dNN::`), with a ✓/✗ summary and the output of
   any that fail. Failing tests stop the loop here.
3. **Your real input?** If you say yes, it runs both parts and checks them against answers AoC has
   already accepted or rejected (including "too high/low" hints).
4. **Submit?** It offers to submit any new answer.
5. **Day done?** When both parts are correct, it congratulates you with your star count and offers to
   move to the next unfinished day.

Saving while it's asking you something just starts again with your new code. Press `r` to
re-run, `q` to quit.

## Writing solutions

Each day is a module in `src/days/`, generated from `src/days/template.rs.txt` (edit that to
change what new days start with). Each part returns `Result<Answer, String>`:

```rust
pub fn puzzle1(input: &str) -> Result<Answer, String> {
    let total: u64 = /* ... */;
    Ok(total.into())        // any integer, or a String / &str for text answers
}
```

`Answer::Unsolved` (what the template returns) is never submitted. Put the puzzle's example in
the tests at the bottom of the file. They start out `#[ignore]`d until you fill them in.

You can also run solutions without the helper: `cargo run --release -- 5 6.1 all.2`.

## Layout

```
├── src/
│   ├── main.rs             runner: `cargo run -- <specs>`
│   ├── answer.rs           the Answer type
│   └── days/
│       ├── mod.rs          day registry (`aoc day N` adds entries)
│       ├── template.rs.txt template for new days
│       └── dNN.rs          your solutions
├── inputs/                 (gitignored) NN.txt inputs + answers.toml
├── aoc_helper/             the `aoc` tool: a separate crate, built into the container
│   ├── config.toml         settings: year, URLs, watch behaviour, leaderboard
│   ├── state.toml          progress: current day, finished days (no answers; fine to commit)
│   └── .env                (gitignored) your AOC_SESSION cookie
├── Dockerfile, compose.yaml, start, .devcontainer/
```

### Configuration (`aoc_helper/config.toml`)

- `year` and `days`: which event, and how many puzzles it has (12 from 2025 on).
- `urls`: where inputs, puzzles, answers and the leaderboard are fetched from. If AoC moves
  something, fix it here.
- `check_unlock`: don't fetch puzzles before they unlock (midnight US-Eastern).
- `[watch]`: the warnings behaviour above, the debounce delay, and `poll = true` if saves aren't
  noticed (e.g. Docker Desktop on Windows with the repo on the Windows filesystem).
- `[leaderboard]`: set `id` to a private leaderboard's id to see it in `aoc status` and to count
  your stars from it. It's fetched at most every 15 minutes, as AoC asks.

If you change `year`, the previous year's inputs are moved into `inputs/<year>/`, so they're never
mixed up with the new ones.

### Credits

The original repository design was inspired by [AxlLind's](https://github.com/AxlLind/AdventOfCode2022).
