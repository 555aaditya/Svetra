//! `svetra infer` (docs/infer.md): turn a catalog snapshot into a draft `svetra.yaml`.
//! Entities and fields only; every part that can't be translated stays as an `UNTRANSLATED`
//! comment with the original SQL.

use crate::check::MONEY_WORDS;
use crate::compile::names::snake;
use crate::inspect::{b, s};
use serde_json::Value;

/// Is this a name Svetra accepts for a field, constraint or table: `^[a-z][a-z0-9_]*$`?
pub fn is_snake(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The entity for a table: its name in PascalCase, or `None` when that wouldn't compile back to
/// the same table (docs/infer.md, Naming). No singularizing: `order_items` is `OrderItems`.
pub fn entity_name(table: &str) -> Option<String> {
    if !is_snake(table) {
        return None;
    }
    let name: String = table
        .split('_')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    (name.starts_with(|c: char| c.is_ascii_uppercase()) && snake(&name) == table).then_some(name)
}

/// What a Svetra field type is, for reading a default value.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Text,
    Enum(Vec<String>),
    Int,
    Float,
    Decimal,
    Bool,
    Date,
    Timestamp,
    Uuid,
    Json,
}

/// A column's type in Svetra's terms (docs/infer.md, Mapping).
#[derive(Debug, Clone, PartialEq)]
pub struct Mapped {
    /// The `type:` value.
    pub ty: &'static str,
    /// Extra keys that belong with the type, such as `values: [a, b]`, each as `key: value`.
    pub opts: Vec<String>,
    pub kind: Kind,
    /// Set when the compiled column is not exactly the original: says how.
    pub lossy: Option<String>,
}

fn mapped(ty: &'static str, kind: Kind) -> Mapped {
    Mapped {
        ty,
        opts: Vec::new(),
        kind,
        lossy: None,
    }
}

fn lossy(mut m: Mapped, why: &str) -> Mapped {
    m.lossy = Some(why.to_string());
    m
}

/// A YAML scalar for a string: plain when that is unambiguous, otherwise single-quoted.
pub fn scalar(s: &str) -> String {
    let plain = s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && !matches!(
            s.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n" | "inf" | "nan"
        );
    if plain {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "''"))
    }
}

/// The type of a column, or why it can't be translated. `column` is the column's snapshot row
/// (`type`, `kind`, `type_schema`, `type_name`) and `enums` is the snapshot's `enums` rows.
pub fn map_type(column: &Value, enums: &[&Value]) -> Result<Mapped, String> {
    let s = |k: &str| column[k].as_str().unwrap_or_default();
    let name = s("name");
    let money_like = name.split('_').any(|w| MONEY_WORDS.contains(&w));
    let full = s("type").to_ascii_lowercase();
    match s("kind") {
        "e" => {
            let found = enums
                .iter()
                .find(|e| e["schema"] == s("type_schema") && e["name"] == s("type_name"));
            let Some(e) = found else {
                return Err(format!("enum type {} is not in the snapshot", s("type")));
            };
            let values: Vec<String> = e["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if values.is_empty() {
                return Err("enum type with no values".into());
            }
            let list: Vec<String> = values.iter().map(|v| scalar(v)).collect();
            return Ok(lossy(
                Mapped {
                    ty: "enum",
                    opts: vec![format!("values: [{}]", list.join(", "))],
                    kind: Kind::Enum(values),
                    lossy: None,
                },
                &format!(
                    "was enum type {}, compiles to text with a check",
                    s("type_name")
                ),
            ));
        }
        "b" => {}
        other => {
            return Err(format!(
                "type {} ({})",
                s("type"),
                match other {
                    "d" => "a domain",
                    "c" => "a composite type",
                    "r" | "m" => "a range type",
                    _ => "not a base type",
                }
            ));
        }
    }
    // Arrays and extension types have a base kind too; they fall through to the error below.
    let base = full
        .split('(')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let m = match base.as_str() {
        "uuid" => mapped("uuid", Kind::Uuid),
        "text" => mapped("text", Kind::Text),
        "character varying" | "character" | "varchar" | "char" if full.contains('(') => lossy(
            mapped("text", Kind::Text),
            &format!("was {full}, the length limit is dropped"),
        ),
        "character varying" | "character" => mapped("text", Kind::Text),
        "citext" => lossy(
            mapped("text", Kind::Text),
            "was citext, comparisons become case-sensitive",
        ),
        "boolean" => mapped("bool", Kind::Bool),
        "date" => mapped("date", Kind::Date),
        // `timestamp(3) with time zone` has its precision inside the name.
        _ if full.starts_with("timestamp") && full.ends_with("with time zone") => {
            mapped("timestamp", Kind::Timestamp)
        }
        _ if full.starts_with("timestamp") && full.ends_with("without time zone") => lossy(
            mapped("timestamp", Kind::Timestamp),
            "was timestamp without time zone, compiles to timestamptz",
        ),
        "jsonb" => mapped("json", Kind::Json),
        "json" => lossy(mapped("json", Kind::Json), "was json, compiles to jsonb"),
        "smallint" | "integer" | "bigint" => {
            if base == "bigint" {
                mapped("int", Kind::Int)
            } else {
                lossy(
                    mapped("int", Kind::Int),
                    &format!("was {base}, compiles to bigint"),
                )
            }
        }
        "real" | "double precision" => {
            if money_like {
                return Err(format!(
                    "{full} named like money; Svetra refuses float for money (SVT005), choose numeric(p,s)"
                ));
            }
            if base == "real" {
                lossy(
                    mapped("float", Kind::Float),
                    "was real, compiles to double precision",
                )
            } else {
                mapped("float", Kind::Float)
            }
        }
        "numeric" => {
            let args = full
                .split_once('(')
                .and_then(|(_, rest)| rest.strip_suffix(')'));
            let Some(args) = args else {
                return Err("numeric without precision and scale".into());
            };
            let (p, sc) = args.split_once(',').unwrap_or((args, "0"));
            let (Ok(p), Ok(sc)) = (p.trim().parse::<u32>(), sc.trim().parse::<u32>()) else {
                return Err(format!("{full} can't be read"));
            };
            if p == 0 || p > 38 || sc > p {
                return Err(format!(
                    "{full}: Svetra allows 1 to 38 digits and a scale up to the precision"
                ));
            }
            Mapped {
                ty: if money_like { "money" } else { "decimal" },
                opts: vec![format!("precision: {p}"), format!("scale: {sc}")],
                kind: Kind::Decimal,
                lossy: None,
            }
        }
        _ => return Err(format!("type {}", s("type"))),
    };
    Ok(m)
}

/// What a column's default becomes (docs/infer.md, Modifiers).
#[derive(Debug, Clone, PartialEq)]
pub enum Default {
    /// `default: <value>`, already rendered as YAML.
    Value(String),
    /// `auto: now`.
    Auto,
    /// Svetra gives a uuid primary key `gen_random_uuid()` itself.
    Implied,
    /// Left as a comment with the original SQL.
    Untranslated,
}

/// The part of a deparsed default after a string literal: nothing, or a `::type` cast.
fn only_cast(rest: &str) -> bool {
    let rest = rest.trim();
    rest.is_empty()
        || rest.strip_prefix("::").is_some_and(|t| {
            !t.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || " _.\"[]()".contains(c))
        })
}

enum Lit {
    Str(String),
    Num(String),
    Bool(bool),
}

fn literal(sql: &str) -> Option<Lit> {
    let mut t = sql.trim();
    while let Some(inner) = t.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        if inner.contains(['(', ')']) {
            break;
        }
        t = inner.trim();
    }
    if let Some(body) = t.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = body.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            if c == '\'' {
                if chars.peek().is_some_and(|(_, n)| *n == '\'') {
                    out.push('\'');
                    chars.next();
                } else {
                    return only_cast(&body[i + 1..]).then_some(Lit::Str(out));
                }
            } else {
                out.push(c);
            }
        }
        return None;
    }
    match t.to_ascii_lowercase().as_str() {
        "true" => return Some(Lit::Bool(true)),
        "false" => return Some(Lit::Bool(false)),
        _ => {}
    }
    is_number(t).then(|| Lit::Num(t.to_string()))
}

fn is_number(t: &str) -> bool {
    let t = t.strip_prefix('-').unwrap_or(t);
    let (int, frac) = t.split_once('.').unwrap_or((t, "0"));
    !int.is_empty()
        && int.chars().all(|c| c.is_ascii_digit())
        && !frac.is_empty()
        && frac.chars().all(|c| c.is_ascii_digit())
}

/// Translate a column's deparsed default. `primary` is whether the column is the primary key.
pub fn translate_default(sql: &str, kind: &Kind, primary: bool) -> Default {
    let lower = sql.trim().to_ascii_lowercase();
    match kind {
        Kind::Uuid
            if primary && matches!(lower.as_str(), "gen_random_uuid()" | "uuid_generate_v4()") =>
        {
            return Default::Implied;
        }
        Kind::Timestamp if matches!(lower.as_str(), "now()" | "current_timestamp") => {
            return Default::Auto;
        }
        Kind::Date if matches!(lower.as_str(), "current_date" | "now()") => return Default::Auto,
        _ => {}
    }
    let Some(lit) = literal(sql) else {
        return Default::Untranslated;
    };
    let value = match (kind, lit) {
        (Kind::Bool, Lit::Bool(b)) => b.to_string(),
        (Kind::Int, Lit::Num(n)) if !n.contains('.') => n,
        (Kind::Int, Lit::Str(n)) if is_number(&n) && !n.contains('.') => n,
        (Kind::Float | Kind::Decimal, Lit::Num(n)) => n,
        (Kind::Float | Kind::Decimal, Lit::Str(n)) if is_number(&n) => n,
        (Kind::Text | Kind::Uuid | Kind::Date, Lit::Str(s)) => scalar(&s),
        (Kind::Enum(values), Lit::Str(s)) if values.contains(&s) => scalar(&s),
        _ => return Default::Untranslated,
    };
    Default::Value(value)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Id(String),
    Str(String),
    Num(String),
    Sym(&'static str),
}

/// Split a deparsed SQL expression into tokens, or `None` on anything outside the small subset
/// that has an equivalent in Svetra's expression language.
fn lex(sql: &str) -> Option<Vec<Tok>> {
    lex_with(sql, false)
}

/// `lex`, optionally also reading `.` (policies name `auth.uid()`; check constraints don't).
fn lex_with(sql: &str, dots: bool) -> Option<Vec<Tok>> {
    const SYMS: [&str; 18] = [
        "::", "<>", "!=", "<=", ">=", "=", "<", ">", "+", "-", "*", "/", "(", ")", "[", "]", ",",
        ".",
    ];
    let chars: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '\'' {
            let mut s = String::new();
            i += 1;
            loop {
                match chars.get(i)? {
                    '\'' if chars.get(i + 1) == Some(&'\'') => {
                        s.push('\'');
                        i += 2;
                    }
                    '\'' => break,
                    ch => {
                        s.push(*ch);
                        i += 1;
                    }
                }
            }
            i += 1;
            out.push(Tok::Str(s));
        } else if c == '"' {
            let end = chars[i + 1..].iter().position(|&x| x == '"')? + i + 1;
            out.push(Tok::Id(chars[i + 1..end].iter().collect()));
            i = end + 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            out.push(Tok::Num(chars[start..i].iter().collect()));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Tok::Id(chars[start..i].iter().collect()));
        } else {
            let rest: String = chars[i..].iter().take(2).collect();
            let sym = SYMS
                .iter()
                .find(|s| rest.starts_with(**s) && (dots || **s != "."))?;
            i += sym.len();
            out.push(Tok::Sym(sym));
        }
    }
    Some(out)
}

