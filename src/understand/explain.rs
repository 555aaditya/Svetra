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
        if e.audit {
            out += &format!(
                "    - Every change to {} is recorded in the audit log: who made it, when, and the row before and after.\n",
                plural(&e.name)
            );
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
    let live = before_live(doc);
    if !live.is_empty() {
        out += "\nBefore going live\n";
        for line in live {
            out += &format!("  - {line}\n");
        }
    }
    out
}

/// "its status" or "its title and status": the fields of a policy's `only`, in words.
fn fields_of(only: &[NameRef]) -> String {
    let names: Vec<String> = only.iter().map(|f| f.name.clone()).collect();
    fields_named(&names)
}

fn fields_named(names: &[String]) -> String {
    let labels: Vec<String> = names.iter().map(|f| label(f)).collect();
    format!("its {}", list(&labels, "and"))
}

/// The pieces of `explain`, for `svetra view`, which shows the same sentences.
pub mod parts {
    use super::*;

    /// "They can change only its status.": the sentence `explain` ends an update rule with.
    pub fn only_sentence(fields: &[String]) -> String {
        format!("They can change only {}.", fields_named(fields))
    }

    fn entities(doc: &Document) -> HashMap<&str, &Entity> {
        doc.entities.iter().map(|e| (e.name.as_str(), e)).collect()
    }

    /// "What your app stores" for one entity.
    pub fn fields(e: &Entity) -> Vec<String> {
        e.fields
            .iter()
            .filter(|f| !(f.primary && f.ty == FieldType::Uuid))
            .map(field_line)
            .collect()
    }

    /// "Who can see and change what" for one entity. For an entity the app can reach, the first
    /// lines are one per policy on it, in file order; then the "Nobody can" lines.
    pub fn access(doc: &Document, e: &Entity) -> Vec<String> {
        super::access(e, doc, &entities(doc))
    }

    /// What starts a scheduled hole, in words.
    pub fn starts(h: &Hole) -> Option<String> {
        super::starts(h)
    }

    pub fn hole(doc: &Document, h: &Hole) -> Vec<String> {
        super::hole(h, &entities(doc))
    }

    pub fn signing_in(doc: &Document) -> Vec<String> {
        doc.auth
            .as_ref()
            .map_or(vec![], |a| super::signing_in(a, &doc.roles))
    }

