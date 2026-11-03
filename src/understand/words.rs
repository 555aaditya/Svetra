//! Plain-English words for graph names (docs/understand.md, Wording rules).

use crate::compile::names::snake;

/// `LedgerEntry` -> "ledger entry".
pub fn singular(entity: &str) -> String {
    snake(entity).replace('_', " ")
}

/// `LedgerEntry` -> "ledger entries", `Account` -> "accounts", `Box` -> "boxes".
pub fn plural(entity: &str) -> String {
    let s = singular(entity);
    let consonant_y = s.ends_with('y')
        && !s.ends_with("ay")
        && !s.ends_with("ey")
        && !s.ends_with("oy")
        && !s.ends_with("uy");
    if consonant_y {
        format!("{}ies", &s[..s.len() - 1])
    } else if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|end| s.ends_with(end))
    {
        format!("{s}es")
    } else {
        format!("{s}s")
    }
}

/// "a" or "an" before a word, by its first letter.
pub fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// `created_at` -> "created at".
pub fn label(field: &str) -> String {
    field.replace('_', " ")
}

/// "a, b or c".
pub fn list(items: &[String], last: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., tail] => format!("{} {last} {tail}", init.join(", ")),
    }
}

/// Capitalise the first letter of a sentence.
pub fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nouns_and_lists() {
        assert_eq!(plural("Account"), "accounts");
        assert_eq!(plural("LedgerEntry"), "ledger entries");
        assert_eq!(plural("Day"), "days");
        assert_eq!(plural("Box"), "boxes");
        assert_eq!(plural("Status"), "statuses");
        assert_eq!(article("account"), "an");
        assert_eq!(article("transaction"), "a");
        assert_eq!(
            list(&["a".into(), "b".into(), "c".into()], "or"),
            "a, b or c"
        );
        assert_eq!(sentence("signed-in people"), "Signed-in people");
    }
}