/// Remove `::type` casts: Svetra's expressions have no casts, and the database's own deparsing
/// adds them everywhere (`(0)::numeric`).
fn drop_casts(toks: Vec<Tok>) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if toks[i] != Tok::Sym("::") {
            out.push(toks[i].clone());
            i += 1;
            continue;
        }
        let Some(Tok::Id(ty)) = toks.get(i + 1) else {
            return None;
        };
        i += 2;
        if matches!(
            ty.as_str(),
            "double" | "character" | "timestamp" | "time" | "bit"
        ) {
            while matches!(toks.get(i), Some(Tok::Id(w))
                if matches!(w.as_str(), "precision" | "varying" | "with" | "without" | "time" | "zone"))
            {
                i += 1;
            }
        }
        if toks.get(i) == Some(&Tok::Sym("(")) {
            while toks.get(i)? != &Tok::Sym(")") {
                i += 1;
            }
            i += 1;
        }
        while toks.get(i) == Some(&Tok::Sym("[")) && toks.get(i + 1) == Some(&Tok::Sym("]")) {
            i += 2;
        }
    }
    Some(out)
}

/// Remove brackets that change nothing: around one token, or around the whole expression.
/// The database's deparsing wraps everything (`((price >= (0)::numeric))`).
fn unparen(mut toks: Vec<Tok>) -> Vec<Tok> {
    loop {
        let single = (0..toks.len().saturating_sub(2)).find(|&i| {
            toks[i] == Tok::Sym("(")
                && toks[i + 2] == Tok::Sym(")")
                && matches!(toks[i + 1], Tok::Id(_) | Tok::Num(_) | Tok::Str(_))
                && !(i > 0 && matches!(toks[i - 1], Tok::Id(_)))
        });
        if let Some(i) = single {
            toks.remove(i + 2);
            toks.remove(i);
            continue;
        }
        let wrapped =
            toks.first() == Some(&Tok::Sym("(")) && toks.last() == Some(&Tok::Sym(")")) && {
                let mut depth = 0;
                toks.iter().enumerate().all(|(i, t)| {
                    match t {
                        Tok::Sym("(") => depth += 1,
                        Tok::Sym(")") => depth -= 1,
                        _ => {}
                    }
                    if i == toks.len() - 1 {
                        depth == 0
                    } else {
                        depth > 0
                    }
                })
            };
        if !wrapped {
            return toks;
        }
        toks.pop();
        toks.remove(0);
    }
}

/// Words that mean something in SQL that Svetra's language can't say.
const NOT_TRANSLATABLE: [&str; 13] = [
    "like", "ilike", "similar", "between", "in", "case", "when", "then", "else", "end", "distinct",
    "all", "any",
];

fn is_id(t: Option<&Tok>, word: &str) -> bool {
    matches!(t, Some(Tok::Id(w)) if w.eq_ignore_ascii_case(word))
}

/// Svetra's text for a deparsed check expression, or `None` when it has no exact equivalent.
/// Only the fixed steps of docs/infer.md (Check constraints); the caller checks the fields.
fn rewrite(toks: &[Tok]) -> Option<String> {
    let mut out = String::new();
    // A space before a word or an operator, but not after an opening bracket.
    fn word(out: &mut String, s: &str) {
        if !out.is_empty() && !out.ends_with(['(', '[']) {
            out.push(' ');
        }
        out.push_str(s);
    }
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Str(s) => {
                if s.contains('\'') {
                    return None;
                }
                word(&mut out, &format!("'{s}'"));
            }
            Tok::Num(n) => word(&mut out, n),
            Tok::Id(id) => {
                let w = id.to_ascii_lowercase();
                match w.as_str() {
                    "and" | "or" | "not" | "true" | "false" | "null" => word(&mut out, &w),
                    "is" => {
                        if is_id(toks.get(i + 1), "not") && is_id(toks.get(i + 2), "null") {
                            word(&mut out, "!= null");
                            i += 2;
                        } else if is_id(toks.get(i + 1), "null") {
                            word(&mut out, "== null");
                            i += 1;
                        } else {
                            return None;
                        }
                    }
                    w if NOT_TRANSLATABLE.contains(&w) => return None,
                    _ => {
                        if toks.get(i + 1) == Some(&Tok::Sym("(")) {
                            return None; // a function call
                        }
                        word(&mut out, id);
                    }
                }
            }
            Tok::Sym("=") => {
                // `x = ANY (ARRAY[...])` is `x in [...]`.
                let any = is_id(toks.get(i + 1), "any")
                    && toks.get(i + 2) == Some(&Tok::Sym("("))
                    && is_id(toks.get(i + 3), "array")
                    && toks.get(i + 4) == Some(&Tok::Sym("["));
                if any {
                    let close = toks[i + 5..].iter().position(|t| *t == Tok::Sym("]"))? + i + 5;
                    if toks.get(close + 1) != Some(&Tok::Sym(")")) {
                        return None;
                    }
                    let mut items = Vec::new();
                    for t in toks[i + 5..close].iter().filter(|t| **t != Tok::Sym(",")) {
                        match t {
                            Tok::Str(s) if !s.contains('\'') => items.push(format!("'{s}'")),
                            Tok::Num(n) => items.push(n.clone()),
                            _ => return None,
                        }
                    }
                    word(&mut out, &format!("in [{}]", items.join(", ")));
                    i = close + 1;
                } else {
                    word(&mut out, "==");
                }
            }
            Tok::Sym("<>") => word(&mut out, "!="),
            Tok::Sym(s @ (")" | "]" | ",")) => out.push_str(s),
            Tok::Sym(s) => word(&mut out, s),
        }
        i += 1;
    }
    Some(out)
}

fn reads_only(e: &crate::expr::Expr, known: &[String]) -> bool {
    use crate::expr::Expr;
    match e {
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => true,
        Expr::Path(p) => p.len() == 1 && known.contains(&p[0]),
        Expr::Neg(x) | Expr::Not(x) => reads_only(x, known),
        Expr::Bin(_, a, b) => reads_only(a, known) && reads_only(b, known),
        Expr::In(x, items) => reads_only(x, known) && items.iter().all(|i| reads_only(i, known)),
        Expr::Call(..) => false,
    }
}

/// A check constraint's definition (`CHECK ((qty > 0))`) as a Svetra expression, when it
/// translates exactly and every column it reads is one of `fields`.
pub fn translate_check(definition: &str, fields: &[String]) -> Option<String> {
    let mut d = definition.trim();
    let head = d.get(..5).filter(|p| p.eq_ignore_ascii_case("check"))?;
    d = d[head.len()..].trim();
    while let Some(x) = [" NOT VALID", " NO INHERIT"]
        .iter()
        .find_map(|suffix| d.strip_suffix(suffix))
    {
        d = x.trim();
    }
    let text = rewrite(&unparen(drop_casts(lex(d)?)?))?;
    let parsed = crate::expr::parse(&text).ok()?;
    reads_only(&parsed, fields).then_some(text)
}

// ---- Policies (docs/infer.md, Policies) ----

/// A policy condition in the few shapes that translate exactly.
#[derive(Debug, Clone, PartialEq)]
enum Cond {
    True,
    /// What a role makes of `auth.role() = 'x'` that isn't true for it. Never rendered.
    False,
    /// `col = auth.uid()` for a `user` field `col`.
    Owner(String),
    /// `auth.role() = 'x'`, true for exactly that role.
    Role(String),
    And(Vec<Cond>),
    Or(Vec<Cond>),
}

impl Cond {
    fn render(&self) -> String {
        match self {
            Cond::True => "true".into(),
            Cond::False => "false".into(),
            Cond::Owner(c) => format!("{c} == auth.user"),
            Cond::Role(r) => format!("auth.role() = '{r}'"),
            Cond::And(v) => v
                .iter()
                .map(|c| match c {
                    Cond::Or(_) => format!("({})", c.render()),
                    _ => c.render(),
                })
                .collect::<Vec<_>>()
                .join(" and "),
            Cond::Or(v) => v.iter().map(Cond::render).collect::<Vec<_>>().join(" or "),
        }
    }

    /// The condition for requests running as `role`: `auth.role() = 'x'` is true or false, and
    /// what that makes of the rest is simplified.
    fn for_role(&self, role: &str) -> Cond {
        match self {
            Cond::Role(r) if r == role => Cond::True,
            Cond::Role(_) => Cond::False,
            Cond::And(v) => {
                let mut out = Vec::new();
                for c in v {
                    match c.for_role(role) {
                        Cond::False => return Cond::False,
                        Cond::True => {}
                        c => out.push(c),
                    }
                }
                match out.len() {
                    0 => Cond::True,
                    1 => out.remove(0),
                    _ => Cond::And(out),
                }
            }
            Cond::Or(v) => {
                let mut out = Vec::new();
                for c in v {
                    match c.for_role(role) {
                        Cond::True => return Cond::True,
                        Cond::False => {}
                        c => out.push(c),
                    }
                }
                match out.len() {
                    0 => Cond::False,
                    1 => out.remove(0),
                    _ => Cond::Or(out),
                }
            }
            c => c.clone(),
        }
    }
}

/// What a table's translated fields are, to tell an owner column from the rest.
#[derive(Debug, Default, Clone, Copy)]
pub struct Fields<'a> {
    /// `user` fields: a column compared with `auth.uid()` here is an owner check.
    pub users: &'a [String],
    /// `ref` fields and the entity each points at.
    pub refs: &'a [(String, String)],
    /// The other translated fields.
    pub plain: &'a [String],
}

/// One side of an `=`: the signed-in person's id, their role, a string, or a column, with the
/// casts applied to it.
enum Side {
    Uid,
    Role,
    Lit(String),
    Col(String),
}

struct Cursor<'a> {
    toks: &'a [Tok],
    pos: usize,
    fields: Fields<'a>,
    /// Why the first `col = auth.uid()` wasn't an owner check, if there was one.
    why: Option<String>,
}

