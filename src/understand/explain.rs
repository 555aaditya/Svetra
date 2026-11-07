//! `svetra explain`: the graph in plain English (docs/understand.md). Describes only what the
//! graph declares, so it can't disagree with the compiled app.

use super::words::{article, label, list, plural, sentence, singular};
use crate::diag::{Code, Diagnostic, Severity};
use crate::expr::{self, Expr, Op};
use crate::model::*;
use std::collections::HashMap;

pub fn explain(doc: &Document, diags: &[Diagnostic]) -> String {
    let entities: HashMap<&str, &Entity> =
        doc.entities.iter().map(|e| (e.name.as_str(), e)).collect();
    let mut out = format!("{}\n\nWhat your app stores\n", doc.system.name);
    for e in &doc.entities {
        out += &format!("  {}\n", sentence(&plural(&e.name)));
        for f in e
            .fields
            .iter()
            .filter(|f| !(f.primary && f.ty == FieldType::Uuid))
        {
            out += &format!("    - {}\n", field_line(f));
        }
    }
    if !doc.roles.is_empty() {
        out += "\nRoles\n";
        for r in &doc.roles {
            let mut line = r.name.clone();
            if let Some(d) = &r.description {
                let mut c = d.chars();
                let d = c.next().map_or(String::new(), |f| {
                    f.to_lowercase().collect::<String>() + c.as_str()
                });
                line += &format!(": {d}");
            }
            if let Some(s) = &r.scope {
                let one = singular(&s.name);
                line += &format!(" (held for one {one} at a time)");
            }
            if r.mfa {
                line += " (needs a second factor)";
            }
            out += &format!("  - {line}\n");
        }
    }
    if let Some(a) = &doc.auth {
        out += "\nSigning in\n";
        for line in signing_in(a, &doc.roles) {
            out += &format!("  - {line}\n");
        }
    }
    let rules: Vec<String> = doc
        .entities
        .iter()
        .flat_map(|e| rules(e, &entities))
        .collect();
    if !rules.is_empty() {
        out += "\nRules that always hold\n";
        for r in rules {
            out += &format!("  - {r}\n");
        }
    }
    out += "\nWho can see and change what\n";
    for e in &doc.entities {
        out += &format!("  {}\n", sentence(&plural(&e.name)));
        for line in access(e, doc, &entities) {
            out += &format!("    - {line}\n");
        }
    }
    if !doc.holes.is_empty() {
        out += "\nCustom code\n";
        for h in &doc.holes {
            out += &format!("  {}\n", h.name);
            for line in hole(h, &entities) {
                out += &format!("    - {line}\n");
            }
        }
    }
    let notes: Vec<String> = diags
        .iter()
        .filter(|d| d.severity() == Severity::Warning)
        .map(|d| note(d, doc))
        .collect();
    if !notes.is_empty() {
        out += "\nWorth knowing\n";
        for n in notes {
            out += &format!("  - {n}\n");
        }
    }
    if let Some(a) = &doc.auth {
        out += "\nBefore going live\n";
        for line in before_live(a) {
            out += &format!("  - {line}\n");
        }
    }
    out
}

/// A sign-in method's usual name.
fn method(m: &str) -> String {
    match m {
        "email" => "email (a password or an emailed link)",
        "github" => "GitHub",
        "gitlab" => "GitLab",
        "linkedin_oidc" => "LinkedIn",
        "slack_oidc" => "Slack",
        "x" => "X",
        other => return sentence(other),
    }
    .into()
}

fn signing_in(a: &Auth, roles: &[RoleDef]) -> Vec<String> {
    let methods: Vec<String> = a.sign_in.iter().map(|m| method(&m.name)).collect();
    let mut out = vec![format!("People sign in with {}.", list(&methods, "or"))];
    out.push(if a.invite_only() {
        "Only people you invite can create an account.".into()
    } else {
        "Anyone can create an account.".into()
    });
    if a.email() {
        out.push(if a.confirms_email() {
            "People confirm their email address before they can sign in.".into()
        } else {
            "People can sign in before confirming their email address.".into()
        });
        let with = match a.requirements() {
            "letters_digits" => ", with letters and digits",
            "lower_upper_letters_digits" => ", with lowercase and uppercase letters and digits",
            "lower_upper_letters_digits_symbols" => {
                ", with lowercase and uppercase letters, digits and symbols"
            }
            _ => "",
        };
        out.push(format!(
            "Passwords need at least {} characters{with}.",
            a.min_length()
        ));
    }
    for r in roles.iter().filter(|r| r.mfa) {
        out.push(format!(
            "The {} role only counts after a second factor, an authenticator app.",
            r.name
        ));
    }
    out
}

