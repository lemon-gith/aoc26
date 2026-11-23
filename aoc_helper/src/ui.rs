//! Terminal output and single-key prompts.

use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::Stylize;
use crossterm::terminal;

pub fn ok(msg: &str) {
    println!("{} {msg}", "✓".green().bold());
}
pub fn bad(msg: &str) {
    println!("{} {msg}", "✗".red().bold());
}
pub fn warn(msg: &str) {
    println!("{} {msg}", "!".yellow().bold());
}
pub fn note(msg: &str) {
    println!("{} {msg}", "·".dark_grey());
}
pub fn heading(msg: &str) {
    println!("\n{}", msg.bold().underlined());
}

pub fn stars(n: u8) -> String {
    match n {
        2 => format!("{}", "★★".yellow().bold()),
        1 => format!("{}{}", "★".yellow().bold(), "☆".dark_grey()),
        _ => format!("{}", "☆☆".dark_grey()),
    }
}

pub fn countdown(d: chrono::TimeDelta) -> String {
    let s = d.num_seconds().max(0);
    let (days, h, m) = (s / 86400, s / 3600 % 24, s / 60 % 60);
    match (days, h) {
        (0, 0) => format!("{m}m {}s", s % 60),
        (0, _) => format!("{h}h {m}m"),
        _ => format!("{days}d {h}h {m}m"),
    }
}

/// A key the user pressed, normalised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Space,
    Quit, // Ctrl-C / Ctrl-D
}

pub enum Pressed {
    Key(Key),
    /// A watched file changed while we were waiting.
    Changed,
}

/// Wait for a key press (or a file change, if `changes` is given).
/// Falls back to reading a line when stdin isn't a terminal.
pub fn read_key(changes: Option<&Receiver<()>>) -> io::Result<Pressed> {
    io::stdout().flush()?;
    if !io::stdin().is_terminal() {
        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line)? == 0 {
            return Ok(Pressed::Key(Key::Quit));
        }
        let line = line.trim();
        return Ok(Pressed::Key(match line.chars().next() {
            None => Key::Enter,
            Some(' ') => Key::Space,
            Some(c) => Key::Char(c.to_ascii_lowercase()),
        }));
    }
    terminal::enable_raw_mode()?;
    let result = (|| loop {
        if let Some(rx) = changes
            && rx.try_recv().is_ok()
        {
            return Ok(Pressed::Changed);
        }
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event::read()?
        {
            if kind == KeyEventKind::Release {
                continue;
            }
            let ctrl = modifiers.contains(KeyModifiers::CONTROL);
            let key = match code {
                KeyCode::Char('c' | 'd') if ctrl => Key::Quit,
                KeyCode::Char(' ') => Key::Space,
                KeyCode::Char(c) => Key::Char(c.to_ascii_lowercase()),
                KeyCode::Enter => Key::Enter,
                KeyCode::Esc => Key::Esc,
                _ => continue,
            };
            return Ok(Pressed::Key(key));
        }
    })();
    terminal::disable_raw_mode()?;
    result
}

/// Ask a question and wait for one of the listed keys. Returns `None` if a watched
/// file changed first. Ctrl-C exits the program.
pub fn choose(
    question: &str,
    options: &[(Key, &str)],
    changes: Option<&Receiver<()>>,
) -> io::Result<Option<Key>> {
    // options with an empty label are hidden extra keys for the same choice
    let opts: Vec<&str> = options
        .iter()
        .map(|(_, label)| *label)
        .filter(|l| !l.is_empty())
        .collect();
    print!(
        "{} {}  {} ",
        "?".cyan().bold(),
        question.bold(),
        opts.join(" · ").dark_grey()
    );
    loop {
        match read_key(changes)? {
            Pressed::Changed => {
                println!("{}", "(file changed)".dark_grey());
                return Ok(None);
            }
            Pressed::Key(Key::Quit) => {
                println!();
                std::process::exit(130);
            }
            Pressed::Key(k) => {
                if let Some((key, label)) = options.iter().find(|(key, _)| *key == k) {
                    let echo = match (label.is_empty(), key) {
                        (false, _) => strip_markup(label),
                        (true, Key::Enter) => "enter".into(),
                        (true, Key::Esc) => "esc".into(),
                        (true, Key::Space) => "space".into(),
                        (true, _) => String::new(),
                    };
                    println!("{}", echo.dark_grey());
                    return Ok(Some(*key));
                }
            }
        }
    }
}

/// Yes/no question: Enter or y = yes, n or Esc = no.
pub fn confirm(question: &str, changes: Option<&Receiver<()>>) -> io::Result<Option<bool>> {
    let k = choose(
        question,
        &[
            (Key::Char('y'), "[y]es"),
            (Key::Enter, ""),
            (Key::Char('n'), "[n]o"),
            (Key::Esc, ""),
        ],
        changes,
    )?;
    Ok(k.map(|k| matches!(k, Key::Char('y') | Key::Enter)))
}

/// Read a whole line (cooked mode).
pub fn read_line(prompt: &str) -> io::Result<String> {
    print!("{} {} ", "?".cyan().bold(), prompt.bold());
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

fn strip_markup(label: &str) -> String {
    label.replace(['[', ']'], "")
}