const RESERVED: [&str; 8] = ["select", "as", "and", "or", "true", "false", "null", "not"];

impl Cursor<'_> {
    fn word(&self, w: &str) -> bool {
        matches!(self.toks.get(self.pos), Some(Tok::Id(x)) if x.eq_ignore_ascii_case(w))
    }

    fn sym(&self, s: &str) -> bool {
        matches!(self.toks.get(self.pos), Some(Tok::Sym(x)) if *x == s)
    }

    fn eat(&mut self, s: &str) -> Option<()> {
        self.sym(s).then(|| self.pos += 1)
    }

    fn list(
        &mut self,
        word: &str,
        one: fn(&mut Self) -> Option<Cond>,
        make: fn(Vec<Cond>) -> Cond,
    ) -> Option<Cond> {
        let mut v = vec![one(self)?];
        while self.word(word) {
            self.pos += 1;
            v.push(one(self)?);
        }
        Some(if v.len() == 1 { v.remove(0) } else { make(v) })
    }

    fn expr(&mut self) -> Option<Cond> {
        self.list("or", Self::and, Cond::Or)
    }

    fn and(&mut self) -> Option<Cond> {
        self.list("and", Self::atom, Cond::And)
    }

    fn atom(&mut self) -> Option<Cond> {
        let start = self.pos;
        if let Some(c) = self.eq() {
            return Some(c);
        }
        self.pos = start;
        if self.eat("(").is_some() {
            let c = self.expr()?;
            self.eat(")")?;
            return Some(c);
        }
        if self.word("true") {
            self.pos += 1;
            return Some(Cond::True);
        }
        None
    }

    /// `col = auth.uid()` or `auth.role() = 'x'`, in either order. A cast to `uuid` changes
    /// nothing; a cast to `text` compares the same values only when both sides have it.
    fn eq(&mut self) -> Option<Cond> {
        let (a, ca) = self.side()?;
        self.eat("=")?;
        let (b, cb) = self.side()?;
        match (a, b) {
            (Side::Role, Side::Lit(r)) | (Side::Lit(r), Side::Role) => {
                let text = |c: &[String]| c.iter().all(|x| x == "text");
                // Only `authenticated`: `auth.role()` is null for a request with no JWT, which
                // still runs as `anon`, so `as: anon` would let in a request the database refuses.
                (r == "authenticated" && text(&ca) && text(&cb)).then_some(Cond::Role(r))
            }
            (Side::Uid, Side::Col(c)) | (Side::Col(c), Side::Uid) => {
                let plain = ca.is_empty() || ca == ["text"];
                if !self.fields.users.contains(&c) {
                    self.why
                        .get_or_insert_with(|| not_an_owner(&c, &self.fields));
                    None
                } else {
                    (ca == cb && plain).then_some(Cond::Owner(c))
                }
            }
            _ => None,
        }
    }

    fn side(&mut self) -> Option<(Side, Vec<String>)> {
        let (base, mut casts) = if self.eat("(").is_some() {
            let sub = self.word("select");
            if sub {
                self.pos += 1;
            }
            let inner = self.side()?;
            if sub && self.word("as") {
                self.pos += 2;
            }
            self.eat(")")?;
            inner
        } else if self.word("auth") {
            self.pos += 1;
            self.eat(".")?;
            let base = if self.word("uid") {
                Side::Uid
            } else if self.word("role") {
                Side::Role
            } else {
                return None;
            };
            self.pos += 1;
            self.eat("(")?;
            self.eat(")")?;
            (base, Vec::new())
        } else if let Some(Tok::Str(text)) = self.toks.get(self.pos) {
            self.pos += 1;
            (Side::Lit(text.clone()), Vec::new())
        } else {
            let Some(Tok::Id(name)) = self.toks.get(self.pos) else {
                return None;
            };
            if RESERVED.iter().any(|w| name.eq_ignore_ascii_case(w)) {
                return None;
            }
            self.pos += 1;
            if self.sym(".") || self.sym("(") {
                return None;
            }
            (Side::Col(name.clone()), Vec::new())
        };
        while self.eat("::").is_some() {
            let Some(Tok::Id(ty)) = self.toks.get(self.pos) else {
                return None;
            };
            self.pos += 1;
            if ty != "uuid" {
                casts.push(ty.clone());
            }
        }
        Some((base, casts))
    }
}

/// Why `col = auth.uid()` isn't an owner check on this table.
fn not_an_owner(col: &str, f: &Fields) -> String {
    if let Some((_, target)) = f.refs.iter().find(|(n, _)| n == col) {
        format!("`{col}` is a reference to {target}, not a user field")
    } else if f.plain.iter().any(|n| n == col) {
        format!(
            "`{col}` is a plain uuid column, not a user field (it has no foreign key to auth.users)"
        )
    } else {
        format!(
            "`{col}` isn't a field of this entity (its column was left out, or it isn't a column)"
        )
    }
}

/// The first thing in a condition that stopped it translating, in plain words.
fn why_not(toks: &[Tok], cursor_why: Option<String>) -> String {
    let id = |i: usize| match toks.get(i) {
        Some(Tok::Id(x)) => Some(x.to_ascii_lowercase()),
        _ => None,
    };
    let sym = |i: usize, s: &str| matches!(toks.get(i), Some(Tok::Sym(x)) if *x == s);
    for i in 0..toks.len() {
        if id(i).as_deref() == Some("select") {
            let mut j = i + 1;
            while sym(j, "(") {
                j += 1;
            }
            if id(j).as_deref() != Some("auth") {
                return "a subquery".into();
            }
        }
    }
    for i in 0..toks.len() {
        let Some(word) = id(i) else { continue };
        if word == "auth" && sym(i + 1, ".") {
            match id(i + 2).as_deref() {
                Some("jwt") => return "reads a JWT claim (auth.jwt())".into(),
                Some("uid") => {}
                Some("role") => {
                    let lit = toks[i..].iter().take(12).find_map(|t| match t {
                        Tok::Str(x) => Some(x.clone()),
                        _ => None,
                    });
                    if lit.as_deref() == Some("anon") {
                        return "auth.role() = 'anon' is null, not true, for a request with no JWT, so as: anon would be wider".into();
                    }
                    if let Some(x) = lit.filter(|x| x != "authenticated") {
                        return format!("auth.role() is compared with '{x}', not 'authenticated'");
                    }
                }
                Some(f) => return format!("calls auth.{f}(), which infer doesn't translate"),
                None => {}
            }
        }
        let prev_dot = i > 0 && (sym(i - 1, ".") || sym(i - 1, "::"));
        let word_is_reserved = [
            "and", "or", "not", "in", "select", "as", "exists", "any", "is",
        ];
        if sym(i + 1, "(") && !prev_dot && !word_is_reserved.contains(&word.as_str()) {
            return format!("calls the function {word}(); infer doesn't read function bodies");
        }
        if ["current_user", "session_user", "current_role"].contains(&word.as_str()) {
            return "reads the database user".into();
        }
    }
    if let Some(why) = cursor_why {
        return why;
    }
    for t in toks {
        match t {
            Tok::Id(x) if ["not", "is", "false"].contains(&x.to_ascii_lowercase().as_str()) => {
                return format!(
                    "uses {}, which infer doesn't translate",
                    x.to_ascii_uppercase()
                );
            }
            Tok::Sym(x) if ["<>", "!=", "<", ">", "<=", ">="].contains(x) => {
                return format!("uses {x}, which infer doesn't translate");
            }
            _ => {}
        }
    }
    if toks.iter().any(|t| matches!(t, Tok::Str(_) | Tok::Num(_))) {
        return "compares with a literal, which infer doesn't translate".into();
    }
    "a condition shape infer doesn't translate".into()
}

/// A policy condition: `true`, an owner check on one of the `user` fields, `auth.role() = 'x'`,
/// or `and`/`or` of those. The reason, for anything else.
fn parse_condition(sql: &str, fields: Fields) -> Result<Cond, String> {
    if sql.len() > 400 {
        return Err("a condition too long to read".into());
    }
    let toks = lex_with(sql, true).ok_or("a condition it can't read")?;
    let mut cur = Cursor {
        toks: &toks,
        pos: 0,
        fields,
        why: None,
    };
    match cur.expr() {
        Some(c) if cur.pos == toks.len() => Ok(c),
        _ => Err(why_not(&toks, cur.why)),
    }
}

/// A database policy that translates to one Svetra policy.
#[derive(Debug, Clone, PartialEq)]
pub struct Translated {
    pub role: &'static str,
    pub actions: Vec<&'static str>,
    pub restrictive: bool,
    pub when: String,
    pub check: Option<String>,
}

/// One row of the snapshot's `policies`, as a Svetra policy, or why not. `granted(role, action)`
/// says whether the database gives that role the table privilege.
pub fn translate_policy(
    p: &Value,
    fields: Fields,
    granted: &dyn Fn(&str, &str) -> bool,
) -> Result<Translated, String> {
    let roles = crate::inspect::roles(p);
    let covered: Vec<&str> = if roles.iter().any(|r| r == "public")
        || (roles.len() == 2 && roles.iter().all(|r| r == "anon" || r == "authenticated"))
    {
        vec!["anon", "authenticated"]
    } else if roles == ["authenticated"] {
        vec!["authenticated"]
    } else if roles == ["anon"] {
        vec!["anon"]
    } else {
        return Err(format!("roles {}", roles.join(", ")));
    };
    let cond = |key: &str| p[key].as_str().map(|sql| parse_condition(sql, fields));
    let (using, check) = (cond("using"), cond("check"));
    let some_sql = |key: &str| p[key].as_str().is_some();
    let cmd = s(p, "cmd").to_ascii_uppercase();
    // The condition that decides who gets in, and the one on the new row, per command.
    let (mut actions, primary, secondary): (Vec<&'static str>, Cond, Option<Cond>) = match cmd
        .as_str()
    {
        "SELECT" | "DELETE" if !some_sql("check") => {
            let a = if cmd == "SELECT" { "select" } else { "delete" };
            (vec![a], using.ok_or("no using")??, None)
        }
        "INSERT" if !some_sql("using") => (vec!["insert"], check.ok_or("no with check")??, None),
        "UPDATE" => (
            vec!["update"],
            using.ok_or("an update policy with no using")??,
            check.transpose()?,
        ),
        "ALL" => (
            vec!["select", "insert", "update", "delete"],
            using.ok_or("no using")??,
            check.transpose()?,
        ),
        _ => return Err(format!("command {cmd}")),
    };
    let restrictive = s(p, "permissive").eq_ignore_ascii_case("restrictive");
    // `auth.role()` is what the role says it is, so each role gets its own reading of the
    // condition, and a role it is `false` for was never let in: it drops out.
    let mut kept: Vec<(&str, String, Option<String>)> = Vec::new();
    for r in &covered {
        let u = primary.for_role(r);
        if u == Cond::False {
            continue;
        }
        let k = match &secondary {
            None => None,
            Some(k) => match k.for_role(r) {
                Cond::False => {
                    return Err(format!(
                        "the with check is never true for {r}, so no change would pass it"
                    ));
                }
                k => Some(k.render()).filter(|k| *k != u.render()),
            },
        };
        if cmd == "ALL" && k.is_some() {
            return Err("FOR ALL with a with check that differs from using".into());
        }
        kept.push((*r, u.render(), k));
    }
    let Some((_, when, check)) = kept.first().cloned() else {
        return Err(format!(
            "auth.role() makes the condition false for {}, so it never lets anyone in",
            covered.join(" and ")
        ));
    };
    if kept.iter().any(|(_, w, k)| *w != when || *k != check) {
        return Err("the condition differs between anon and authenticated".into());
    }
    if restrictive && kept.len() < covered.len() {
        return Err(
            "a restrictive policy that is false for a role it applies to: dropping that role would widen access"
                .into(),
        );
    }
    let covered: Vec<&str> = kept.iter().map(|(r, _, _)| *r).collect();
    let role = match covered[..] {
        ["anon", "authenticated"] => "anyone",
        ["authenticated"] => "authenticated",
        _ => "anon",
    };
    actions.retain(|a| covered.iter().all(|r| granted(r, a)));
    if actions.is_empty() {
        return Err("the database withholds the table privilege from this role".into());
    }
    Ok(Translated {
        role,
        actions,
        restrictive,
        when,
        check,
    })
}

/// A drafted `svetra.yaml` and what it holds, for the summary line.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub yaml: String,
    pub entities: usize,
    pub fields: usize,
    /// Things left as `UNTRANSLATED` comments: columns, defaults, constraints, indexes, keys, tables.
    pub untranslated: usize,
    pub critical: usize,
}