fn before_live(a: &Auth) -> Vec<String> {
    let mut out: Vec<String> = a
        .sign_in
        .iter()
        .filter(|m| m.name != "email")
        .map(|m| {
            let env = format!("SUPABASE_AUTH_EXTERNAL_{}", m.name.to_uppercase());
            format!(
                "Create a {} sign-in app, then set {env}_CLIENT_ID and {env}_SECRET.",
                method(&m.name)
            )
        })
        .collect();
    if a.email() {
        out.push("Set up your own email sender (SMTP). Supabase's built-in sender is for testing and only sends a few emails an hour.".into());
    }
    out.push("Set your app's real web address in Supabase, for sign-in links and the page people return to after signing in.".into());
    out
}

/// How a field's type reads in English.
fn type_words(t: &FieldType) -> String {
    let digits = |p: u8, s: u8| format!("up to {} digits before the point and {s} after", p - s);
    match t {
        FieldType::Text => "text".into(),
        FieldType::Int => "a whole number".into(),
        FieldType::Float => "a number".into(),
        FieldType::Bool => "yes or no".into(),
        FieldType::Uuid => "an id".into(),
        FieldType::Timestamp => "a date and time".into(),
        FieldType::Date => "a date".into(),
        FieldType::Money { precision, scale } => {
            format!("an amount of money, {}", digits(*precision, *scale))
        }
        FieldType::Decimal { precision, scale } => {
            format!("an exact number, {}", digits(*precision, *scale))
        }
        FieldType::Enum(values) => format!("one of {}", list(values, "or")),
        FieldType::Json => "free-form data".into(),
        FieldType::Ref(r) => {
            let e = singular(&r.name);
            format!("{} {e}", article(&e))
        }
        FieldType::User => "a signed-in person".into(),
    }
}

fn field_line(f: &Field) -> String {
    let mut line = match &f.ty {
        FieldType::User if ["owner", "author", "user", "created_by"].contains(&f.name.as_str()) => {
            "the signed-in person who owns it".to_string()
        }
        FieldType::User => format!("{}: a signed-in person", label(&f.name)),
        FieldType::Ref(r) if f.name == crate::compile::names::snake(&r.name) => {
            format!("the {} it belongs to", singular(&r.name))
        }
        FieldType::Ref(r) => format!("{}: a linked {}", label(&f.name), singular(&r.name)),
        t => format!("{}: {}", label(&f.name), type_words(t)),
    };
    if f.auto_now {
        line += ", set automatically when created";
    }
    if f.primary {
        line += " (its unique id)";
    } else if f.required {
        line += " (required)";
    }
    if let Some(d) = &f.default {
        let v = match d {
            Literal::Bool(b) => {
                if *b {
                    "yes".into()
                } else {
                    "no".into()
                }
            }
            Literal::Int(i) => i.to_string(),
            Literal::Float(x) => x.to_string(),
            Literal::Str(s) => s.clone(),
        };
        line += &format!(" (starts as {v})");
    }
    line
}

/// Field names render bare in constraints ("balance"), and as "its balance" in access rules.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Constraint,
    Policy,
    /// A hole's `when`: names are its inputs ("the source"), not a row's fields.
    Hole,
}

fn english(e: &Expr, st: Style) -> String {
    let field = |f: &str| match st {
        Style::Policy => format!("its {}", label(f)),
        Style::Hole => format!("the {}", label(f)),
        Style::Constraint => label(f),
    };
    let its = if st == Style::Hole { "the" } else { "its" };
    match e {
        Expr::Num(n) => format!("{n}"),
        Expr::Str(s) => s.clone(),
        Expr::Bool(b) => {
            if *b {
                "yes".into()
            } else {
                "no".into()
            }
        }
        Expr::Null => "empty".into(),
        Expr::Path(p) => match p.as_slice() {
            [f] => field(f),
            [a, b] if a == "auth" && b == "user" => "them".into(),
            [r, f] => format!("the {} of {its} {}", label(f), label(r)),
            _ => p.join("."),
        },
        Expr::Call(_, args) => match args.as_slice() {
            [Expr::Str(role)] => format!("they have the {role} role"),
            [Expr::Str(role), Expr::Path(p)] if p.len() == 1 && p[0] == "id" => {
                format!("they have the {role} role for it")
            }
            [Expr::Str(role), Expr::Path(p)] if p.len() == 1 => {
                format!("they have the {role} role for {its} {}", label(&p[0]))
            }
            _ => "they have the required role".into(),
        },
        Expr::Neg(x) => format!("-{}", english(x, st)),
        Expr::Not(x) => format!("not ({})", english(x, st)),
        Expr::In(x, items) => {
            let words: Vec<String> = items.iter().map(|i| english(i, st)).collect();
            format!("{} is {}", english(x, st), list(&words, "or"))
        }
        Expr::Bin(op @ (Op::Eq | Op::Ne), l, r)
            if matches!(**l, Expr::Null) || matches!(**r, Expr::Null) =>
        {
            let x = if matches!(**l, Expr::Null) { r } else { l };
            format!(
                "{} is {}",
                english(x, st),
                if *op == Op::Eq { "empty" } else { "filled in" }
            )
        }
        Expr::Bin(Op::Ne, l, r) if matches!((&**l, &**r), (Expr::Path(a), Expr::Path(b)) if a.len() == 1 && b.len() == 1) =>
        {
            format!("{} and {} are different", english(l, st), english(r, st))
        }
        Expr::Bin(op, l, r) => {
            let (a, b) = (group(l, *op, st), group(r, *op, st));
            let word = match op {
                Op::Or => ", or",
                Op::And => " and",
                Op::Eq => " is",
                Op::Ne => " is not",
                Op::Lt => " is less than",
                Op::Le => " is at most",
                Op::Gt => " is more than",
                Op::Ge => " is at least",
                Op::Add => " +",
                Op::Sub => " -",
                Op::Mul => " ×",
                Op::Div => " ÷",
            };
            format!("{a}{word} {b}")
        }
    }
}

