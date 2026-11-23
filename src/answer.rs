//! The value a puzzle part evaluates to.
//!
//! Most answers are numbers, some are text. Return whichever fits with `.into()`:
//!
//! ```ignore
//! Ok(count.into())          // any integer type
//! Ok("ABCDEFG".into())      // &str or String
//! Ok(Answer::Unsolved)      // not done yet (`aoc` won't submit these)
//! ```

use std::fmt;

#[allow(dead_code)] // not every variant gets used every year
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Unsolved,
    Int(i128),
    Text(String),
}

impl fmt::Display for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Answer::Unsolved => write!(f, "(unsolved)"),
            Answer::Int(n) => write!(f, "{n}"),
            Answer::Text(s) => write!(f, "{s}"),
        }
    }
}

macro_rules! from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Answer {
            fn from(n: $t) -> Self {
                Answer::Int(n as i128)
            }
        }
    )*};
}
from_int!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, i128);

impl From<u128> for Answer {
    fn from(n: u128) -> Self {
        match i128::try_from(n) {
            Ok(n) => Answer::Int(n),
            Err(_) => Answer::Text(n.to_string()),
        }
    }
}

impl From<String> for Answer {
    fn from(s: String) -> Self {
        Answer::Text(s)
    }
}

impl From<&str> for Answer {
    fn from(s: &str) -> Self {
        Answer::Text(s.to_owned())
    }
}

impl From<char> for Answer {
    fn from(c: char) -> Self {
        Answer::Text(c.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(Answer::from(5u64), Answer::Int(5));
        assert_eq!(Answer::from(-3i32), Answer::Int(-3));
        assert_eq!(Answer::from("AB"), Answer::Text("AB".into()));
        assert_eq!(Answer::from(42usize).to_string(), "42");
    }
}