    pub fn before_going_live(doc: &Document) -> Vec<String> {
        super::before_live(doc)
    }
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

fn before_live(doc: &Document) -> Vec<String> {
    let Some(a) = &doc.auth else {
        return schedule_live(doc);
    };
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
    out.extend(schedule_live(doc));
    out
}

/// Once, when any code runs by itself on a schedule.
fn schedule_live(doc: &Document) -> Vec<String> {
    if doc
        .holes
        .iter()
        .any(|h| matches!(h.kind, HoleKind::Schedule(_)))
    {
        vec!["Code that runs by itself on a schedule needs Supabase's pg_cron extension. The update turns it on; if your project refuses, switch it on under Database, Extensions in the Supabase dashboard.".into()]
    } else {
        vec![]
    }
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
    /// A triggered hole's `when`: the row after the change, and `old.` for the row before.
    Row,
}

fn english(e: &Expr, st: Style) -> String {
    let field = |f: &str| match st {
        Style::Policy | Style::Row => format!("its {}", label(f)),
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
            [a, f] if a == "old" && st == Style::Row => format!("its previous {}", label(f)),
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
        // A field named for an action, such as `paid_by` or `assigned_to`, reads as that action:
        // "it was paid by them" instead of "its paid by is them".
        Expr::Bin(op @ (Op::Eq | Op::Ne), l, r) if st == Style::Policy && acted(l, r).is_some() => {
            let f = label(acted(l, r).unwrap_or_default());
            let not = if *op == Op::Ne { " not" } else { "" };
            let verb = if f.ends_with(" by") { "was" } else { "is" };
            format!("it {verb}{not} {f} them")
        }
        Expr::Bin(Op::Ne, l, r) if matches!((&**l, &**r), (Expr::Path(a), Expr::Path(b)) if a.len() == 1 && b.len() == 1) =>
        {
            format!("{} and {} are different", english(l, st), english(r, st))
        }
        Expr::Bin(op, l, r) => {
            // `status == 'paid' and old.status != 'paid'` is a change to paid.
            if *op == Op::And
                && st == Style::Row
                && let Some((f, to)) = becomes(l, r)
            {
                return format!("its {} becomes {}", label(f), english(to, st));
            }
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

/// `f == x and old.f != x` (either order): the field and the value it becomes.
fn becomes<'a>(l: &'a Expr, r: &'a Expr) -> Option<(&'a str, &'a Expr)> {
    let parts = |e: &'a Expr, op: Op| -> Option<(&'a Expr, &'a Expr)> {
        match e {
            Expr::Bin(o, a, b) if *o == op => Some((a, b)),
            _ => None,
        }
    };
    let lit = |e: &Expr| matches!(e, Expr::Str(_) | Expr::Num(_) | Expr::Bool(_));
    let (now, was) = if parts(l, Op::Eq).is_some() {
        (parts(l, Op::Eq)?, parts(r, Op::Ne)?)
    } else {
        (parts(r, Op::Eq)?, parts(l, Op::Ne)?)
    };
    match (now, was) {
        ((Expr::Path(a), x), (Expr::Path(b), y))
            if a.len() == 1
                && b.len() == 2
                && b[0] == "old"
                && a[0] == b[1]
                && x == y
                && lit(x) =>
        {
            Some((a[0].as_str(), x))
        }
        _ => None,
    }
}

/// The field in `field == auth.user` (either way round) when its name ends in `_by` or `_to`.
fn acted<'a>(l: &'a Expr, r: &'a Expr) -> Option<&'a str> {
    let me =
        |e: &Expr| matches!(e, Expr::Path(p) if p.len() == 2 && p[0] == "auth" && p[1] == "user");
    let field = |e: &'a Expr| match e {
        Expr::Path(p) => match p.as_slice() {
            [f] if f.ends_with("_by") || f.ends_with("_to") => Some(f.as_str()),
            _ => None,
        },
        _ => None,
    };
    if me(r) {
        field(l)
    } else if me(l) {
        field(r)
    } else {
        None
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
        let mut out = vec![format!(
            "{} are private: the app can't reach them at all{tail}",
            sentence(&many)
        )];
        out.extend(fires(e, doc));
        return out;
    }
    // Restrictive policies only narrow, so they don't get a sentence of their own: each sentence
    // they narrow ends with the limit instead (docs/update-check.md).
    let policies: Vec<&Policy> = doc
        .policies
        .iter()
        .filter(|p| p.on.name == e.name && !p.restrictive)
        .collect();
    let limits: Vec<&Policy> = doc
        .policies
        .iter()
        .filter(|p| p.on.name == e.name && p.restrictive)
        .collect();
    // The end of a sentence: what a restrictive policy adds, and, for an update, what must hold
    // after the change.
    let ending = |p: &Policy| -> String {
        let mut text = String::new();
        for r in limits
            .iter()
            .filter(|r| r.allow.iter().any(|a| p.allow.contains(a)))
        {
            let c = expr::parse(&r.when).expect("validated expression");
            text += &format!(", and only when {}", english(&c, Style::Policy));
            if let Some(only) = &r.only {
                text += &format!(" (and then only {} can change)", fields_of(only));
            }
        }
        text += ".";
        if let Some(check) = &p.check {
            let c = expr::parse(check).expect("validated expression");
            text += &format!(" After the change, {}.", english(&c, Style::Policy));
        }
        if let Some(only) = &p.only {
            text += &format!(" They can change only {}.", fields_of(only));
        }
        text
    };
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
            out.push(format!("{}{}", line.trim_end_matches('.'), ending(p)));
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
        out.push(format!(
            "{who} can {} {what}{}",
            list(&verbs, "and"),
            ending(p)
        ));
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
    out.extend(fires(e, doc));
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

/// What starts a scheduled or triggered hole, in words: `None` for a hole a person calls.
fn starts(h: &Hole) -> Option<String> {
    match &h.kind {
        HoleKind::Schedule(s) => {
            let when = match crate::cron::words(&s.cron) {
                Some(w) => format!("{w} (UTC)"),
                None => format!("on the schedule \"{}\" (UTC)", s.cron),
            };
            Some(format!(
                "runs by itself {when}. Nobody can run it from the app."
            ))
        }
        HoleKind::Trigger(t) => {
            let one = singular(&t.on.name);
            let events: Vec<String> = t
                .events
                .iter()
                .map(|a| event_past(*a).to_string())
                .collect();
            let only = h
                .when
                .as_deref()
                .and_then(|w| expr::parse(w).ok())
                .map_or(String::new(), |e| {
                    format!(", only when {}", english(&e, Style::Row))
                });
            Some(format!(
                "runs by itself after {} {one} is {}{only}. Nobody can run it directly.",
                article(&one),
                list(&events, "or")
            ))
        }
        HoleKind::Function => None,
    }
}

/// An event as a past participle: "created", "changed" or "deleted".
fn event_past(a: Action) -> &'static str {
    match a {
        Action::Insert => "created",
        Action::Update => "changed",
        _ => "deleted",
    }
}

/// The lines under an entity for trigger holes that fire on it: what changing it also runs.
fn fires(e: &Entity, doc: &Document) -> Vec<String> {
    doc.holes
        .iter()
        .filter_map(|h| match &h.kind {
            HoleKind::Trigger(t) if t.on.name == e.name => Some((h, t)),
            _ => None,
        })
        .map(|(h, t)| {
            let verbs: Vec<&str> = t
                .events
                .iter()
                .map(|a| match a {
                    Action::Insert => "Creating",
                    Action::Update => "Changing",
                    _ => "Deleting",
                })
                .collect();
            // "Creating or deleting" reads as one verb phrase: only the first is capitalised.
            let verb = verbs
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    if i == 0 {
                        v.to_string()
                    } else {
                        v.to_lowercase()
                    }
                })
                .collect::<Vec<_>>();
            let changes = if h.writes.is_empty() {
                String::new()
            } else {
                let many: Vec<String> = h.writes.iter().map(|w| plural(&w.name)).collect();
                format!(", which changes {}", many.join(", "))
            };
            let one = singular(&e.name);
            format!(
                "{} {} {one} also runs {}{changes}.",
                list(&verb, "or"),
                article(&one),
                h.name
            )
        })
        .collect()
}