/// Mixed `and`/`or` keeps its grouping in brackets, so the English means exactly the same.
fn group(e: &Expr, parent: Op, st: Style) -> String {
    let mixed = matches!(
        (parent, e),
        (Op::And, Expr::Bin(Op::Or, ..)) | (Op::Or, Expr::Bin(Op::And, ..))
    );
    if mixed {
        format!("({})", english(e, st))
    } else {
        english(e, st)
    }
}

fn rules(e: &Entity, _entities: &HashMap<&str, &Entity>) -> Vec<String> {
    let one = singular(&e.name);
    let possessive = format!("{} {one}'s", sentence(article(&one)));
    let mut out: Vec<String> = e
        .constraints
        .iter()
        .map(|c| {
            let parsed = expr::parse(&c.expr).expect("validated expression");
            let text = english(&parsed, Style::Constraint);
            if matches!(parsed, Expr::Bin(_, ref l, _) if matches!(**l, Expr::Path(_))) {
                format!("{possessive} {text}.")
            } else {
                format!("For {} {one}: {text}.", article(&one))
            }
        })
        .collect();
    let single = e
        .fields
        .iter()
        .filter(|f| f.unique && !f.primary)
        .map(|f| vec![label(&f.name)]);
    let lists = e
        .unique
        .iter()
        .map(|l| l.fields.iter().map(|x| label(&x.name)).collect::<Vec<_>>());
    for cols in single.chain(lists) {
        out.push(format!(
            "No two {} have the same {}.",
            plural(&e.name),
            list(&cols, "and")
        ));
    }
    out
}

fn access(e: &Entity, doc: &Document, _entities: &HashMap<&str, &Entity>) -> Vec<String> {
    let many = plural(&e.name);
    let one = singular(&e.name);
    let writers: Vec<String> = doc
        .holes
        .iter()
        .filter(|h| h.writes.iter().any(|w| w.name == e.name))
        .map(|h| h.name.clone())
        .collect();
    let by_code = if writers.is_empty() {
        String::new()
    } else {
        format!(" Only custom code can change them: {}.", writers.join(", "))
    };
    if e.private {
        let tail = if writers.is_empty() {
            "; only custom code can.".to_string()
        } else {
            format!(".{by_code}")
        };
        return vec![format!(
            "{} are private: the app can't reach them at all{tail}",
            sentence(&many)
        )];
    }
    let policies: Vec<&Policy> = doc
        .policies
        .iter()
        .filter(|p| p.on.name == e.name)
        .collect();
    let mut out = Vec::new();
    for p in &policies {
        let verbs: Vec<String> = p
            .allow
            .iter()
            .map(|a| match a {
                Action::Select => "see",
                Action::Insert => "create",
                Action::Update => "change",
                Action::Delete => "delete",
            })
            .map(String::from)
            .collect();
        let parsed = expr::parse(&p.when).expect("validated expression");
        // A condition that is only a role check reads best with the role as the subject.
        if let Expr::Call(_, args) = &parsed {
            let line = match args.as_slice() {
                [Expr::Str(r)] => format!(
                    "People with the {r} role can {} {many}.",
                    list(&verbs, "and")
                ),
                [Expr::Str(r), Expr::Path(f)]
                    if f.len() == 1 && e.fields.iter().any(|x| x.primary && x.name == f[0]) =>
                {
                    format!(
                        "People with the {r} role for {} {one} can {} that {one}.",
                        article(&one),
                        list(&verbs, "and")
                    )
                }
                [Expr::Str(r), Expr::Path(f)] if f.len() == 1 => {
                    format!(
                        "People with the {r} role for {} {one}'s {} can {} that {one}.",
                        article(&one),
                        label(&f[0]),
                        list(&verbs, "and")
                    )
                }
                _ => format!("Some people can {} {many}.", list(&verbs, "and")),
            };
            out.push(line);
            continue;
        }
        let who = match p.role {
            Role::Authenticated => "Signed-in people",
            Role::Anon => "People who aren't signed in",
            Role::Anyone => "Anyone, signed in or not,",
        };
        let what = if matches!(parsed, Expr::Bool(true)) {
            many.clone()
        } else {
            format!(
                "{} {one} when {}",
                article(&one),
                english(&parsed, Style::Policy)
            )
        };
        out.push(format!("{who} can {} {what}.", list(&verbs, "and")));
    }
    let sees = policies.iter().any(|p| p.allow.contains(&Action::Select));
    out.push(if sees {
        format!("Nobody can see {many} any other way.")
    } else {
        format!("Nobody can see {many}.")
    });
    let missing: Vec<String> = [
        (Action::Insert, "create"),
        (Action::Update, "change"),
        (Action::Delete, "delete"),
    ]
    .iter()
    .filter(|(a, _)| !policies.iter().any(|p| p.allow.contains(a)))
    .map(|(_, w)| w.to_string())
    .collect();
    if !missing.is_empty() {
        out.push(format!(
            "Nobody can {} {many} directly.{by_code}",
            list(&missing, "or")
        ));
    }
    out
}

