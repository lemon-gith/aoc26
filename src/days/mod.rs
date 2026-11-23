//! Day registry.
//!
//! `aoc set day N` adds entries to the `days!` list below for you,
//! but you can also add them by hand: `N => dNN,` plus a `dNN.rs` file.
//! Each day module must have `pub fn puzzle1(&str)` and `pub fn puzzle2(&str)`,
//! both returning `Result<Answer, String>` (see `template.rs.txt`).

use crate::answer::Answer;

macro_rules! days {
    ($($num:literal => $module:ident),* $(,)?) => {
        $(pub mod $module;)*

        pub fn implemented() -> &'static [u8] {
            &[$($num),*]
        }

        #[allow(unused_variables)] // `input` is unused until the first day is added
        pub fn solve(day: u8, part: u8, input: &str) -> Result<Answer, String> {
            match (day, part) {
                $(
                    ($num, 1) => $module::puzzle1(input),
                    ($num, 2) => $module::puzzle2(input),
                )*
                (_, 1 | 2) => Err(format!("day {day} isn't implemented yet")),
                _ => Err(format!("there is no part {part}")),
            }
        }
    };
}

days! {
    // aoc:days (the helper inserts new days above this line)
}
