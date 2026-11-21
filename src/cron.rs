//! The five-field cron expressions of scheduled holes (docs/format.md, Scheduled holes): a
//! validator that is stricter than pg_cron's own, and the common forms in words for `explain`.

const FIELDS: [(&str, u32, u32); 5] = [
    ("minute", 0, 59),
    ("hour", 0, 23),
    ("day of month", 1, 31),
    ("month", 1, 12),
    ("day of week", 0, 7),
];

/// Check `src`. The error says which field is wrong and why.
pub fn validate(src: &str) -> Result<(), String> {
    let parts: Vec<&str> = src.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(format!(
            "has {} fields, but a schedule has five: minute, hour, day of month, month, day of week",
            parts.len()
        ));
    }
    for (i, part) in parts.iter().enumerate() {
        let (name, lo, hi) = FIELDS[i];
        field(part, lo, hi).map_err(|why| format!("field {} ({name}), `{part}`: {why}", i + 1))?;
    }
    Ok(())
}

fn field(part: &str, lo: u32, hi: u32) -> Result<(), String> {
    for item in part.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((r, s)) => (r, Some(s)),
            None => (item, None),
        };
        if let Some(s) = step {
            match s.parse::<u32>() {
                Ok(n) if n >= 1 => {}
                _ => return Err(format!("the step `{s}` must be a whole number from 1")),
            }
        }
        if range == "*" {
            continue;
        }
        let ends: Vec<&str> = range.split('-').collect();
        if ends.len() > 2 {
            return Err(format!("`{range}` is not a number or a range"));
        }
        let mut nums = Vec::new();
        for e in &ends {
            match e.parse::<u32>() {
                Ok(n) if (lo..=hi).contains(&n) => nums.push(n),
                Ok(n) => return Err(format!("{n} is out of range {lo} to {hi}")),
                Err(_) if e.chars().all(|c| c.is_ascii_alphabetic()) && !e.is_empty() => {
                    return Err(format!("`{e}` is a name, and only numbers are allowed"));
                }
                Err(_) => return Err(format!("`{e}` is not a number")),
            }
        }
        if let [a, b] = nums[..]
            && a > b
        {
            return Err(format!("the range {a}-{b} runs backwards"));
        }
        if step.is_some() && ends.len() == 1 {
            return Err(format!(
                "a step needs `*` or a range, not the single value `{range}`"
            ));
        }
    }
    Ok(())
}

/// The expression in words, for the common forms; `None` for anything else, which is then shown
/// as written. Times are UTC.
pub fn words(src: &str) -> Option<String> {
    let p: Vec<&str> = src.split_whitespace().collect();
    let [min, hour, dom, month, dow] = p[..] else {
        return None;
    };
    let num = |s: &str| s.parse::<u32>().ok();
    if (min, hour, dom, month, dow) == ("*", "*", "*", "*", "*") {
        return Some("every minute".into());
    }
    if let Some(n) = min.strip_prefix("*/").and_then(num)
        && (hour, dom, month, dow) == ("*", "*", "*", "*")
    {
        return Some(if n == 1 {
            "every minute".into()
        } else {
            format!("every {n} minutes")
        });
    }
    let m = num(min).filter(|m| *m < 60)?;
    if (hour, dom, month, dow) == ("*", "*", "*", "*") {
        return Some(if m == 0 {
            "every hour".into()
        } else {
            format!("every hour at {m} minutes past")
        });
    }
    let h = num(hour).filter(|h| *h < 24)?;
    let at = format!("{h:02}:{m:02}");
    match (dom, month, dow) {
        ("*", "*", "*") => Some(format!("every day at {at}")),
        ("*", "*", d) => {
            let day = [
                "Sunday",
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ]
            .get(num(d)? as usize)?;
            Some(format!("every {day} at {at}"))
        }
        (d, "*", "*") => {
            let d = num(d).filter(|d| (1..=31).contains(d))?;
            Some(format!("on the {d}{} of each month at {at}", ordinal(d)))
        }
        _ => None,
    }
}

fn ordinal(n: u32) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lists_ranges_and_steps() {
        for ok in [
            "0 3 * * *",
            "*/15 * * * *",
            "0 9 * * 1-5",
            "0,30 8-18/2 1,15 * 7",
            "* * * * *",
        ] {
            assert_eq!(validate(ok), Ok(()), "{ok}");
        }
    }

    #[test]
    fn says_which_field_is_wrong() {
        let msg = |s| validate(s).unwrap_err();
        assert!(msg("0 3 * *").contains("has 4 fields"));
        assert!(msg("0 25 * * *").contains("field 2 (hour)"));
        assert!(msg("0 25 * * *").contains("out of range 0 to 23"));
        assert!(msg("0 3 * * mon").contains("is a name"));
        assert!(msg("0 3 0 * *").contains("day of month"));
        assert!(msg("*/0 * * * *").contains("step"));
        assert!(msg("5/10 * * * *").contains("step needs"));
        assert!(msg("0 9-3 * * *").contains("backwards"));
    }

    #[test]
    fn spells_out_common_forms_only() {
        assert_eq!(words("0 3 * * *").as_deref(), Some("every day at 03:00"));
        assert_eq!(words("*/15 * * * *").as_deref(), Some("every 15 minutes"));
        assert_eq!(words("0 9 * * 1").as_deref(), Some("every Monday at 09:00"));
        assert_eq!(
            words("0 0 1 * *").as_deref(),
            Some("on the 1st of each month at 00:00")
        );
        assert_eq!(words("0 9 * * 1-5"), None);
        assert_eq!(words("0 0 1 6 *"), None);
    }
}
