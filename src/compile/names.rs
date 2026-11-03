//! SQL names. Every identifier is double-quoted (docs/compiler.md, Names).

/// `LedgerEntry` -> `ledger_entry`, `HTTPRequest` -> `http_request`.
pub fn snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next_lower)
            {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A double-quoted identifier. Names are validated before compiling, so they never contain quotes.
pub fn q(ident: &str) -> String {
    format!("\"{ident}\"")
}

/// `"public"."<table>"` for an entity.
pub fn table(entity: &str) -> String {
    format!("\"public\".{}", q(&snake(entity)))
}

/// A SQL string literal, with single quotes doubled.
pub fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_table_names() {
        assert_eq!(snake("Account"), "account");
        assert_eq!(snake("LedgerEntry"), "ledger_entry");
        assert_eq!(snake("HTTPRequest"), "http_request");
        assert_eq!(snake("Order2Item"), "order2_item");
        assert_eq!(table("Order"), "\"public\".\"order\"");
        assert_eq!(lit("it's"), "'it''s'");
    }
}