/// A hole parameter or output in English.
fn param_words(t: &ParamType, entities: &HashMap<&str, &Entity>) -> String {
    match t {
        ParamType::Entity(r) => {
            let e = singular(&r.name);
            format!("{} {e}", article(&e))
        }
        ParamType::FieldOf { entity, field } => {
            let target = entities
                .get(entity.name.as_str())
                .expect("validated hole reference");
            let f = target
                .fields
                .iter()
                .find(|x| &x.name == field)
                .expect("validated field");
            if f.primary {
                let e = singular(&entity.name);
                format!("{} {e}", article(&e))
            } else {
                type_words(&f.ty)
            }
        }
        ParamType::Type(t) => type_words(t),
    }
}

fn hole(h: &Hole, entities: &HashMap<&str, &Entity>) -> Vec<String> {
    let mut out = Vec::new();
    if !h.input.is_empty() {
        let params: Vec<String> = h
            .input
            .iter()
            .map(|p| format!("{} ({})", label(&p.name), param_words(&p.ty, entities)))
            .collect();
        out.push(format!("takes: {}", params.join(", ")));
    }
    out.push(format!(
        "returns: {}",
        h.output
            .as_ref()
            .map_or("nothing".into(), |t| param_words(t, entities))
    ));
    let names = |rs: &[NameRef]| {
        rs.iter()
            .map(|r| plural(&r.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let who = match h.role {
        Role::Authenticated => "signed-in people",
        Role::Anon => "people who aren't signed in",
        Role::Anyone => "anyone, signed in or not",
    };
    out.push(match h.when.as_deref().and_then(|w| expr::parse(w).ok()) {
        Some(e) => format!(
            "who can call it: {who}, only when {}",
            english(&e, Style::Hole)
        ),
        None => format!("who can call it: {who}"),
    });
    out.push(match (h.reads.is_empty(), h.writes.is_empty()) {
        (true, true) => "can't read or change any data".into(),
        (false, true) => format!("can read: {}. Can't change any data.", names(&h.reads)),
        (true, false) => format!("can change: {}.", names(&h.writes)),
        (false, false) => format!(
            "can read: {}. Can change: {}.",
            names(&h.reads),
            names(&h.writes)
        ),
    });
    out
}

/// Warnings from `validate`, reworded for people.
fn note(d: &Diagnostic, doc: &Document) -> String {
    let entity = d
        .path
        .strip_prefix("entities.")
        .map(|p| p.split('.').next().unwrap_or(p));
    let many = entity.map(plural).unwrap_or_default();
    match d.code {
        Code::Svt004 => format!(
            "Nothing can ever add or change {many}: nobody is allowed to, and no custom code does."
        ),
        Code::Svt027 => {
            let target = d
                .path
                .strip_prefix("policies.")
                .and_then(|p| p.split('.').next())
                .and_then(|name| doc.policies.iter().find(|p| p.name == name));
            let what = target.map_or_else(String::new, |p| plural(&p.on.name));
            format!("A rule about who can see {what} does nothing, because {what} are private.")
        }
        Code::Svt007 => {
            let target = d
                .path
                .strip_prefix("policies.")
                .and_then(|p| p.split('.').next())
                .and_then(|name| doc.policies.iter().find(|p| p.name == name));
            let what = target.map_or_else(String::new, |p| plural(&p.on.name));
            format!(
                "People who aren't signed in can reach some {what} that signed-in people may not be able to. Signing in could take access away; if {what} should be public, make them public for everyone."
            )
        }
        _ => d.message.clone(),
    }
}
