//! Day specs: `5`, `5.1`, `3-7`, `3-7.2`, `all`, `all.1`.

use anyhow::{Result, bail};

/// Expand specs into (day, part) pairs, in order, without duplicates.
/// `all` means `all_days`.
pub fn parse(specs: &[String], max_day: u8, all_days: &[u8]) -> Result<Vec<(u8, u8)>> {
    let mut out = vec![];
    for spec in specs {
        let (days, part) = match spec.split_once('.') {
            Some((d, p)) => (d, Some(p)),
            None => (spec.as_str(), None),
        };
        let parts: &[u8] = match part {
            None => &[1, 2],
            Some("1") => &[1],
            Some("2") => &[2],
            Some(p) => bail!("'{p}' in '{spec}' isn't a part (use 1 or 2)"),
        };
        let days: Vec<u8> = if days.eq_ignore_ascii_case("all") {
            all_days.to_vec()
        } else if let Some((a, b)) = days.split_once('-') {
            let (a, b) = (day(a, spec, max_day)?, day(b, spec, max_day)?);
            if a > b {
                bail!("'{spec}': range goes backwards");
            }
            (a..=b).collect()
        } else {
            vec![day(days, spec, max_day)?]
        };
        for d in days {
            for &p in parts {
                if !out.contains(&(d, p)) {
                    out.push((d, p));
                }
            }
        }
    }
    Ok(out)
}

/// Just the days from some specs (parts ignored).
pub fn days(specs: &[String], max_day: u8, all_days: &[u8]) -> Result<Vec<u8>> {
    let mut days: Vec<u8> = parse(specs, max_day, all_days)?
        .into_iter()
        .map(|(d, _)| d)
        .collect();
    days.dedup();
    Ok(days)
}

fn day(s: &str, spec: &str, max_day: u8) -> Result<u8> {
    match s.parse::<u8>() {
        Ok(d) if (1..=max_day).contains(&d) => Ok(d),
        Ok(_) => bail!("'{spec}': days go from 1 to {max_day}"),
        Err(_) => bail!("'{spec}' isn't a day spec (try 5, 5.1, 3-7, all, all.2)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(specs: &[&str]) -> Result<Vec<(u8, u8)>> {
        let specs: Vec<String> = specs.iter().map(|s| s.to_string()).collect();
        parse(&specs, 12, &[1, 2, 3])
    }

    #[test]
    fn specs() {
        assert_eq!(p(&["5"]).unwrap(), vec![(5, 1), (5, 2)]);
        assert_eq!(p(&["5.1", "6"]).unwrap(), vec![(5, 1), (6, 1), (6, 2)]);
        assert_eq!(p(&["all.2"]).unwrap(), vec![(1, 2), (2, 2), (3, 2)]);
        assert_eq!(p(&["2-3.1"]).unwrap(), vec![(2, 1), (3, 1)]);
        assert_eq!(p(&["5", "5.1"]).unwrap(), vec![(5, 1), (5, 2)]);
        assert!(p(&["13"]).is_err());
        assert!(p(&["5.3"]).is_err());
        assert!(p(&["x"]).is_err());
        assert!(p(&["4-2"]).is_err());
    }
}