fn hole(h: &Hole, entities: &HashMap<&str, &Entity>) -> Vec<String> {
    let mut out = Vec::new();
    let scheduled = starts(h);
    if let Some(s) = &scheduled {
        out.push(s.clone());
    }
    if !h.input.is_empty() {
        let params: Vec<String> = h
            .input
            .iter()
            .map(|p| format!("{} ({})", label(&p.name), param_words(&p.ty, entities)))
            .collect();
        out.push(format!("takes: {}", params.join(", ")));
    }
    if scheduled.is_none() {
        out.push(format!(
            "returns: {}",
            h.output
                .as_ref()
                .map_or("nothing".into(), |t| param_words(t, entities))
        ));
    }
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
    if scheduled.is_none() {
        out.push(match h.when.as_deref().and_then(|w| expr::parse(w).ok()) {
            Some(e) => format!(
                "who can call it: {who}, only when {}",
                english(&e, Style::Hole)
            ),
            None => format!("who can call it: {who}"),
        });
    }
    // A trigger runs as its own role, so what it changes doesn't depend on who made the change.
    let whoever = if matches!(h.kind, HoleKind::Trigger(_)) {
        ", whoever made the change"
    } else {
        ""
    };
    out.push(match (h.reads.is_empty(), h.writes.is_empty()) {
        (true, true) => "can't read or change any data".into(),
        (false, true) => format!("can read: {}. Can't change any data.", names(&h.reads)),
        (true, false) => format!("can change: {}{whoever}.", names(&h.writes)),
        (false, false) => format!(
            "can read: {}. Can change: {}{whoever}.",
            names(&h.reads),
            names(&h.writes)
        ),
    });
    if !h.grants.is_empty() {
        let roles: Vec<&str> = h.grants.iter().map(|g| g.name.as_str()).collect();
        let mut line = if let [only] = roles.as_slice() {
            format!("can give people the {only} role, or take it away, and no other role.")
        } else {
            format!(
                "can give people the {} roles, or take them away, and no other role.",
                roles.join(" or ")
            )
        };
        if scheduled.is_some() {
            line += " Nobody is asked first: it happens by itself.";
        }
        out.push(line);
    }
    if !h.require.is_empty() {
        let rules: Vec<String> = h
            .require
            .iter()
            .filter_map(|r| expr::parse(r).ok())
            .map(|e| english(&e, Style::Hole))
            .collect();
        out.push(format!(
            "a call with bad input is refused unless {}.",
            rules.join(", and ")
        ));
    }
    if let Some(key) = &h.idempotent {
        out.push(format!(
            "a call repeated with the same {key} by the same person returns the first answer and runs nothing; one repeated with different details is refused. The keys are kept for good."
        ));
    }
    if h.audit {
        out.push(
            "every call is recorded in the audit log, with who made it and what they sent.".into(),
        );
    }
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
        Code::Svt040 => {
            let name = d.path.split('.').nth(1).unwrap_or_default();
            format!(
                "{name} runs by itself and gives or takes away roles, so nobody is asked first."
            )
        }
        _ => d.message.clone(),
    }
}