fn rows<'a>(snap: &'a Value, key: &str) -> Vec<&'a Value> {
    snap[key].as_array().into_iter().flatten().collect()
}

fn on<'a>(rows: &[&'a Value], table: &str) -> Vec<&'a Value> {
    rows.iter()
        .filter(|r| s(r, "schema") == "public" && s(r, "table") == table)
        .copied()
        .collect()
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|x| x.as_str().unwrap_or_default().to_string())
        .collect()
}

/// SQL on one line, so a comment can't end early.
fn one_line(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn untranslated(counts: &mut Counts, text: String) -> String {
    counts.untranslated += 1;
    format!("# UNTRANSLATED {}", one_line(&text))
}

struct Plan {
    table: String,
    entity: String,
    pk: String,
    pk_uuid: bool,
    /// The key is a person's id: a uuid with a foreign key to `auth.users (id)`.
    pk_user: bool,
}

struct Counts {
    fields: usize,
    untranslated: usize,
    policies: usize,
    policies_done: usize,
}

/// A constraint put in the draft, so a validation error can send it back to a comment.
struct Placed {
    entity: String,
    name: String,
    table: String,
    original: String,
}

/// Which parts of an entity can't be said, and the lines that say so.
struct Entity {
    text: String,
    placed: Vec<Placed>,
    /// The `policies:` blocks this entity's policies became.
    policies: Vec<String>,
}

/// Draft a `svetra.yaml` from a snapshot taken with `inspect::infer_query`.
pub fn infer(snap: &Value) -> Result<Draft, String> {
    if !snap["columns"].is_array() {
        return Err(
            "That snapshot predates `infer`. Run `svetra infer --print-query` and take it again."
                .into(),
        );
    }
    let mut rejected: Vec<(String, String)> = Vec::new();
    // A constraint that builds but doesn't type-check goes back to a comment, then we build again.
    for _ in 0..20 {
        let (draft, placed) = build(snap, &rejected)?;
        let bad: Vec<(String, String)> = crate::report::validate(&draft.yaml)
            .iter()
            .filter(|d| d.code == crate::diag::Code::Svt003)
            .filter_map(|d| {
                let path = d.path.strip_prefix("entities.")?;
                let (entity, rest) = path.split_once(".constraints.")?;
                let name = rest.split('.').next()?;
                placed
                    .iter()
                    .find(|p| p.entity == entity && p.name == name)
                    .map(|p| (p.table.clone(), p.original.clone()))
            })
            .filter(|b| !rejected.contains(b))
            .collect();
        if bad.is_empty() {
            return Ok(draft);
        }
        rejected.extend(bad);
    }
    build(snap, &rejected).map(|(d, _)| d)
}

fn build(snap: &Value, rejected: &[(String, String)]) -> Result<(Draft, Vec<Placed>), String> {
    let columns = rows(snap, "columns");
    let constraints = rows(snap, "constraints");
    let indexes = rows(snap, "indexes");
    let enums = rows(snap, "enums");
    let findings = crate::inspect::findings(snap);
    let mut counts = Counts {
        fields: 0,
        untranslated: 0,
        policies: 0,
        policies_done: 0,
    };
    let access = Access {
        tables: rows(snap, "tables"),
        policies: rows(snap, "policies"),
    };
    let mut left_out: Vec<String> = Vec::new();
    let mut plans: Vec<Plan> = Vec::new();
    let tables = rows(snap, "tables");
    for t in &tables {
        let (schema, name) = (s(t, "schema"), s(t, "name"));
        let mut out = |why: String| {
            counts.untranslated += 1;
            left_out.push(format!(
                "# UNTRANSLATED table {schema}.{name}: {}",
                one_line(&why)
            ));
        };
        if schema != "public" {
            out("only tables in public become entities".into());
            continue;
        }
        let cols = on(&columns, name);
        if cols.is_empty() {
            out("a partition of another table, or a table with no columns".into());
            continue;
        }
        let Some(entity) = entity_name(name) else {
            out("its name doesn't compile back to the same table".into());
            continue;
        };
        let pks: Vec<_> = on(&constraints, name)
            .into_iter()
            .filter(|c| s(c, "kind") == "p")
            .collect();
        let [pk] = pks[..] else {
            out("no primary key".into());
            continue;
        };
        let pk_cols = names(&pk["columns"]);
        let [pk_col] = &pk_cols[..] else {
            out(format!(
                "an entity needs one primary field: {} {}",
                s(pk, "name"),
                s(pk, "definition")
            ));
            continue;
        };
        let Some(col) = cols.iter().find(|c| s(c, "name") == pk_col) else {
            continue;
        };
        let problem = if !is_snake(pk_col) {
            Some("the primary key's name isn't snake_case".to_string())
        } else if b(col, "generated") {
            Some("the primary key is a generated column".to_string())
        } else {
            map_type(col, &enums).err()
        };
        if let Some(why) = problem {
            out(format!("primary key {pk_col}: {why}"));
            continue;
        }
        let uuid = map_type(col, &enums).is_ok_and(|m| m.kind == Kind::Uuid);
        let pk_user = uuid
            && on(&constraints, name).iter().any(|c| {
                s(c, "kind") == "f"
                    && names(&c["columns"]) == [pk_col.as_str()]
                    && (s(c, "ref_schema"), s(c, "ref_table")) == ("auth", "users")
                    && names(&c["ref_columns"]) == ["id"]
            });
        plans.push(Plan {
            table: name.to_string(),
            entity,
            pk: pk_col.clone(),
            pk_uuid: uuid,
            pk_user,
        });
    }
    if plans.is_empty() {
        return Err("Nothing to infer: no table in public could become an entity.".into());
    }
    plans.sort_by(|a, b| a.table.cmp(&b.table));
    let mut used: Vec<String> = Vec::new();
    let mut used_policies: Vec<String> = Vec::new();
    let mut placed = Vec::new();
    let mut body = String::new();
    let mut policy_blocks: Vec<String> = Vec::new();
    for plan in &plans {
        let e = entity(
            plan,
            &plans,
            &columns,
            &constraints,
            &indexes,
            &enums,
            &findings,
            rejected,
            &access,
            &mut used,
            &mut used_policies,
            &mut counts,
        );
        body += &e.text;
        placed.extend(e.placed);
        policy_blocks.extend(e.policies);
    }
    if !policy_blocks.is_empty() {
        body += "\npolicies:\n";
        for block in &policy_blocks {
            body += block;
        }
    }
    if !left_out.is_empty() {
        body += "\n# Tables left out of the entities:\n";
        for l in &left_out {
            body += l;
            body.push('\n');
        }
    }
    let count =
        |sev: crate::inspect::Severity| findings.iter().filter(|f| f.severity == sev).count();
    let (critical, warnings) = (
        count(crate::inspect::Severity::Critical),
        count(crate::inspect::Severity::Warning),
    );
    let mut head = String::from(
        "# Drafted by `svetra infer` from a database snapshot. Review before adopting.\n\
# An entity is `private: true` only when none of its policies translated; the draft is never\n\
# wider than the database (docs/infer.md, Policies).\n",
    );
    if counts.policies > 0 {
        head += &format!(
            "# Policies: {} of {} translated.\n",
            counts.policies_done, counts.policies
        );
    }
    head += &format!(
        "# {} entities from {} tables. Skipped: {}, {}.\n",
        plans.len(),
        tables.len(),
        plural(rows(snap, "views").len(), "view"),
        plural(rows(snap, "functions").len(), "function")
    );
    head += &format!(
        "# {} things left as UNTRANSLATED (search for UNTRANSLATED).\n#\n",
        counts.untranslated
    );
    if findings.is_empty() {
        head += "# svetra check found nothing.\n";
    } else {
        head += &format!(
            "# svetra check found {critical} critical, {warnings} warning{} and {} note{}:\n",
            if warnings == 1 { "" } else { "s" },
            count(crate::inspect::Severity::Info),
            if count(crate::inspect::Severity::Info) == 1 {
                ""
            } else {
                "s"
            },
        );
        for f in &findings {
            head += &format!(
                "#   {:<9} {}: {}\n",
                f.severity.word(),
                f.object,
                one_line(&f.message)
            );
        }
    }
    head += "# yaml-language-server: $schema=https://raw.githubusercontent.com/555aaditya/Svetra/main/schema/svetra.schema.json\n";
    head += "svetra: \"0.1\"\nsystem: App   # rename\n\nentities:\n";
    Ok((
        Draft {
            yaml: head + &body,
            entities: plans.len(),
            fields: counts.fields,
            untranslated: counts.untranslated,
            critical,
        },
        placed,
    ))
}

/// The action after `key` (` ON DELETE `, ` ON UPDATE `) in a foreign key's definition.
fn fk_action(definition: &str, key: &str) -> Option<String> {
    let rest = &definition[definition.find(key)? + key.len()..];
    let end = [
        " ON ",
        " DEFERRABLE",
        " NOT DEFERRABLE",
        " NOT VALID",
        " MATCH",
    ]
    .iter()
    .filter_map(|w| rest.find(w))
    .min()
    .unwrap_or(rest.len());
    Some(one_line(rest[..end].trim()))
}

/// What a foreign key's `on delete` and `on update` become: for a `ref` an `on_delete` option,
/// and for what Svetra can't say a `lossy` note (docs/infer.md, `on delete`).
fn foreign_key_note(
    definition: &str,
    is_ref: bool,
    required: bool,
) -> (Option<String>, Vec<String>) {
    let mut option = None;
    let mut notes = Vec::new();
    match fk_action(definition, " ON DELETE ").as_deref() {
        None | Some("NO ACTION" | "RESTRICT") => {}
        Some("CASCADE") if is_ref => option = Some("on_delete: cascade".to_string()),
        Some("SET NULL") if is_ref && !required => option = Some("on_delete: set_null".to_string()),
        Some(action) => notes.push(format!(
            "lossy: the foreign key had ON DELETE {action}, {}",
            if !is_ref {
                "a user field has none"
            } else if required {
                "and on_delete: set_null isn't allowed on a required field, so the delete is refused"
            } else {
                "which Svetra can't write"
            }
        )),
    }
    if let Some(action) = fk_action(definition, " ON UPDATE ")
        && !matches!(action.as_str(), "NO ACTION" | "RESTRICT")
    {
        notes.push(format!(
            "lossy: the foreign key had ON UPDATE {action}, Svetra writes none"
        ));
    }
    (option, notes)
}

/// The snapshot's table and policy rows, for the policies of each entity.
struct Access<'a> {
    tables: Vec<&'a Value>,
    policies: Vec<&'a Value>,
}

/// What the policies of one table become: `policies:` blocks, comments for the entity, and
/// whether the entity stays `private`.
struct TablePolicies {
    blocks: Vec<String>,
    notes: Vec<String>,
    comment: Option<String>,
    private: bool,
}

/// The policy as `CREATE POLICY`, rebuilt from its `pg_policies` row, for an `UNTRANSLATED` comment.
fn original_policy(table: &str, p: &Value) -> String {
    let roles = crate::inspect::roles(p);
    let mut sql = format!(
        "CREATE POLICY {} ON public.{table} AS {} FOR {} TO {}",
        s(p, "name"),
        s(p, "permissive"),
        s(p, "cmd"),
        if roles.is_empty() {
            "PUBLIC".to_string()
        } else {
            roles.join(", ")
        }
    );
    for (key, clause) in [("using", "USING"), ("check", "WITH CHECK")] {
        if let Some(c) = p[key].as_str() {
            sql += &format!(" {clause} ({c})");
        }
    }
    sql
}

/// Translate a table's policies, never widening access (docs/infer.md, Policies).
fn table_policies(
    plan: &Plan,
    fields: Fields,
    access: &Access,
    used: &mut Vec<String>,
    counts: &mut Counts,
) -> TablePolicies {
    let table = plan.table.as_str();
    let rows = on(&access.policies, table);
    let rls = access
        .tables
        .iter()
        .find(|t| s(t, "schema") == "public" && s(t, "name") == table)
        .is_some_and(|t| b(t, "rls"));
    let granted = |role: &str, action: &str| {
        access
            .tables
            .iter()
            .find(|t| s(t, "schema") == "public" && s(t, "name") == table)
            .is_some_and(|t| b(&t[role], action))
    };
    let mut out = TablePolicies {
        blocks: Vec::new(),
        notes: Vec::new(),
        comment: None,
        private: true,
    };
    counts.policies += rows.len();
    if !rls {
        out.comment = Some(
            "row-level security is off here, so the database has no access rule (see the check finding). No policy is invented; add one deliberately."
                .into(),
        );
    }
    let mut results: Vec<(&Value, Result<Translated, String>)> = rows
        .iter()
        .map(|p| {
            let r = if rls {
                translate_policy(p, fields, &granted)
            } else {
                Err("row-level security is off, so the policy isn't enforced".into())
            };
            (*p, r)
        })
        .collect();
    let restrictive_lost = results
        .iter()
        .any(|(p, r)| r.is_err() && s(p, "permissive").eq_ignore_ascii_case("restrictive"));
    let permissive_kept = results
        .iter()
        .any(|(_, r)| r.as_ref().is_ok_and(|t| !t.restrictive));
    if restrictive_lost || !permissive_kept {
        let why = if restrictive_lost {
            "a restrictive policy on this table didn't translate, and without it this policy would be wider than the database"
        } else {
            "no permissive policy on this table translated, and a restrictive one alone lets nobody in"
        };
        for (_, r) in results.iter_mut() {
            if r.is_ok() {
                *r = Err(why.into());
            }
        }
    }
    let done = results.iter().filter(|(_, r)| r.is_ok()).count();
    for (p, r) in &results {
        match r {
            Ok(t) => {
                let name = s(p, "name");
                let base = if is_snake(name) {
                    name.to_string()
                } else {
                    format!("{table}_policy")
                };
                let mut final_name = base.clone();
                let mut n = 2;
                while used.contains(&final_name) {
                    final_name = if n == 2 && is_snake(name) {
                        format!("{table}_{name}")
                    } else {
                        format!("{base}_{n}")
                    };
                    n += 1;
                }
                used.push(final_name.clone());
                let mut block = format!(
                    "  {final_name}:\n    on: {}\n    as: {}\n    allow: [{}]\n",
                    plan.entity,
                    t.role,
                    t.actions.join(", ")
                );
                if t.restrictive {
                    block += "    restrictive: true\n";
                }
                let quote =
                    |e: &str| format!("\"{}\"", e.replace('\\', "\\\\").replace('"', "\\\""));
                block += &format!("    when: {}\n", quote(&t.when));
                if let Some(k) = &t.check {
                    block += &format!("    check: {}\n", quote(k));
                }
                out.blocks.push(block);
            }
            Err(why) => {
                out.notes.push(untranslated(
                    counts,
                    format!(
                        "policy {}: {} -- {why}",
                        s(p, "name"),
                        original_policy(table, p)
                    ),
                ));
            }
        }
    }
    counts.policies_done += done;
    out.private = done == 0;
    if rls && !rows.is_empty() {
        out.comment = Some(if done == 0 {
            format!(
                "policies: none of {} translated. The database lets some people in; this draft keeps everyone out (private) until you port a policy.",
                rows.len()
            )
        } else if done < rows.len() {
            format!(
                "policies: {done} of {} translated. The draft is NARROWER than the database: whatever an UNTRANSLATED policy below lets in, this draft keeps out, until you port it.",
                rows.len()
            )
        } else {
            return out;
        });
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn entity(
    plan: &Plan,
    plans: &[Plan],
    columns: &[&Value],
    constraints: &[&Value],
    indexes: &[&Value],
    enums: &[&Value],
    findings: &[crate::inspect::Finding],
    rejected: &[(String, String)],
    access: &Access,
    used: &mut Vec<String>,
    used_policies: &mut Vec<String>,
    counts: &mut Counts,
) -> Entity {
    let table = plan.table.as_str();
    let cols = on(columns, table);
    let cons = on(constraints, table);
    let idxs = on(indexes, table);
    let mut notes: Vec<String> = Vec::new(); // entity-level UNTRANSLATED lines
    let mut lines: Vec<(String, Vec<String>)> = Vec::new(); // (key and spec, notes) or ("", [comment])
    let mut fields: Vec<String> = Vec::new();
    let mut consumed: Vec<String> = Vec::new();
    let mut users: Vec<String> = Vec::new();
    let mut refs: Vec<(String, String)> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    for col in &cols {
        let name = s(col, "name");
        let ty = s(col, "type");
        let is_pk = name == plan.pk;
        if !is_snake(name) {
            let c = untranslated(
                counts,
                format!("column \"{name}\" {ty}: its name isn't snake_case"),
            );
            lines.push((String::new(), vec![c]));
            continue;
        }
        if b(col, "generated") {
            let c = untranslated(
                counts,
                format!(
                    "column {name} {ty}: a generated column, default {}",
                    s(col, "default")
                ),
            );
            lines.push((String::new(), vec![c]));
            continue;
        }
        let fk = cons.iter().find(|c| {
            s(c, "kind") == "f"
                && names(&c["columns"]) == [name]
                && names(&c["ref_columns"]).len() == 1
        });
        let mut mapped = None;
        let mut note: Vec<String> = Vec::new();
        if let (Some(fk), "uuid") = (fk, ty) {
            let (rs, rt) = (s(fk, "ref_schema"), s(fk, "ref_table"));
            let rc = names(&fk["ref_columns"]).remove(0);
            let def = s(fk, "definition");
            let required = b(col, "not_null");
            let target = plans
                .iter()
                .find(|p| rs == "public" && p.table == rt && p.pk == rc && p.pk_uuid);
            let user = Mapped {
                ty: "user",
                opts: vec![],
                kind: Kind::Uuid,
                lossy: None,
            };
            if rs == "auth" && rt == "users" && rc == "id" {
                mapped = Some(user);
                note.extend(foreign_key_note(def, false, required).1);
            } else if let (Some(t), false) = (target, is_pk) {
                if t.pk_user {
                    // A person's id held in a column that points at the profile table.
                    mapped = Some(user);
                    let mut why = vec![format!(
                        "lossy: was a foreign key to {rs}.{rt} ({rc}), now one to auth.users: it accepts a person with no row in {rt}"
                    )];
                    why.extend(
                        foreign_key_note(def, false, required)
                            .1
                            .iter()
                            .map(|n| n.trim_start_matches("lossy: ").to_string()),
                    );
                    note.push(why.join("; "));
                } else {
                    let (opt, notes) = foreign_key_note(def, true, required);
                    let mut opts = vec![format!("entity: {}", t.entity)];
                    opts.extend(opt);
                    mapped = Some(Mapped {
                        ty: "ref",
                        opts,
                        kind: Kind::Uuid,
                        lossy: None,
                    });
                    note.extend(notes);
                }
            }
            if mapped.is_some() {
                consumed.push(s(fk, "name").to_string());
            }
        }
        let is_link = mapped.is_some();
        let mapped = match mapped {
            Some(m) => m,
            None => match map_type(col, enums) {
                Ok(m) => m,
                Err(why) => {
                    let c = untranslated(counts, format!("column {name} {ty}: {why}"));
                    lines.push((String::new(), vec![c]));
                    continue;
                }
            },
        };
        if let Some(l) = &mapped.lossy {
            note.push(format!("lossy: {l}"));
        }
        let mut spec = vec![format!("type: {}", mapped.ty)];
        spec.extend(mapped.opts.iter().cloned());
        if is_pk {
            spec.push("primary: true".into());
        } else if b(col, "not_null") {
            spec.push("required: true".into());
        }
        let single_unique = cons
            .iter()
            .filter(|c| s(c, "kind") == "u" && names(&c["columns"]) == [name])
            .map(|c| s(c, "name"))
            .chain(
                idxs.iter()
                    .filter(|i| {
                        b(i, "unique")
                            && !b(i, "partial")
                            && s(i, "method") == "btree"
                            && simple(i) == Some(vec![name.to_string()])
                    })
                    .map(|i| s(i, "name")),
            )
            .next();
        if let (Some(u), false) = (single_unique, is_pk) {
            spec.push("unique: true".into());
            consumed.push(u.to_string());
        }
        if let Some(d) = col["default"].as_str() {
            match if is_link {
                Default::Untranslated
            } else {
                translate_default(d, &mapped.kind, is_pk)
            } {
                Default::Value(v) => spec.push(format!("default: {v}")),
                Default::Auto => spec.push("auto: now".into()),
                Default::Implied => {}
                Default::Untranslated => {
                    note.push(untranslated(counts, format!("default: {d}")));
                }
            }
        }
        if let Some(i) = col["identity"].as_str() {
            note.push(untranslated(
                counts,
                format!("identity ({i}): Svetra has no auto-increment, inserts must give a value"),
            ));
        }
        counts.fields += 1;
        fields.push(name.to_string());
        match mapped.ty {
            "user" => users.push(name.to_string()),
            "ref" => {
                let target = mapped.opts.iter().find_map(|o| o.strip_prefix("entity: "));
                refs.push((name.to_string(), target.unwrap_or_default().to_string()));
            }
            _ => plain.push(name.to_string()),
        }
        lines.push((format!("{name}: {{ {} }}", spec.join(", ")), note));
    }
    // Constraints, unique and indexes, now that the fields are known.
    let mut expressions: Vec<(String, String)> = Vec::new();
    let mut placed = Vec::new();
    let mut unique: Vec<Vec<String>> = Vec::new();
    let mut index_lists: Vec<Vec<String>> = Vec::new();
    for c in &cons {
        let (name, def) = (s(c, "name"), s(c, "definition"));
        match s(c, "kind") {
            "c" => {
                let hit = if rejected.contains(&(table.to_string(), def.to_string())) {
                    None
                } else {
                    translate_check(def, &fields)
                };
                match hit {
                    Some(expr) => {
                        let base = if is_snake(name) {
                            name.to_string()
                        } else {
                            format!("{table}_check")
                        };
                        let mut final_name = base.clone();
                        let mut n = 2;
                        while used.contains(&final_name) {
                            final_name = format!("{base}_{n}");
                            n += 1;
                        }
                        used.push(final_name.clone());
                        placed.push(Placed {
                            entity: plan.entity.clone(),
                            name: final_name.clone(),
                            table: table.to_string(),
                            original: def.to_string(),
                        });
                        expressions.push((final_name, expr));
                    }
                    None => notes.push(untranslated(counts, format!("check {name}: {def}"))),
                }
            }
            "u" if !consumed.iter().any(|x| x == name) => {
                let cols = names(&c["columns"]);
                if cols.iter().all(|x| fields.contains(x)) {
                    unique.push(cols);
                } else {
                    notes.push(untranslated(counts, format!("unique {name}: {def}")));
                }
            }
            "f" if !consumed.iter().any(|x| x == name) => {
                notes.push(untranslated(counts, format!("foreign key {name}: {def}")));
            }
            _ => {}
        }
    }
    for i in &idxs {
        let (name, def) = (s(i, "name"), s(i, "definition"));
        if consumed.iter().any(|x| x == name) {
            continue;
        }
        let plain = !b(i, "partial")
            && s(i, "method") == "btree"
            && ![" DESC", " NULLS ", " COLLATE ", " INCLUDE "]
                .iter()
                .any(|w| def.contains(w));
        match simple(i).filter(|cols| plain && cols.iter().all(|x| fields.contains(x))) {
            Some(cols) if b(i, "unique") => unique.push(cols),
            Some(cols) => index_lists.push(cols),
            None => notes.push(untranslated(counts, format!("index {name}: {def}"))),
        }
    }
    let fields_by_kind = Fields {
        users: &users,
        refs: &refs,
        plain: &plain,
    };
    let pol = table_policies(plan, fields_by_kind, access, used_policies, counts);
    notes.extend(pol.notes);
    // Render.
    let width = lines
        .iter()
        .filter_map(|(k, _)| k.split_once(": ").map(|(n, _)| n.len()))
        .max()
        .unwrap_or(0);
    let mut text = format!("  {}:\n", plan.entity);
    for f in findings
        .iter()
        .filter(|f| f.object == format!("public.{table}"))
    {
        text += &format!(
            "    # check: {}, {}: {}\n",
            f.severity.word(),
            f.kind,
            one_line(&f.message)
        );
    }
    if let Some(c) = &pol.comment {
        text += &format!("    # {c}\n");
    }
    if pol.private {
        text += "    private: true\n";
    }
    text += "    fields:\n";
    for (spec, note) in &lines {
        match spec.split_once(": ") {
            Some((n, rest)) => {
                let pad = " ".repeat(width - n.len());
                let tail = if note.is_empty() {
                    String::new()
                } else {
                    format!(
                        "   # {}",
                        note.iter()
                            .map(|x| x.trim_start_matches("# ").to_string())
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                };
                text += &format!("      {n}:{pad} {rest}{tail}\n");
            }
            None => {
                for c in note {
                    text += &format!("      {c}\n");
                }
            }
        }
    }
    if !expressions.is_empty() {
        text += "    constraints:\n";
        for (n, e) in &expressions {
            text += &format!(
                "      {n}: \"{}\"\n",
                e.replace('\\', "\\\\").replace('"', "\\\"")
            );
        }
    }
    for (key, lists) in [("unique", &unique), ("indexes", &index_lists)] {
        if !lists.is_empty() {
            text += &format!("    {key}:\n");
            for l in lists {
                text += &format!("      - [{}]\n", l.join(", "));
            }
        }
    }
    for n in &notes {
        text += &format!("    {n}\n");
    }
    Entity {
        text,
        placed,
        policies: pol.blocks,
    }
}

/// An index's columns when every position is a plain column.
fn simple(index: &Value) -> Option<Vec<String>> {
    index["columns"]
        .as_array()?
        .iter()
        .map(|c| c.as_str().map(String::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn col(name: &str, ty: &str) -> Value {
        json!({ "name": name, "type": ty, "kind": "b", "type_schema": "pg_catalog", "type_name": "x" })
    }

    fn ty(name: &str, t: &str) -> Result<(String, Vec<String>, bool), String> {
        map_type(&col(name, t), &[]).map(|m| (m.ty.to_string(), m.opts, m.lossy.is_some()))
    }

    fn exact(t: &str) -> String {
        let (ty, opts, lossy) = ty("x", t).unwrap();
        assert!(opts.is_empty() && !lossy, "{t} should be exact");
        ty
    }

    #[test]
    fn exact_types() {
        assert_eq!(exact("uuid"), "uuid");
        assert_eq!(exact("text"), "text");
        assert_eq!(exact("boolean"), "bool");
        assert_eq!(exact("date"), "date");
        assert_eq!(exact("timestamp with time zone"), "timestamp");
        assert_eq!(exact("timestamp(3) with time zone"), "timestamp");
        assert_eq!(exact("jsonb"), "json");
        assert_eq!(exact("bigint"), "int");
        assert_eq!(exact("double precision"), "float");
    }

    #[test]
    fn widened_types_say_so() {
        for t in [
            "integer",
            "smallint",
            "real",
            "character varying(255)",
            "character(3)",
            "citext",
            "json",
            "timestamp without time zone",
        ] {
            assert!(ty("x", t).unwrap().2, "{t} should carry a lossy note");
        }
        assert_eq!(ty("x", "integer").unwrap().0, "int");
        assert_eq!(ty("x", "character varying").unwrap().0, "text");
        assert!(
            !ty("x", "character varying").unwrap().2,
            "no limit, no loss"
        );
    }

    #[test]
    fn numeric_is_decimal_or_money_by_name() {
        let (t, o, _) = ty("quantity", "numeric(10,3)").unwrap();
        assert_eq!(
            (t.as_str(), o),
            ("decimal", vec!["precision: 10".into(), "scale: 3".into()])
        );
        assert_eq!(ty("price", "numeric(12,2)").unwrap().0, "money");
        assert_eq!(ty("unit_price", "numeric(12,2)").unwrap().0, "money");
        assert_eq!(ty("priceless", "numeric(12,2)").unwrap().0, "decimal");
        assert_eq!(ty("n", "numeric(5)").unwrap().1[1], "scale: 0");
    }

    #[test]
    fn what_it_cannot_translate_is_an_error_not_a_guess() {
        assert!(ty("rating", "numeric").is_err());
        assert!(ty("rating", "numeric(40,2)").is_err());
        assert!(ty("tags", "text[]").is_err());
        assert!(ty("blob", "bytea").is_err());
        assert!(ty("ip", "inet").is_err());
        assert!(
            ty("total", "double precision").is_err(),
            "float money is SVT005"
        );
        assert!(ty("weight", "real").is_ok());
        let mut domain = col("email", "email_address");
        domain["kind"] = json!("d");
        assert!(map_type(&domain, &[]).is_err());
    }

    #[test]
    fn enums_keep_their_values() {
        let mut c = col("status", "order_status");
        c["kind"] = json!("e");
        c["type_schema"] = json!("public");
        c["type_name"] = json!("order_status");
        let e =
            json!({ "schema": "public", "name": "order_status", "values": ["open", "on", "it's"] });
        let m = map_type(&c, &[&e]).unwrap();
        assert_eq!(m.ty, "enum");
        assert_eq!(m.opts, vec!["values: [open, 'on', 'it''s']"]);
        assert!(m.lossy.is_some());
        assert!(map_type(&c, &[]).is_err(), "an enum the snapshot lacks");
    }

    #[test]
    fn entity_names_compile_back_to_the_table() {
        assert_eq!(entity_name("order_items").as_deref(), Some("OrderItems"));
        assert_eq!(entity_name("profiles").as_deref(), Some("Profiles"));
        assert_eq!(entity_name("t2_x").as_deref(), Some("T2X"));
        for bad in ["Order", "a__b", "_x", "2fa", "t_2x", "my-table", ""] {
            assert_eq!(entity_name(bad), None, "{bad}");
        }
    }
}
#[cfg(test)]
mod sql_tests {
    use super::*;

    fn fields(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn check(def: &str) -> Option<String> {
        translate_check(
            def,
            &fields(&["price", "qty", "status", "shipped", "kind", "note"]),
        )
    }

    #[test]
    fn defaults_become_literals_or_auto() {
        let d = |sql: &str, k: Kind| translate_default(sql, &k, false);
        assert_eq!(d("'none'::text", Kind::Text), Default::Value("none".into()));
        assert_eq!(d("''::text", Kind::Text), Default::Value("''".into()));
        assert_eq!(
            d("'it''s'::text", Kind::Text),
            Default::Value("'it''s'".into())
        );
        assert_eq!(d("0", Kind::Int), Default::Value("0".into()));
        assert_eq!(d("(-1)", Kind::Int), Default::Value("-1".into()));
        assert_eq!(d("'5'::integer", Kind::Int), Default::Value("5".into()));
        assert_eq!(d("1.50", Kind::Decimal), Default::Value("1.50".into()));
        assert_eq!(d("false", Kind::Bool), Default::Value("false".into()));
        let e = Kind::Enum(vec!["open".into(), "paid".into()]);
        assert_eq!(
            d("'open'::order_status", e.clone()),
            Default::Value("open".into())
        );
        assert_eq!(d("'gone'::order_status", e), Default::Untranslated);
        assert_eq!(d("now()", Kind::Timestamp), Default::Auto);
        assert_eq!(d("CURRENT_TIMESTAMP", Kind::Timestamp), Default::Auto);
        assert_eq!(d("CURRENT_DATE", Kind::Date), Default::Auto);
    }

    #[test]
    fn defaults_with_logic_stay_explicit() {
        let d = |sql: &str, k: Kind| translate_default(sql, &k, false);
        assert_eq!(
            d("nextval('t_id_seq'::regclass)", Kind::Int),
            Default::Untranslated
        );
        assert_eq!(
            d("timezone('utc'::text, now())", Kind::Timestamp),
            Default::Untranslated
        );
        assert_eq!(d("now()", Kind::Text), Default::Untranslated);
        assert_eq!(d("1.5", Kind::Int), Default::Untranslated);
        assert_eq!(d("gen_random_uuid()", Kind::Uuid), Default::Untranslated);
        assert_eq!(
            translate_default("gen_random_uuid()", &Kind::Uuid, true),
            Default::Implied
        );
        assert_eq!(
            translate_default("uuid_generate_v4()", &Kind::Uuid, true),
            Default::Implied
        );
    }

    #[test]
    fn simple_checks_translate() {
        assert_eq!(
            check("CHECK ((price >= (0)::numeric))").as_deref(),
            Some("price >= 0")
        );
        assert_eq!(
            check("CHECK (((qty > 0) AND (qty < 100)))").as_deref(),
            Some("(qty > 0) and (qty < 100)")
        );
        assert_eq!(
            check("CHECK (((status <> 'shipped'::text) OR shipped))").as_deref(),
            Some("(status != 'shipped') or shipped")
        );
        assert_eq!(
            check("CHECK ((status = ANY (ARRAY['a'::text, 'b'::text])))").as_deref(),
            Some("status in ['a', 'b']")
        );
        assert_eq!(
            check("CHECK ((note IS NOT NULL))").as_deref(),
            Some("note != null")
        );
        assert_eq!(
            check("CHECK ((note IS NULL))").as_deref(),
            Some("note == null")
        );
        assert_eq!(
            check("CHECK ((qty >= (- 1)))").as_deref(),
            Some("qty >= (- 1)")
        );
    }

    #[test]
    fn checks_with_no_exact_equivalent_are_refused() {
        for def in [
            "CHECK ((qty < least(10, 100)))",
            "CHECK ((note ~~ 'a%'::text))",
            "CHECK ((note ~ '^a'::text))",
            "CHECK (((note)::text <> ALL (ARRAY['a'::text])))",
            "CHECK ((qty BETWEEN 1 AND 5))",
            "CHECK ((length(note) > 3))",
            "CHECK ((other > 1))",
            "CHECK ((qty > (SELECT 1)))",
            "CHECK ((note || 'x' = 'y'))",
            "EXCLUDE USING gist (kind WITH =)",
            "CHECK ((qty > 0)",
        ] {
            assert_eq!(check(def), None, "{def}");
        }
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use serde_json::json;

    fn users() -> Vec<String> {
        vec!["owner".to_string(), "editor".to_string()]
    }

    fn refs() -> Vec<(String, String)> {
        vec![("created_by".to_string(), "Users".to_string())]
    }

    fn plain() -> Vec<String> {
        vec!["status".to_string(), "token".to_string()]
    }

    /// A condition with no `auth.role()`, as the Svetra expression, or why not.
    fn why(sql: &str) -> Result<String, String> {
        let (u, r, p) = (users(), refs(), plain());
        let fields = Fields {
            users: &u,
            refs: &r,
            plain: &p,
        };
        parse_condition(sql, fields).map(|c| c.render())
    }

    fn cond(sql: &str) -> Option<String> {
        why(sql).ok()
    }

    fn all(_: &str, _: &str) -> bool {
        true
    }

    fn policy(cmd: &str, roles: Value, using: Value, check: Value) -> Value {
        json!({ "cmd": cmd, "roles": roles, "using": using, "check": check, "permissive": "PERMISSIVE" })
    }

    fn tr(p: &Value) -> Result<Translated, String> {
        let u = ["owner".to_string()];
        translate_policy(
            p,
            Fields {
                users: &u,
                ..Fields::default()
            },
            &all,
        )
    }

    #[test]
    fn owner_in_every_shape_the_database_prints() {
        for sql in [
            "(owner = auth.uid())",
            "(auth.uid() = owner)",
            "owner = auth.uid()",
            "((owner = auth.uid()))",
            "(owner = ( SELECT auth.uid() AS uid))",
            "(( SELECT auth.uid() AS uid) = owner)",
            "(owner = (SELECT auth.uid()))",
            "(owner = (auth.uid())::uuid)",
            "((owner)::uuid = auth.uid())",
            "((owner)::text = (auth.uid())::text)",
            "(((auth.uid())::text) = (owner)::text)",
            "((owner)::text = ( SELECT (auth.uid())::text AS uid))",
        ] {
            assert_eq!(cond(sql).as_deref(), Some("owner == auth.user"), "{sql}");
        }
    }

    #[test]
    fn true_and_combinations() {
        assert_eq!(cond("true").as_deref(), Some("true"));
        assert_eq!(cond("(true)").as_deref(), Some("true"));
        assert_eq!(
            cond("((owner = auth.uid()) AND (editor = auth.uid()))").as_deref(),
            Some("owner == auth.user and editor == auth.user")
        );
        assert_eq!(
            cond("(((owner = auth.uid()) OR (editor = auth.uid())) AND true)").as_deref(),
            Some("(owner == auth.user or editor == auth.user) and true")
        );
    }

    #[test]
    fn everything_else_stays_untranslated() {
        for sql in [
            "(other = auth.uid())",
            "(owner = auth.uid()) AND (other = 1)",
            "((owner)::text = auth.uid())",
            "(owner = (auth.uid())::text)",
            "((owner)::text = (auth.uid())::varchar)",
            "((owner)::text::text = (auth.uid())::text::text)",
            "(owner = auth.uid()) AND",
            "(owner = auth.uid() OR)",
            "owner = auth.uid() owner",
            "(owner <> auth.uid())",
            "(NOT (owner = auth.uid()))",
            "(owner IS NOT NULL)",
            "false",
            "(status = 'published'::text)",
            "((auth.jwt() ->> 'role'::text) = 'admin'::text)",
            "(auth.role() = 'service_role'::text)",
            "(auth.role() <> 'anon'::text)",
            "(auth.role() = auth.role())",
            "is_admin()",
            "(owner = auth.uid() AND is_admin())",
            "(EXISTS ( SELECT 1 FROM membership m WHERE ((m.org = orders.org) AND (m.member = auth.uid()))))",
            "(owner IN ( SELECT user_id FROM team))",
            "(owner = ( SELECT id FROM profiles))",
            "(t.owner = auth.uid())",
            "(auth.uid() = auth.uid())",
            "",
        ] {
            assert_eq!(cond(sql), None, "{sql}");
        }
    }

    #[test]
    fn commands_become_allow_when_and_check() {
        let own = json!("(owner = auth.uid())");
        let auth = json!(["authenticated"]);
        let t = tr(&policy("SELECT", auth.clone(), own.clone(), Value::Null)).unwrap();
        assert_eq!(t.role, "authenticated");
        assert_eq!(t.actions, ["select"]);
        assert_eq!(t.when, "owner == auth.user");
        assert!(t.check.is_none() && !t.restrictive);
        let t = tr(&policy("INSERT", auth.clone(), Value::Null, own.clone())).unwrap();
        assert_eq!(
            (&t.actions[..], t.when.as_str()),
            (&["insert"][..], "owner == auth.user")
        );
        let t = tr(&policy("DELETE", auth.clone(), own.clone(), Value::Null)).unwrap();
        assert_eq!(t.actions, ["delete"]);
        let t = tr(&policy("UPDATE", auth.clone(), own.clone(), Value::Null)).unwrap();
        assert!(t.check.is_none());
        // The same check as `using` is left out; a different one is kept.
        let same = policy(
            "UPDATE",
            auth.clone(),
            own.clone(),
            json!("owner = auth.uid()"),
        );
        assert!(tr(&same).unwrap().check.is_none());
        let t = tr(&policy("UPDATE", auth.clone(), json!("true"), own.clone())).unwrap();
        assert_eq!(
            (t.when.as_str(), t.check.as_deref()),
            ("true", Some("owner == auth.user"))
        );
        let t = tr(&policy("ALL", auth.clone(), own.clone(), Value::Null)).unwrap();
        assert_eq!(t.actions, ["select", "insert", "update", "delete"]);
        let t = tr(&policy("ALL", auth.clone(), own.clone(), own.clone())).unwrap();
        assert!(t.check.is_none());
    }

    #[test]
    fn commands_it_cannot_say_exactly() {
        let own = json!("(owner = auth.uid())");
        let auth = json!(["authenticated"]);
        for p in [
            policy("ALL", auth.clone(), own.clone(), json!("true")),
            policy("UPDATE", auth.clone(), Value::Null, own.clone()),
            policy("SELECT", auth.clone(), own.clone(), own.clone()),
            policy("INSERT", auth.clone(), own.clone(), own.clone()),
            policy("SELECT", auth.clone(), json!("is_admin()"), Value::Null),
            policy("UPDATE", auth.clone(), own.clone(), json!("is_admin()")),
            policy("TRUNCATE", auth.clone(), own.clone(), Value::Null),
        ] {
            assert!(tr(&p).is_err(), "{p}");
        }
    }

    #[test]
    fn roles_become_as() {
        let role = |r: Value| tr(&policy("SELECT", r, json!("true"), Value::Null));
        assert_eq!(role(json!(["public"])).unwrap().role, "anyone");
        assert_eq!(
            role(json!(["anon", "authenticated"])).unwrap().role,
            "anyone"
        );
        assert_eq!(
            role(json!(["authenticated"])).unwrap().role,
            "authenticated"
        );
        assert_eq!(role(json!(["anon"])).unwrap().role, "anon");
        assert_eq!(role(json!("{anon,authenticated}")).unwrap().role, "anyone");
        for r in [
            json!(["service_role"]),
            json!(["authenticated", "service_role"]),
            json!([]),
        ] {
            assert!(role(r).is_err());
        }
    }

    #[test]
    fn restrictive_is_kept() {
        let mut p = policy("ALL", json!(["authenticated"]), json!("true"), Value::Null);
        p["permissive"] = json!("RESTRICTIVE");
        assert!(tr(&p).unwrap().restrictive);
    }

    #[test]
    fn privileges_are_respected() {
        let p = policy("ALL", json!(["public"]), json!("true"), Value::Null);
        let no_delete = |_: &str, a: &str| a != "delete";
        let t = translate_policy(&p, Fields::default(), &no_delete).unwrap();
        assert_eq!(t.actions, ["select", "insert", "update"]);
        // `anyone` needs both roles to hold it.
        let only_anon = |r: &str, _: &str| r == "anon";
        assert!(translate_policy(&p, Fields::default(), &only_anon).is_err());
        let q = policy("SELECT", json!(["anon"]), json!("true"), Value::Null);
        assert!(translate_policy(&q, Fields::default(), &only_anon).is_ok());
    }

    #[test]
    fn a_reason_names_what_stopped_the_condition() {
        for (sql, reason) in [
            (
                "(created_by = auth.uid())",
                "`created_by` is a reference to Users, not a user field",
            ),
            (
                "(auth.uid() = created_by)",
                "`created_by` is a reference to Users, not a user field",
            ),
            (
                "(token = auth.uid())",
                "`token` is a plain uuid column, not a user field (it has no foreign key to auth.users)",
            ),
            (
                "(ghost = auth.uid())",
                "`ghost` isn't a field of this entity (its column was left out, or it isn't a column)",
            ),
            (
                "authorize('channels.delete'::app_permission)",
                "calls the function authorize(); infer doesn't read function bodies",
            ),
            (
                "(owner = auth.uid() AND is_admin())",
                "calls the function is_admin(); infer doesn't read function bodies",
            ),
            (
                "((auth.jwt() ->> 'role'::text) = 'admin'::text)",
                "reads a JWT claim (auth.jwt())",
            ),
            (
                "(EXISTS ( SELECT 1 FROM membership m WHERE (m.member = auth.uid())))",
                "a subquery",
            ),
            ("(owner IN ( SELECT user_id FROM team))", "a subquery"),
            (
                "(auth.role() = 'service_role'::text)",
                "auth.role() is compared with 'service_role', not 'authenticated'",
            ),
            ("(current_user = 'admin')", "reads the database user"),
            (
                "(NOT (owner = auth.uid()))",
                "uses NOT, which infer doesn't translate",
            ),
            (
                "(owner IS NOT NULL)",
                "uses IS, which infer doesn't translate",
            ),
            (
                "(owner <> auth.uid())",
                "uses <>, which infer doesn't translate",
            ),
            ("false", "uses FALSE, which infer doesn't translate"),
            (
                "(status = 'published'::text)",
                "compares with a literal, which infer doesn't translate",
            ),
            (
                "(owner = auth.uid()) AND",
                "a condition shape infer doesn't translate",
            ),
        ] {
            assert_eq!(why(sql).unwrap_err(), reason, "{sql}");
        }
    }

    #[test]
    fn a_policy_carries_the_reason() {
        let p = policy(
            "SELECT",
            json!(["authenticated"]),
            json!("(created_by = auth.uid())"),
            Value::Null,
        );
        let (u, r) = (users(), refs());
        let f = Fields {
            users: &u,
            refs: &r,
            plain: &[],
        };
        let err = translate_policy(&p, f, &all).unwrap_err();
        assert!(
            err.contains("`created_by` is a reference to Users"),
            "{err}"
        );
    }

    fn role_policy(
        cmd: &str,
        roles: Value,
        using: &str,
        check: Value,
    ) -> Result<Translated, String> {
        tr(&policy(cmd, roles, json!(using), check))
    }

    #[test]
    fn auth_role_becomes_as() {
        let read = |roles: Value, sql: &str| role_policy("SELECT", roles, sql, Value::Null);
        for sql in [
            "(auth.role() = 'authenticated'::text)",
            "('authenticated'::text = auth.role())",
            "(( SELECT auth.role() AS role) = 'authenticated'::text)",
            "((SELECT auth.role()) = 'authenticated')",
            "(auth.role() = 'authenticated')",
        ] {
            let t = read(json!(["authenticated"]), sql).unwrap();
            assert_eq!(
                (t.role, t.when.as_str()),
                ("authenticated", "true"),
                "{sql}"
            );
        }
        // `to public` with a role check says who it was for: the other role was never let in.
        let t = read(json!(["public"]), "(auth.role() = 'authenticated'::text)").unwrap();
        assert_eq!((t.role, t.when.as_str()), ("authenticated", "true"));
        // `'anon'` is never translated: a request with no JWT runs as anon with a null
        // `auth.role()`, so `as: anon` would let in a request the database refuses.
        for (roles, sql) in [
            (json!(["anon"]), "(auth.role() = 'anon'::text)"),
            (json!(["public"]), "(auth.role() = 'anon'::text)"),
            (
                json!(["anon", "authenticated"]),
                "((auth.role() = 'anon'::text) OR (auth.role() = 'authenticated'::text))",
            ),
        ] {
            assert!(read(roles, sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn auth_role_combines_with_the_rest_of_the_condition() {
        let read = |roles: Value, sql: &str| role_policy("SELECT", roles, sql, Value::Null);
        let t = read(
            json!(["authenticated"]),
            "((auth.role() = 'authenticated'::text) AND (owner = auth.uid()))",
        )
        .unwrap();
        assert_eq!(
            (t.role, t.when.as_str()),
            ("authenticated", "owner == auth.user")
        );
        // Which half of an `or` holds depends on the role.
        let t = read(
            json!(["public"]),
            "((auth.role() = 'authenticated'::text) AND (owner = auth.uid()))",
        )
        .unwrap();
        assert_eq!(
            (t.role, t.when.as_str()),
            ("authenticated", "owner == auth.user")
        );
        // Any mention of `auth.role() = 'anon'` is left untranslated, even where it is false.
        assert!(
            read(
                json!(["authenticated"]),
                "((auth.role() = 'anon'::text) OR (owner = auth.uid()))",
            )
            .is_err()
        );
        let t = read(
            json!(["authenticated"]),
            "((auth.role() = 'authenticated'::text) OR (owner = auth.uid()))",
        )
        .unwrap();
        assert_eq!(t.when, "true");
        // An owner check that holds for both roles stays one policy.
        let t = read(
            json!(["public"]),
            "((owner = auth.uid()) OR (auth.role() = 'anon'::text))",
        );
        assert!(
            t.is_err(),
            "anon gets everything, authenticated only their own"
        );
    }

    #[test]
    fn auth_role_it_cannot_say_stays_untranslated_with_a_reason() {
        let read = |roles: Value, sql: &str| role_policy("SELECT", roles, sql, Value::Null);
        // Never true for the role the policy is for: it lets nobody in.
        let e = read(json!(["anon"]), "(auth.role() = 'authenticated'::text)").unwrap_err();
        assert!(e.contains("never lets anyone in"), "{e}");
        let e = read(
            json!(["authenticated"]),
            "(auth.role() = 'service_role'::text)",
        )
        .unwrap_err();
        assert!(e.contains("'service_role'"), "{e}");
    }

    #[test]
    fn auth_role_on_an_update_applies_to_using_and_check() {
        let t = tr(&policy(
            "UPDATE",
            json!(["public"]),
            json!("(auth.role() = 'authenticated'::text)"),
            json!("((auth.role() = 'authenticated'::text) AND (owner = auth.uid()))"),
        ))
        .unwrap();
        assert_eq!(
            (t.role, t.when.as_str(), t.check.as_deref()),
            ("authenticated", "true", Some("owner == auth.user"))
        );
    }

    #[test]
    fn a_restrictive_role_policy_never_drops_a_role() {
        let mut p = policy(
            "SELECT",
            json!(["public"]),
            json!("(auth.role() = 'authenticated'::text)"),
            Value::Null,
        );
        p["permissive"] = json!("RESTRICTIVE");
        // Dropping `anon` from a restrictive policy that is false for it would let `anon` past it.
        let e = tr(&p).unwrap_err();
        assert!(e.contains("widen"), "{e}");
        p["roles"] = json!(["authenticated"]);
        assert!(tr(&p).unwrap().restrictive);
    }

    fn fk(text: &str) -> String {
        format!("FOREIGN KEY (c) REFERENCES t(id){text}")
    }

    #[test]
    fn on_delete_becomes_the_ref_option() {
        let note = |text: &str, is_ref, required| foreign_key_note(&fk(text), is_ref, required);
        assert_eq!(
            note(" ON DELETE CASCADE", true, true),
            (Some("on_delete: cascade".into()), vec![])
        );
        assert_eq!(
            note(" ON DELETE SET NULL", true, false),
            (Some("on_delete: set_null".into()), vec![])
        );
        assert_eq!(
            note(
                " ON UPDATE NO ACTION ON DELETE CASCADE DEFERRABLE",
                true,
                false
            )
            .0,
            Some("on_delete: cascade".into())
        );
        for text in [
            "",
            " ON DELETE NO ACTION",
            " ON DELETE RESTRICT",
            " NOT VALID",
        ] {
            assert_eq!(note(text, true, true), (None, vec![]), "{text}");
        }
    }

    #[test]
    fn an_action_svetra_cannot_write_is_a_lossy_note() {
        let note = |text: &str, is_ref, required| foreign_key_note(&fk(text), is_ref, required);
        let (opt, notes) = note(" ON DELETE SET DEFAULT", true, false);
        assert_eq!(opt, None);
        assert_eq!(
            notes,
            ["lossy: the foreign key had ON DELETE SET DEFAULT, which Svetra can't write"]
        );
        // `set null` on a required field would be `SVT023`.
        let (opt, notes) = note(" ON DELETE SET NULL", true, true);
        assert_eq!(opt, None);
        assert!(
            notes[0].contains("isn't allowed on a required field"),
            "{notes:?}"
        );
        // A user field has no action at all, and no update action either.
        let (opt, notes) = note(" ON UPDATE CASCADE ON DELETE CASCADE", false, true);
        assert_eq!(opt, None);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("a user field has none"), "{notes:?}");
        assert!(notes[1].contains("ON UPDATE CASCADE"), "{notes:?}");
    }
}
