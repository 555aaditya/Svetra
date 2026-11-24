//! Access tests (docs/compiler.md, Access tests): pgTAP checks that the database enforces
//! the rules in `svetra.yaml`, run as real users. Expected results come from evaluating
//! the rules here, never from the generated SQL, so the two can disagree when one is wrong.

use super::names::{lit, q, table};
use crate::diag::{Code, Diagnostic};
use crate::expr::{self, Expr, Op};
use crate::model::*;
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::HashMap;

const ROWS: [char; 4] = ['a', 'b', 'c', 'd'];

/// A value as the rules see it. Uuids and enums are text. Times are normalised UTC text,
/// `YYYY-MM-DDTHH:MM:SS` for timestamps and `YYYY-MM-DD` for dates, so they sort as text.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    Null,
    Bool(bool),
    Num(f64),
    Text(String),
    Time(String),
    Date(String),
}

struct Person {
    name: String,
    id: Option<String>,
    /// (role, scope id): `None` for a global role.
    grants: Vec<(String, Option<String>)>,
    /// The session was verified with a second factor (`aal2`).
    aal2: bool,
    /// Set for an owner who also holds a role: the statement, run as the database owner just
    /// before each check and undone with it, that gives alice that role. The role is not in the
    /// test data, so every other check still sees alice without it.
    before: Option<String>,
}

struct World<'a> {
    entities: Vec<&'a Entity>,
    by_name: HashMap<&'a str, usize>,
    alice: String,
    bob: String,
    /// How many of `entities` are the design's own (the rest are hole inputs).
    tables: usize,
    carol: String,
    /// A person with no checks, who exists only to key the fourth test row of an entity whose
    /// primary key is a `user` field (docs/compiler.md, The test rows).
    fourth: String,
    /// Roles with `mfa: true`: they count only in an `aal2` session.
    mfa: Vec<&'a str>,
}

fn person_id(n: usize) -> String {
    format!("5e7a0000-0000-4000-8000-{n:012x}")
}

impl<'a> World<'a> {
    fn entity(&self, name: &str) -> (usize, &'a Entity) {
        let i = self.by_name[name];
        (i, self.entities[i])
    }

    /// Which row of the target a ref field points at, from row `k` (0 to 3, `a` to `d`).
    /// The first ref to an entity follows the row (a, c to a; b, d to b); a second one flips.
    fn ref_row(&self, e: &Entity, field: &str, k: usize) -> usize {
        let target = |f: &Field| match &f.ty {
            FieldType::Ref(t) => Some(t.name.clone()),
            _ => None,
        };
        let f = e
            .fields
            .iter()
            .find(|f| f.name == field)
            .expect("validated field");
        let t = target(f).expect("a ref field");
        let nth = e
            .fields
            .iter()
            .take_while(|x| x.name != field)
            .filter(|x| target(x).as_deref() == Some(t.as_str()))
            .count();
        (k % 2) ^ (nth % 2)
    }

    /// The stored rows (`a` and `b` of each of the design's entities) that point at row `tk` of
    /// entity `ti`, with the `on_delete` of the field that does.
    fn pointing_at(&self, ti: usize, tk: usize) -> Vec<(usize, usize, OnDelete)> {
        let mut out = Vec::new();
        for (si, source) in self.entities.iter().enumerate().take(self.tables) {
            for sk in 0..2 {
                for f in &source.fields {
                    if let FieldType::Ref(t) = &f.ty
                        && t.name == self.entities[ti].name
                        && self.ref_row(source, &f.name, sk) == tk
                    {
                        out.push((si, sk, f.on_delete));
                    }
                }
            }
        }
        out
    }

    /// Why deleting the stored row `k` of entity `ei` fails, if it does (docs/format.md, Deleting
    /// a referenced row): the entity and letter of a stored row that still points, through a
    /// `restrict` field, at a row the delete removes. The delete removes the row and, through
    /// `cascade`, every row that points at a removed row; a row removed anyway (itself, or by
    /// cascade) blocks nothing. This reads the `on_delete` options, never the generated keys.
    fn delete_blocker(&self, ei: usize, k: usize) -> Option<(String, char)> {
        let mut gone = vec![(ei, k)];
        let mut next = 0;
        while let Some(&(ti, tk)) = gone.get(next) {
            next += 1;
            for (si, sk, on) in self.pointing_at(ti, tk) {
                if on == OnDelete::Cascade && !gone.contains(&(si, sk)) {
                    gone.push((si, sk));
                }
            }
        }
        gone.iter().find_map(|&(ti, tk)| {
            self.pointing_at(ti, tk)
                .into_iter()
                .find(|&(si, sk, on)| on == OnDelete::Restrict && !gone.contains(&(si, sk)))
                .map(|(si, sk, _)| (self.entities[si].name.clone(), ROWS[sk]))
        })
    }

    /// The SQL literal and rule value of `field` on row `k` of entity `ei`.
    fn value(&self, ei: usize, field: &Field, k: usize) -> (String, Val) {
        let e = self.entities[ei];
        let r = ROWS[k];
        let text = |s: String| (lit(&s), Val::Text(s));
        let num = |n: f64| (format!("{n}"), Val::Num(n));
        match &field.ty {
            FieldType::Uuid if field.primary => {
                text(format!("5e7a0000-0000-4000-8001-{:04x}0000000{r}", ei + 1))
            }
            FieldType::Uuid => {
                let fi = e
                    .fields
                    .iter()
                    .position(|f| f.name == field.name)
                    .expect("own field");
                text(format!(
                    "5e7a0000-0000-4000-8002-{:04x}{:04x}000{r}",
                    ei + 1,
                    fi + 1
                ))
            }
            FieldType::Int if field.primary => num(2_000_000_001.0 + k as f64),
            FieldType::Int | FieldType::Decimal { .. } | FieldType::Money { .. } => {
                num(k as f64 + 1.0)
            }
            FieldType::Float => num(k as f64 + 1.5),
            FieldType::Text => text(format!("svetra {r}")),
            FieldType::Bool => {
                let b = k.is_multiple_of(2);
                (b.to_string(), Val::Bool(b))
            }
            FieldType::Timestamp => {
                let t = format!("2030-01-0{}T10:00:00", k + 1);
                (lit(&format!("{t}Z")), Val::Time(t))
            }
            FieldType::Date => {
                let d = format!("2030-01-0{}", k + 1);
                (lit(&d), Val::Date(d))
            }
            FieldType::Enum(values) => text(values[k % values.len()].clone()),
            FieldType::Json => text(format!("{{\"svetra\": \"{r}\"}}")),
            // A `user` primary key is one row per person, so the four rows have four different keys:
            // c and d can't repeat the people of a and b, whose rows are already stored.
            FieldType::User if field.primary => {
                text([&self.alice, &self.bob, &self.carol, &self.fourth][k].clone())
            }
            // The first `user` field is alice on rows a and c and bob on b and d; a second one flips,
            // so a rule that compares two people (an author and a reviewer) sees two different ones.
            FieldType::User => {
                let nth = e
                    .fields
                    .iter()
                    .take_while(|x| x.name != field.name)
                    .filter(|x| x.ty == FieldType::User && !x.primary)
                    .count();
                text(
                    if k.is_multiple_of(2) == nth.is_multiple_of(2) {
                        &self.alice
                    } else {
                        &self.bob
                    }
                    .clone(),
                )
            }
            FieldType::Ref(t) => {
                let (ti, target) = self.entity(&t.name);
                let pk = primary(target);
                self.value(ti, pk, self.ref_row(e, &field.name, k))
            }
        }
    }

    fn field_value(&self, ei: usize, name: &str, k: usize) -> Val {
        let e = self.entities[ei];
        let f = e
            .fields
            .iter()
            .find(|f| f.name == name)
            .expect("validated field");
        self.value(ei, f, k).1
    }
}

fn primary(e: &Entity) -> &Field {
    e.fields
        .iter()
        .find(|f| f.primary)
        .expect("validated primary key")
}

/// One field of the checked row, given a new value, as an update would leave it.
struct Swap<'s> {
    field: &'s str,
    to: SwapTo,
    /// Another field the same update changes.
    and: Option<&'s Swap<'s>>,
}

impl Swap<'_> {
    /// The new value of `field` in this update, if it changes it.
    fn of(&self, field: &str) -> Option<&SwapTo> {
        if self.field == field {
            Some(&self.to)
        } else {
            self.and.and_then(|s| s.of(field))
        }
    }

    /// Every field this update changes.
    fn fields(&self) -> Vec<&str> {
        let mut out = vec![self.field];
        if let Some(s) = self.and {
            out.extend(s.fields());
        }
        out
    }
}

#[derive(Clone)]
enum SwapTo {
    /// A `user` field set to this person's id.
    User(String),
    /// A `ref` field pointed at this row of its target.
    Row(usize),
    /// An enum or bool field set to this value.
    Value(Val),
}

/// Where a rule is evaluated: one row of one entity, for one person.
struct Env<'w, 'a> {
    world: &'w World<'a>,
    ei: usize,
    k: usize,
    person: &'w Person,
    /// The row after an update that changes one field; `None` for the stored row.
    swap: Option<&'w Swap<'w>>,
    /// Set when a rule compares a time with text that can't be read as an ISO time.
    unsure: &'w Cell<bool>,
}

fn truth(v: &Val) -> Option<bool> {
    match v {
        Val::Bool(b) => Some(*b),
        _ => None,
    }
}

fn from_truth(t: Option<bool>) -> Val {
    t.map_or(Val::Null, Val::Bool)
}

/// Text that Postgres reads as a UTC time: `YYYY-MM-DD`, optionally followed by `T` or a
/// space, `HH:MM` and `:SS`, then optionally `Z` or `+00:00`. Returns the date and the time.
fn iso(s: &str) -> Option<(String, String)> {
    let digits = |x: &str| x.chars().all(|c| c.is_ascii_digit());
    let (date, rest) = (s.get(..10)?, &s[10..]);
    let d: Vec<&str> = date.split('-').collect();
    if !(d.len() == 3
        && d[0].len() == 4
        && d[1].len() == 2
        && d[2].len() == 2
        && d.iter().all(|p| digits(p)))
    {
        return None;
    }
    if rest.is_empty() {
        return Some((date.into(), "00:00:00".into()));
    }
    let rest = rest.strip_prefix(['T', ' '])?;
    let rest = ["Z", "+00:00", "+00"]
        .iter()
        .find_map(|z| rest.strip_suffix(z))
        .unwrap_or(rest);
    let t: Vec<&str> = rest.split(':').collect();
    if !((t.len() == 2 || t.len() == 3) && t.iter().all(|p| p.len() == 2 && digits(p))) {
        return None;
    }
    Some((
        date.into(),
        if t.len() == 2 {
            format!("{rest}:00")
        } else {
            rest.into()
        },
    ))
}

/// SQL comparison: `None` when either side is null, or when a time is compared with text
/// that isn't an ISO time (which also marks the rule as unsure).
fn compare(l: &Val, r: &Val, env: &Env) -> Option<Ordering> {
    let unsure = || {
        env.unsure.set(true);
        None
    };
    match (l, r) {
        (Val::Num(a), Val::Num(b)) => a.partial_cmp(b),
        (Val::Text(a), Val::Text(b))
        | (Val::Time(a), Val::Time(b))
        | (Val::Date(a), Val::Date(b)) => Some(a.as_bytes().cmp(b.as_bytes())),
        (Val::Bool(a), Val::Bool(b)) => Some(a.cmp(b)),
        // Postgres reads the text as the other side's type: a date drops the time of day.
        (Val::Time(a), Val::Text(b)) => {
            iso(b).map_or_else(unsure, |(d, t)| Some(a.cmp(&format!("{d}T{t}"))))
        }
        (Val::Date(a), Val::Text(b)) => iso(b).map_or_else(unsure, |(d, _)| Some(a.cmp(&d))),
        (Val::Text(_), Val::Time(_) | Val::Date(_)) => compare(r, l, env).map(Ordering::reverse),
        // A date meets a timestamp at midnight.
        (Val::Date(a), Val::Time(b)) => Some(format!("{a}T00:00:00").cmp(b)),
        (Val::Time(_), Val::Date(_)) => compare(r, l, env).map(Ordering::reverse),
        _ => None,
    }
}

fn eval(e: &Expr, env: &Env) -> Val {
    match e {
        Expr::Num(n) => Val::Num(*n),
        Expr::Str(s) => Val::Text(s.clone()),
        Expr::Bool(b) => Val::Bool(*b),
        Expr::Null => Val::Null,
        Expr::Path(p) => match p.as_slice() {
            [a, b] if a == "auth" && b == "user" => {
                env.person.id.clone().map_or(Val::Null, Val::Text)
            }
            [f] => match env.swap.and_then(|s| s.of(f)) {
                Some(SwapTo::User(id)) => Val::Text(id.clone()),
                Some(SwapTo::Value(v)) => v.clone(),
                Some(SwapTo::Row(j)) => {
                    let e = env.world.entities[env.ei];
                    let FieldType::Ref(t) =
                        &e.fields.iter().find(|x| &x.name == f).expect("a ref").ty
                    else {
                        unreachable!("only refs point at rows")
                    };
                    let (ti, te) = env.world.entity(&t.name);
                    env.world.value(ti, primary(te), *j).1
                }
                _ => env.world.field_value(env.ei, f, env.k),
            },
            [r, f] => {
                let e = env.world.entities[env.ei];
                let row = match env.swap.and_then(|s| s.of(r)) {
                    Some(SwapTo::Row(j)) => *j,
                    _ => env.world.ref_row(e, r, env.k),
                };
                let FieldType::Ref(t) = &e
                    .fields
                    .iter()
                    .find(|x| &x.name == r)
                    .expect("typeck resolved the ref")
                    .ty
                else {
                    unreachable!("typeck checks one-hop paths")
                };
                env.world
                    .field_value(env.world.by_name[t.name.as_str()], f, row)
            }
            _ => unreachable!("typeck rejects deeper paths"),
        },
        Expr::Call(_, args) => {
            let (role, scope) = match args.as_slice() {
                [Expr::Str(r)] => (r, None),
                [Expr::Str(r), s] => match eval(s, env) {
                    Val::Text(id) => (r, Some(id)),
                    _ => return Val::Bool(false),
                },
                _ => unreachable!("typeck checks has_role's arguments"),
            };
            Val::Bool(
                env.person
                    .grants
                    .iter()
                    .any(|(g, s)| g == role && *s == scope)
                    && (env.person.aal2 || !env.world.mfa.contains(&role.as_str())),
            )
        }
        Expr::Neg(x) => match eval(x, env) {
            Val::Num(n) => Val::Num(-n),
            _ => Val::Null,
        },
        Expr::Not(x) => from_truth(truth(&eval(x, env)).map(|b| !b)),
        Expr::In(x, items) => {
            let v = eval(x, env);
            let hits: Vec<Option<bool>> = items
                .iter()
                .map(|i| compare(&v, &eval(i, env), env).map(|o| o.is_eq()))
                .collect();
            if hits.contains(&Some(true)) {
                Val::Bool(true)
            } else if hits.contains(&None) {
                Val::Null
            } else {
                Val::Bool(false)
            }
        }
        // `x == null` compiles to `is null`, so it's never null itself.
        Expr::Bin(op @ (Op::Eq | Op::Ne), l, r)
            if matches!(**l, Expr::Null) || matches!(**r, Expr::Null) =>
        {
            let x = if matches!(**l, Expr::Null) { r } else { l };
            Val::Bool((eval(x, env) == Val::Null) == (*op == Op::Eq))
        }
        Expr::Bin(Op::And, l, r) => match (truth(&eval(l, env)), truth(&eval(r, env))) {
            (Some(false), _) | (_, Some(false)) => Val::Bool(false),
            (Some(true), Some(true)) => Val::Bool(true),
            _ => Val::Null,
        },
        Expr::Bin(Op::Or, l, r) => match (truth(&eval(l, env)), truth(&eval(r, env))) {
            (Some(true), _) | (_, Some(true)) => Val::Bool(true),
            (Some(false), Some(false)) => Val::Bool(false),
            _ => Val::Null,
        },
        Expr::Bin(op, l, r) => {
            let (l, r) = (eval(l, env), eval(r, env));
            if let (Val::Num(a), Val::Num(b)) = (&l, &r) {
                match op {
                    Op::Add => return Val::Num(a + b),
                    Op::Sub => return Val::Num(a - b),
                    Op::Mul => return Val::Num(a * b),
                    Op::Div if *b != 0.0 => return Val::Num(a / b),
                    Op::Div => return Val::Null,
                    _ => {}
                }
            }
            let Some(o) = compare(&l, &r, env) else {
                return Val::Null;
            };
            Val::Bool(match op {
                Op::Eq => o.is_eq(),
                Op::Ne => o.is_ne(),
                Op::Lt => o.is_lt(),
                Op::Le => o.is_le(),
                Op::Gt => o.is_gt(),
                Op::Ge => o.is_ge(),
                _ => unreachable!("arithmetic on non-numbers fails typeck"),
            })
        }
    }
}

/// A field, and each value to set it to: (SQL literal, rule value, words).
type Transition<'a> = (&'a Field, Vec<(String, Val, String)>);

/// The own fields an expression reads (one-segment paths).
fn reads(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Path(p) if p.len() == 1 => out.push(p[0].clone()),
        Expr::Call(_, args) => args.iter().for_each(|a| reads(a, out)),
        Expr::Neg(x) | Expr::Not(x) => reads(x, out),
        Expr::In(x, items) => {
            reads(x, out);
            items.iter().for_each(|i| reads(i, out));
        }
        Expr::Bin(_, l, r) => {
            reads(l, out);
            reads(r, out);
        }
        _ => {}
    }
}

/// Which row an update's rule is read on: the row as it is (`when`, PostgreSQL's `USING`) or as the
/// update leaves it (`check`, or `when` without one, PostgreSQL's `WITH CHECK`). Only an update
/// has a `check`, so for every other action the two are the same.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Old,
    New,
}

fn condition(p: &Policy, phase: Phase) -> &str {
    match phase {
        Phase::New => p.check.as_deref().unwrap_or(&p.when),
        Phase::Old => &p.when,
    }
}

/// Does the policy apply to this person's database role, on this entity and action?
fn covers(p: &Policy, person: &Person, entity: &str, a: Action) -> bool {
    let applies = match p.role {
        Role::Anyone => true,
        Role::Anon => person.id.is_none(),
        Role::Authenticated => person.id.is_some(),
    };
    applies && p.on.name == entity && p.allow.contains(&a)
}

/// `Some(true)` or `Some(false)` for the rule on this row, `None` when it can't be evaluated.
fn holds(
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    swap: Option<&Swap>,
    rule: &str,
) -> Option<bool> {
    let unsure = Cell::new(false);
    let env = Env {
        world,
        ei,
        k,
        person,
        swap,
        unsure: &unsure,
    };
    let v = eval(&expr::parse(rule).expect("validated"), &env);
    if unsure.get() {
        None
    } else {
        Some(v == Val::Bool(true))
    }
}

/// Does some rule let `person` do `action` on row `k` of entity `ei`? `None` when a rule that
/// could decide it can't be evaluated, so the check is skipped rather than guessed. As in
/// PostgreSQL: some permissive policy must hold (they combine with `or`), and every restrictive
/// one must hold too; null counts as not holding.
#[allow(clippy::too_many_arguments)]
fn allowed(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    a: Action,
    phase: Phase,
    swap: Option<&Swap>,
) -> Option<bool> {
    let name = &world.entities[ei].name;
    let (mut granted, mut grant_unknown) = (false, false);
    let (mut refused, mut limit_unknown) = (false, false);
    for p in doc.policies.iter().filter(|p| covers(p, person, name, a)) {
        match holds(world, person, ei, k, swap, condition(p, phase)) {
            None if p.restrictive => limit_unknown = true,
            None => grant_unknown = true,
            Some(held) if p.restrictive => refused |= !held,
            Some(held) => granted |= held,
        }
    }
    let permissive = if granted {
        Some(true)
    } else if grant_unknown {
        None
    } else {
        Some(false)
    };
    let restrictive = if refused {
        Some(false)
    } else if limit_unknown {
        None
    } else {
        Some(true)
    };
    both(permissive, restrictive)
}

/// An update by `person` of row `k`, with `swap` applied to the new row: the old row must pass
/// the update rules and the new row must pass them again (`check`), and, because the generated
/// update reads the row with a `where`, both rows must pass the select rules too.
fn update_allowed(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    swap: Option<&Swap>,
) -> Option<bool> {
    let old = both(
        allowed(doc, world, person, ei, k, Action::Select, Phase::Old, None),
        allowed(doc, world, person, ei, k, Action::Update, Phase::Old, None),
    );
    let new = both(
        allowed(doc, world, person, ei, k, Action::Update, Phase::New, swap),
        allowed(doc, world, person, ei, k, Action::Select, Phase::Old, swap),
    );
    both(old, new)
}

/// Either holds: `None` if either is unknown and the other isn't already true.
fn either(x: Option<bool>, y: Option<bool>) -> Option<bool> {
    match (x, y) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

/// Does the policy leave out `field` from its `only`? A policy without `only` limits nothing.
fn limits(p: &Policy, field: &str) -> bool {
    p.only
        .as_ref()
        .is_some_and(|only| !only.iter().any(|f| f.name == field))
}

/// Is `field` locked for an update by `person` of row `k` (docs/column-rules.md, section 2)? A
/// field is locked when an applicable restrictive policy leaves it out of its `only`, or when no
/// applicable permissive policy leaves it free (no `only`, or the field is listed). A policy is
/// applicable when its role matches and its `when` holds on the old row. A field no policy
/// limits is never locked, and has no guard.
fn locked(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    field: &str,
) -> Option<bool> {
    let name = &world.entities[ei].name;
    if !doc
        .policies
        .iter()
        .any(|p| &p.on.name == name && p.allow.contains(&Action::Update) && limits(p, field))
    {
        return Some(false);
    }
    let applicable = |restrictive: bool, wanted: &dyn Fn(&Policy) -> bool| -> Option<bool> {
        let mut unknown = false;
        for p in doc.policies.iter().filter(|p| {
            p.restrictive == restrictive && covers(p, person, name, Action::Update) && wanted(p)
        }) {
            match holds(world, person, ei, k, None, &p.when) {
                Some(true) => return Some(true),
                None => unknown = true,
                Some(false) => {}
            }
        }
        if unknown { None } else { Some(false) }
    };
    let restricted = applicable(true, &|p| limits(p, field));
    let freed = applicable(false, &|p| !limits(p, field));
    either(restricted, freed.map(|f| !f))
}

/// An update by `person` of row `k` is allowed by the rules, and changes no locked field.
fn update_expected(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    swap: Option<&Swap>,
) -> Option<bool> {
    let mut ok = update_allowed(doc, world, person, ei, k, swap);
    for field in swap.map(Swap::fields).unwrap_or_default() {
        ok = both(ok, locked(doc, world, person, ei, k, field).map(|l| !l));
    }
    ok
}

/// The same update, but allowed by one single policy: the old row passes *its* `when` and the new
/// row passes *its* `check`. PostgreSQL combines the two across policies independently, so an
/// update can be allowed although no one policy allows it (SVT034).
fn update_allowed_by_one(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    swap: Option<&Swap>,
) -> Option<bool> {
    let name = &world.entities[ei].name;
    let selects = both(
        allowed(doc, world, person, ei, k, Action::Select, Phase::Old, None),
        allowed(doc, world, person, ei, k, Action::Select, Phase::Old, swap),
    );
    let restricted = both(
        allowed_restrictions(doc, world, person, ei, k, Phase::Old, None),
        allowed_restrictions(doc, world, person, ei, k, Phase::New, swap),
    );
    let mut unknown = false;
    for p in doc
        .policies
        .iter()
        .filter(|p| !p.restrictive && covers(p, person, name, Action::Update))
    {
        let both_rows = both(
            holds(world, person, ei, k, None, condition(p, Phase::Old)),
            holds(world, person, ei, k, swap, condition(p, Phase::New)),
        );
        match both_rows {
            Some(true) => return both(Some(true), both(selects, restricted)),
            None => unknown = true,
            Some(false) => {}
        }
    }
    if unknown { None } else { Some(false) }
}

/// Every restrictive update policy holds on the row (the restrictive half of `allowed`).
fn allowed_restrictions(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    phase: Phase,
    swap: Option<&Swap>,
) -> Option<bool> {
    let name = &world.entities[ei].name;
    let (mut refused, mut unknown) = (false, false);
    for p in doc
        .policies
        .iter()
        .filter(|p| p.restrictive && covers(p, person, name, Action::Update))
    {
        match holds(world, person, ei, k, swap, condition(p, phase)) {
            None => unknown = true,
            Some(held) => refused |= !held,
        }
    }
    if refused {
        Some(false)
    } else if unknown {
        None
    } else {
        Some(true)
    }
}

/// Both must hold: `None` if either is unknown and the other doesn't already rule it out.
fn both(x: Option<bool>, y: Option<bool>) -> Option<bool> {
    match (x, y) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

/// The names of the permissive rules that let `person` do `action` on row `k` of entity `ei`.
fn allowing(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    a: Action,
) -> Vec<String> {
    let name = &world.entities[ei].name;
    doc.policies
        .iter()
        .filter(|p| !p.restrictive && covers(p, person, name, a))
        .filter(|p| holds(world, person, ei, k, None, &p.when) == Some(true))
        .map(|p| p.name.clone())
        .collect()
}

/// One check in the generated file, as data: what `svetra view` shows (docs/view.md).
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// The person's name in the tests: `anon`, `alice`, `admin`, `staff at Location a`, ...
    pub person: String,
    /// An entity, or for `call`, a hole.
    pub entity: String,
    /// `see`, `create`, `change`, `delete` or `call`.
    pub action: &'static str,
    /// `a` to `d`.
    pub row: char,
    /// Allowed, denied, or `None` when the evaluator can't decide and the check is skipped.
    pub expected: Option<bool>,
    /// Its number in the pgTAP output.
    pub number: usize,
    /// The rules that allow it.
    pub because: Vec<String>,
    /// For a `change` check: the update policies with an `only` (docs/column-rules.md) that
    /// cover this person's database role, whatever the row. The test rows can't show every
    /// person such a policy applies to (a reviewer needs a submitted document), so this is not
    /// decided per row.
    pub column_rules: Vec<String>,
}

/// The people the tests run as, in order, with what they hold.
#[derive(Debug, Clone, PartialEq)]
pub struct TestPerson {
    pub name: String,
    pub signed_in: bool,
    /// (role, scope entity) for a role holder.
    pub role: Option<(String, Option<String>)>,
    pub second_factor: bool,
}

/// An update PostgreSQL allows although no single policy does (SVT034): the old row passes one
/// policy's `when`, and the new row passes another's `check`.
#[derive(Debug, Clone, PartialEq)]
pub struct Hazard {
    pub entity: String,
    pub person: String,
    /// The change, as a sentence: "set the status of Doc a to submitted".
    pub what: String,
    /// The policies whose `when` holds on the row as it is.
    pub from: Vec<String>,
    /// The policies whose `check` holds on the row afterwards.
    pub to: Vec<String>,
}

/// A field one policy limits and another frees, for the same person and row (SVT036): the first
/// policy's `only` has no effect for them.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlap {
    pub entity: String,
    pub person: String,
    pub field: String,
    /// Policies whose `only` leaves the field out.
    pub limiting: Vec<String>,
    /// Policies that leave it free, so it stays free.
    pub freeing: Vec<String>,
}

/// The generated test file, and its checks as data.
pub struct Analysis {
    pub text: String,
    pub people: Vec<TestPerson>,
    pub checks: Vec<Check>,
    pub hazards: Vec<Hazard>,
    pub overlaps: Vec<Overlap>,
}

/// The permissive update policies that cover `person` on row `k`, split by whether they limit
/// `field`. Both non-empty means the limit changes nothing for them.
fn overlap(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    field: &str,
) -> Option<(Vec<String>, Vec<String>)> {
    let name = &world.entities[ei].name;
    let applicable: Vec<&Policy> = doc
        .policies
        .iter()
        .filter(|p| !p.restrictive && covers(p, person, name, Action::Update))
        .filter(|p| holds(world, person, ei, k, None, &p.when) == Some(true))
        .collect();
    let names = |limited: bool| -> Vec<String> {
        applicable
            .iter()
            .filter(|p| limits(p, field) == limited)
            .map(|p| p.name.clone())
            .collect()
    };
    let (limiting, freeing) = (names(true), names(false));
    (!limiting.is_empty() && !freeing.is_empty()).then_some((limiting, freeing))
}

/// Records the update as a hazard when PostgreSQL allows it and no one policy does.
#[allow(clippy::too_many_arguments)]
fn note_hazard(
    hazards: &mut Vec<Hazard>,
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    swap: &Swap,
    what: &str,
    expected: Option<bool>,
) {
    if expected != Some(true)
        || update_allowed_by_one(doc, world, person, ei, k, Some(swap)) != Some(false)
    {
        return;
    }
    let name = &world.entities[ei].name;
    let holding = |phase: Phase, swap: Option<&Swap>| -> Vec<String> {
        doc.policies
            .iter()
            .filter(|p| !p.restrictive && covers(p, person, name, Action::Update))
            .filter(|p| {
                let row = if phase == Phase::Old { None } else { swap };
                holds(world, person, ei, k, row, condition(p, phase)) == Some(true)
            })
            .map(|p| p.name.clone())
            .collect()
    };
    let (from, to) = (holding(Phase::Old, None), holding(Phase::New, Some(swap)));
    if !hazards
        .iter()
        .any(|h| h.entity == *name && h.from == from && h.to == to)
    {
        hazards.push(Hazard {
            entity: name.clone(),
            person: person.name.clone(),
            what: what.to_string(),
            from,
            to,
        });
    }
}

/// The SVT034 and SVT036 warnings for a validated design: updates only a *combination* of policies
/// allows, and limits that another policy covering the same person frees.
pub fn hazards(doc: &Document) -> Vec<Diagnostic> {
    // Needs two permissive update policies on one entity; skip the analysis otherwise.
    let combine = doc.entities.iter().any(|e| {
        doc.policies
            .iter()
            .filter(|p| !p.restrictive && p.on.name == e.name && p.allow.contains(&Action::Update))
            .count()
            >= 2
    });
    if !combine {
        return Vec::new();
    }
    let analysis = analyse(doc);
    let combined = analysis.hazards.iter().filter_map(|h| {
        let first = doc.policies.iter().find(|p| h.from.first() == Some(&p.name))?;
        Some(Diagnostic {
            code: Code::Svt034,
            path: format!("policies.{}", first.name),
            line: first.pos.line,
            col: first.pos.col,
            message: format!(
                "{} can {}: policy `{}` allows it on the row as it is and `{}` on the row afterwards, but neither allows it alone.",
                h.person,
                h.what,
                h.from.join("` or `"),
                h.to.join("` or `")
            ),
            hint: "Merge the two into one policy with a `when` and a `check`, or make their `check` conditions match.".into(),
        })
    });
    let freed = analysis.overlaps.iter().filter_map(|o| {
        let first = doc.policies.iter().find(|p| o.limiting.first() == Some(&p.name))?;
        Some(Diagnostic {
            code: Code::Svt036,
            path: format!("policies.{}.only", first.name),
            line: first.pos.line,
            col: first.pos.col,
            message: format!(
                "{} can change the {} of {} through `{}`, so the `only` of `{}` doesn't limit it for them.",
                o.person,
                o.field,
                o.entity,
                o.freeing.join("` or `"),
                o.limiting.join("` or `")
            ),
            hint: "Give the freeing policy an `only` too, or merge the two policies.".into(),
        })
    });
    combined.chain(freed).collect()
}

/// Entities in an order where every ref target comes first; file order breaks ties and cycles.
/// What changing one owner or reference field of row `k` would do, as a person `me` would do it:
/// a `user` field is set to `me` (or, if it already is, to the other test owner), a `ref` field is
/// pointed at the other row of its target. `None` when the new value could be refused for another
/// reason (a unique or constrained field), so the expected outcome would not be the rules'.
fn owner_swap(
    world: &World,
    e: &Entity,
    f: &Field,
    k: usize,
    me: &str,
) -> Option<(SwapTo, String, String)> {
    let ei = world.by_name[e.name.as_str()];
    let (to, value, words) = match &f.ty {
        FieldType::User => {
            let current = world.value(ei, f, k).1;
            let (id, who) = if current != Val::Text(me.to_string()) {
                (me.to_string(), "themselves")
            } else if current == Val::Text(world.alice.clone()) {
                (world.bob.clone(), "bob")
            } else {
                (world.alice.clone(), "alice")
            };
            (SwapTo::User(id.clone()), lit(&id), format!("to {who}"))
        }
        FieldType::Ref(t) => {
            let (ti, te) = world.entity(&t.name);
            let j = world.ref_row(e, &f.name, k) ^ 1;
            let words = format!("at {} {}", t.name, ROWS[j]);
            (SwapTo::Row(j), world.value(ti, primary(te), j).0, words)
        }
        _ => return None,
    };
    let lone_unique = f.unique
        || e.unique
            .iter()
            .any(|u| u.fields.len() == 1 && u.fields[0].name == f.name);
    let constrained = e
        .constraints
        .iter()
        .any(|c| c.expr.contains(f.name.as_str()));
    if constrained || (lone_unique && !matches!(&to, SwapTo::User(id) if id == me)) {
        return None;
    }
    Some((to, value, words))
}

fn insert_order(world: &World, tables: usize) -> Vec<usize> {
    let mut done: Vec<usize> = Vec::new();
    while done.len() < tables {
        let ready = (0..tables).find(|&i| {
            !done.contains(&i)
                && world.entities[i].fields.iter().all(|f| match &f.ty {
                    FieldType::Ref(t) => {
                        let ti = world.by_name[t.name.as_str()];
                        ti == i || done.contains(&ti)
                    }
                    _ => true,
                })
        });
        let next = ready.unwrap_or_else(|| {
            (0..tables)
                .find(|i| !done.contains(i))
                .expect("some entity is left")
        });
        done.push(next);
    }
    done
}

const HELPER: &str = r#"-- Run one statement as one person, then undo it. Returns allowed, denied, blocked or the error.
-- p_before, if given, is run first as the database owner (to give a person an extra role).
create schema svetra_test;
create function svetra_test.try(p_role text, p_user uuid, p_sql text, p_aal text default 'aal1', p_before text default null) returns text
language plpgsql as $$
declare n bigint;
begin
  begin
    -- p_before runs as the database owner, then is undone with the statement.
    if p_before is not null then execute p_before; end if;
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role, 'aal', p_aal)::text, true);
    execute p_sql;
    get diagnostics n = row_count;
    raise exception 'svetra_undo';
  exception when others then
    if sqlerrm = 'svetra_undo' then
      return case when n > 0 then 'allowed' else 'denied' end;
    elsif sqlstate = '42501' then
      return 'denied';
    -- A delete that a foreign key refuses: the row was visible and deletable, but another still points at it.
    elsif sqlstate = '23503' and sqlerrm like 'update or delete on table%' then
      return 'blocked';
    end if;
    return 'error ' || sqlstate || ': ' || sqlerrm;
  end;
end $$;
-- Call a hole as one person, then undo it. Refused means SQLSTATE 42501; anything else, even an
-- error from the hole's own code, means the call got past who-may-call-it.
create function svetra_test.call(p_role text, p_user uuid, p_sql text, p_aal text default 'aal1') returns text
language plpgsql as $$
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role, 'aal', p_aal)::text, true);
    execute p_sql;
    raise exception 'svetra_undo';
  exception when others then
    return case when sqlstate = '42501' then 'refused' else 'let through' end;
  end;
end $$;
-- What a role may do on a table, as "select, insert, update, delete" or "none".
create function svetra_test.privileges(p_role text, p_table text) returns text language sql as $$
  select coalesce(string_agg(v.p, ', ' order by v.o), 'none')
  from (values (1, 'select'), (2, 'insert'), (3, 'update'), (4, 'delete')) v(o, p)
  where has_table_privilege(p_role, p_table, v.p)
$$;
-- Whether a role may run some function with this name in this schema.
create function svetra_test.executes(p_role text, p_schema text, p_name text) returns boolean language sql as $$
  select coalesce(bool_or(has_function_privilege(p_role, p.oid, 'execute')), false)
  from pg_proc p join pg_namespace n on n.oid = p.pronamespace
  where n.nspname = p_schema and p.proname = p_name
$$;
"#;

/// Only in a file with triggered holes: the condition in a trigger's definition (`pg_get_triggerdef`),
/// and the same read back from a probe trigger made with the compiled text, so the two are compared
/// in PostgreSQL's own normal form. The probe is dropped again.
const TRIGGER_HELPER: &str = r#"create function svetra_test.noop() returns trigger language plpgsql as $$ begin return null; end $$;
create function svetra_test.when_of(p_trigger oid) returns text language sql as $$
  select substring(pg_get_triggerdef(p_trigger) from ' WHEN \((.*)\) EXECUTE ') $$;
create function svetra_test.trigger_when(p_table text, p_event text, p_cond text) returns text
language plpgsql as $$
declare r text;
begin
  if p_cond is null then return null; end if;
  execute format('create trigger svetra_test_when after %s on %s for each row when (%s) execute function svetra_test.noop()', p_event, p_table, p_cond);
  select svetra_test.when_of(t.oid) into r from pg_trigger t where t.tgname = 'svetra_test_when';
  execute format('drop trigger svetra_test_when on %s', p_table);
  return r;
end $$;
"#;

/// Only a design with `require` gets it, so the other test files are unchanged.
const INPUT_HELPER: &str = r#"-- Call a hole as one person, then undo it. A refusal for bad input (SQLSTATE 22023) returns its
-- message; refused (42501) and anything else return what `call` does.
create function svetra_test.call_input(p_role text, p_user uuid, p_sql text, p_aal text default 'aal1') returns text
language plpgsql as $$
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role, 'aal', p_aal)::text, true);
    execute p_sql;
    raise exception 'svetra_undo';
  exception when others then
    return case when sqlstate = '42501' then 'refused' when sqlstate = '22023' then sqlerrm else 'let through' end;
  end;
end $$;
"#;

/// The whole test file for a validated document.
pub fn tests(doc: &Document) -> String {
    analyse(doc).text
}

/// The test file, plus each check on a row and each hole call as data.
pub fn analyse(doc: &Document) -> Analysis {
    let mut people = vec![
        Person {
            name: "anon".into(),
            id: None,
            grants: vec![],
            aal2: false,
            before: None,
        },
        Person {
            name: "alice".into(),
            id: Some(person_id(1)),
            grants: vec![],
            aal2: false,
            before: None,
        },
        Person {
            name: "bob".into(),
            id: Some(person_id(2)),
            grants: vec![],
            aal2: false,
            before: None,
        },
        // Owns nothing and holds no role, in a session with a second factor (anyone can add
        // one): a rule open to every signed-in person, or a role check that ignores who holds
        // the role, shows up as carol getting something.
        Person {
            name: "carol".into(),
            id: Some(person_id(3)),
            grants: vec![],
            aal2: true,
            before: None,
        },
    ];
    // Each hole's inputs act as one more entity, so its `when` evaluates like a policy. Only the
    // first `tables` entities are tables.
    let tables = doc.entities.len();
    let by_name_src: std::collections::HashMap<&str, &Entity> =
        doc.entities.iter().map(|e| (e.name.as_str(), e)).collect();
    let hole_inputs: Vec<Entity> = doc
        .holes
        .iter()
        .map(|h| crate::typeck::hole_entity(h, &by_name_src))
        .collect();
    let all: Vec<&Entity> = doc.entities.iter().chain(&hole_inputs).collect();
    let world = World {
        by_name: all
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.as_str(), i))
            .collect(),
        entities: all,
        tables,
        alice: person_id(1),
        bob: person_id(2),
        carol: person_id(3),
        fourth: person_id(0xfff),
        mfa: doc
            .roles
            .iter()
            .filter(|r| r.mfa)
            .map(|r| r.name.as_str())
            .collect(),
    };
    let globals = doc.roles.iter().filter(|r| r.scope.is_none());
    let scoped = doc.roles.iter().filter(|r| r.scope.is_some());
    let holders: Vec<&RoleDef> = globals.chain(scoped).collect();
    let without: Vec<&RoleDef> = holders.iter().copied().filter(|r| r.mfa).collect();
    // Each role holder, then each mfa role again, held in a session without a second factor.
    for (r, aal2) in holders
        .iter()
        .map(|r| (r, r.mfa))
        .chain(without.iter().map(|r| (r, false)))
    {
        let id = person_id(people.len());
        let (name, scope) = match &r.scope {
            None => (r.name.clone(), None),
            Some(s) => {
                let (si, se) = world.entity(&s.name);
                let Val::Text(sid) = world.value(si, primary(se), 0).1 else {
                    unreachable!("scope entities have uuid keys")
                };
                (format!("{} at {} a", r.name, s.name), Some(sid))
            }
        };
        let name = if r.mfa && !aal2 {
            format!("{name} without a second factor")
        } else {
            name
        };
        people.push(Person {
            name,
            id: Some(id),
            grants: vec![(r.name.clone(), scope)],
            aal2,
            before: None,
        });
    }
    // Alice, who owns rows, also holding each role in turn (a scoped one at the other scope row
    // than her own rows point at). A person who is an owner and a role holder at once is where
    // two update policies can be combined: PostgreSQL ORs their `check`s independently.
    let mut owners_with_roles: Vec<Person> = Vec::new();
    if doc
        .entities
        .iter()
        .any(|e| e.fields.iter().any(|f| f.ty == FieldType::User))
    {
        for r in &holders {
            let (name, scope_sql, scope) = match &r.scope {
                None => (
                    format!("alice who is also {}", r.name),
                    "null".to_string(),
                    None,
                ),
                Some(s) => {
                    let (si, se) = world.entity(&s.name);
                    let (sql, Val::Text(sid)) = world.value(si, primary(se), 1) else {
                        unreachable!("scope entities have uuid keys")
                    };
                    (
                        format!("alice who is also {} at {} b", r.name, s.name),
                        sql,
                        Some(sid),
                    )
                }
            };
            let grant = format!(
                "insert into \"svetra\".\"role_grants\" (\"user_id\", \"role\", \"scope_id\") values ('{}', {}, {scope_sql})",
                world.alice,
                lit(&r.name)
            );
            owners_with_roles.push(Person {
                name,
                id: Some(world.alice.clone()),
                grants: vec![(r.name.clone(), scope)],
                aal2: r.mfa,
                before: Some(grant),
            });
        }
    }

    let mut setup = Vec::new();
    let mut users: Vec<String> = people
        .iter()
        .filter_map(|p| {
            p.id.as_ref()
                .map(|id| format!("('{id}', '{}@svetra.test')", p.name.replace(' ', ".")))
        })
        .collect();
    if doc
        .entities
        .iter()
        .any(|e| primary(e).ty == FieldType::User)
    {
        users.push(format!("('{}', 'fourth@svetra.test')", world.fourth));
    }
    setup.push(lives(
        &format!(
            "insert into auth.users (id, email) values {}",
            users.join(", ")
        ),
        "people",
    ));
    for p in &people {
        for (role, scope) in &p.grants {
            let scope = scope.as_ref().map_or("null".into(), |s| lit(s));
            let sql = format!(
                "insert into \"svetra\".\"role_grants\" (\"user_id\", \"role\", \"scope_id\") values ('{}', {}, {scope})",
                p.id.as_ref().expect("role holders are signed in"),
                lit(role)
            );
            setup.push(lives(&sql, &format!("{} holds {role}", p.name)));
        }
    }
    for ei in insert_order(&world, tables) {
        for (k, r) in ROWS.iter().enumerate().take(2) {
            let what = format!("{} {r}", world.entities[ei].name);
            setup.push(lives(&insert(&world, ei, k), &what));
        }
    }

    let mut checks = Vec::new();
    // Checks of triggered holes that read the catalog, run before the test data goes in, while
    // the triggers are still switched on.
    let mut early: Vec<String> = Vec::new();
    let mut found: Vec<Check> = Vec::new();
    // Giving alice a role, for the scope entity's `a` row when the role is scoped.
    let grant_to_alice = |r: &RoleDef| {
        let scope = match &r.scope {
            None => "null".to_string(),
            Some(s) => {
                let (si, se) = world.entity(&s.name);
                world.value(si, primary(se), 0).0
            }
        };
        format!(
            "insert into \"svetra\".\"role_grants\" (\"user_id\", \"role\", \"scope_id\") values ('{}', {}, {scope})",
            world.alice,
            lit(&r.name)
        )
    };
    // Roles are granted by the app's owner or a hole, never by the person holding them.
    if let Some(r) = doc.roles.first() {
        checks.push(format!(
            "select is(svetra_test.try('authenticated', '{}', $svetra${}$svetra$), 'denied', {});",
            world.alice,
            grant_to_alice(r),
            lit(&format!("alice can't give herself the {} role", r.name))
        ));
    }
    let mut hazards: Vec<Hazard> = Vec::new();
    let mut overlaps: Vec<Overlap> = Vec::new();
    for (ei, e) in world.entities.iter().enumerate().take(tables) {
        // The fields a rule with a `check` reads, and each value to try them at.
        let updates: Vec<&Policy> = doc
            .policies
            .iter()
            .filter(|p| p.on.name == e.name && p.allow.contains(&Action::Update))
            .collect();
        let mut transitions: Vec<Transition> = Vec::new();
        if updates.iter().any(|p| p.check.is_some()) {
            let mut mentioned = Vec::new();
            for p in &updates {
                for rule in [Some(&p.when), p.check.as_ref()].into_iter().flatten() {
                    reads(&expr::parse(rule).expect("validated"), &mut mentioned);
                }
            }
            for f in e
                .fields
                .iter()
                .filter(|f| !f.primary && mentioned.contains(&f.name))
            {
                // A value another row holds would break a one-field unique rule, and a
                // constraint naming the field could reject the new value: skip both.
                let alone = f.unique
                    || e.unique
                        .iter()
                        .any(|u| u.fields.len() == 1 && u.fields[0].name == f.name);
                if alone
                    || e.constraints
                        .iter()
                        .any(|c| c.expr.contains(f.name.as_str()))
                {
                    continue;
                }
                let values: Vec<(String, Val, String)> = match &f.ty {
                    FieldType::Enum(vs) => vs
                        .iter()
                        .map(|v| (lit(v), Val::Text(v.clone()), v.clone()))
                        .collect(),
                    FieldType::Bool => [true, false]
                        .iter()
                        .map(|b| (b.to_string(), Val::Bool(*b), b.to_string()))
                        .collect(),
                    _ => continue,
                };
                transitions.push((f, values));
            }
        }
        // An entity whose policies limit which fields an update may change gets checks for that.
        let guarded = updates.iter().any(|p| p.only.is_some());
        let skippable = |f: &Field| {
            f.unique
                || e.unique
                    .iter()
                    .any(|u| u.fields.len() == 1 && u.fields[0].name == f.name)
                || e.constraints
                    .iter()
                    .any(|c| c.expr.contains(f.name.as_str()))
        };
        for p in people.iter().chain(&owners_with_roles) {
            let mut check =
                |what: String,
                 sql: String,
                 ok: Option<bool>,
                 tag: Option<(&'static str, usize, Action)>| {
                    if let Some((action, k, a)) = tag
                        && p.before.is_none()
                    {
                        found.push(Check {
                            person: p.name.clone(),
                            entity: e.name.clone(),
                            action,
                            row: ROWS[k],
                            expected: ok,
                            number: checks.len(),
                            because: if e.private {
                                vec![]
                            } else {
                                allowing(doc, &world, p, ei, k, a)
                            },
                            column_rules: if a == Action::Update {
                                doc.policies
                                    .iter()
                                    .filter(|x| !x.restrictive && x.only.is_some())
                                    .filter(|x| covers(x, p, &e.name, Action::Update))
                                    .map(|x| x.name.clone())
                                    .collect()
                            } else {
                                vec![]
                            },
                        });
                    }
                    let role = if p.id.is_some() {
                        "authenticated"
                    } else {
                        "anon"
                    };
                    let user = p.id.as_ref().map_or("null".into(), |id| format!("'{id}'"));
                    // A delete the rules allow still fails while a `restrict` reference points at it.
                    let blocker = match (tag, ok) {
                        (Some(("delete", k, _)), Some(true)) => world.delete_blocker(ei, k),
                        _ => None,
                    };
                    let aal = match (&p.before, p.aal2) {
                        (Some(b), a2) => format!(
                            ", '{}', $svetra${b}$svetra$",
                            if a2 { "aal2" } else { "aal1" }
                        ),
                        (None, true) => ", 'aal2'".to_string(),
                        (None, false) => String::new(),
                    };
                    checks.push(match ok {
                    Some(_) if blocker.is_some() => {
                        let (by, r) = blocker.expect("checked");
                        format!(
                            "select is(svetra_test.try('{role}', {user}, $svetra${sql}$svetra${aal}), 'blocked', {});",
                            lit(&format!("{} can't {what}: {by} {r} still points at it", p.name))
                        )
                    }
                    Some(ok) => format!(
                        "select is(svetra_test.try('{role}', {user}, $svetra${sql}$svetra${aal}), '{}', {});",
                        if ok { "allowed" } else { "denied" },
                        lit(&format!("{} {} {what}", p.name, if ok { "can" } else { "can't" }))
                    ),
                    None => format!(
                        "select skip({}, 1);",
                        lit(&format!("{} {what}: a rule compares a time with text that isn't an ISO time", p.name))
                    ),
                });
                };
            let row = |verb: &str, k: usize| format!("{verb} {} {}", e.name, ROWS[k]);
            if e.private {
                if p.name == "anon" || p.name == "alice" {
                    let see = Some(("see", 0, Action::Select));
                    check(row("see", 0), select(&world, ei, 0), Some(false), see);
                    let create = Some(("create", 2, Action::Insert));
                    check(row("create", 2), insert(&world, ei, 2), Some(false), create);
                }
                continue;
            }
            let can = |k, a| allowed(doc, &world, p, ei, k, a, Phase::Old, None);
            for k in 0..2 {
                let tag = Some(("see", k, Action::Select));
                check(
                    row("see", k),
                    select(&world, ei, k),
                    can(k, Action::Select),
                    tag,
                );
            }
            for k in 0..2 {
                let ok = update_allowed(doc, &world, p, ei, k, None);
                let tag = Some(("change", k, Action::Update));
                check(row("change", k), update(&world, ei, k), ok, tag);
            }
            for k in 0..2 {
                let ok = both(can(k, Action::Select), can(k, Action::Delete));
                let tag = Some(("delete", k, Action::Delete));
                check(row("delete", k), delete(&world, ei, k), ok, tag);
            }
            for k in 2..4 {
                check(
                    row("create", k),
                    insert(&world, ei, k),
                    can(k, Action::Insert),
                    Some(("create", k, Action::Insert)),
                );
            }
            // Changing a field a rule with a `check` reads to each value it can take: a draft
            // becomes submitted, a submitted one approved, and nothing else (docs/update-check.md).
            for (k, r) in ROWS.iter().enumerate().take(2) {
                for (f, values) in &transitions {
                    for (sql_value, value, word) in values {
                        if &world.field_value(ei, &f.name, k) == value {
                            continue;
                        }
                        let swap = Swap {
                            field: &f.name,
                            to: SwapTo::Value(value.clone()),
                            and: None,
                        };
                        let sql = format!(
                            "update {} set {} = {sql_value} where {}",
                            table(&e.name),
                            q(&f.name),
                            key(&world, ei, k)
                        );
                        let expected = update_expected(doc, &world, p, ei, k, Some(&swap));
                        let what = format!("set the {} of {} {r} to {word}", f.name, e.name);
                        note_hazard(&mut hazards, doc, &world, p, ei, k, &swap, &what, expected);
                        check(what, sql, expected, None);
                    }
                }
            }
            // Fields an update may not change (docs/column-rules.md): each on its own, and each
            // beside a transition the ordinary rules allow, which only the guard can refuse.
            if guarded {
                for (k, r) in ROWS.iter().enumerate().take(2) {
                    for f in e.fields.iter().filter(|f| !f.primary) {
                        if let Some((limiting, freeing)) = overlap(doc, &world, p, ei, k, &f.name)
                            && !overlaps.iter().any(|o| {
                                o.entity == e.name && o.limiting == limiting && o.freeing == freeing
                            })
                        {
                            overlaps.push(Overlap {
                                entity: e.name.clone(),
                                person: p.name.clone(),
                                field: f.name.clone(),
                                limiting,
                                freeing,
                            });
                        }
                    }
                    // A change of each field to the other row's value. A `ref` can't be set to a
                    // value without its row, and skippable fields could be refused for another reason.
                    let changes: Vec<(&Field, String, Val)> = e
                        .fields
                        .iter()
                        .filter(|f| {
                            !f.primary && !matches!(f.ty, FieldType::Ref(_)) && !skippable(f)
                        })
                        .filter_map(|f| {
                            let (sql, value) = world.value(ei, f, k ^ 1);
                            (value != world.field_value(ei, &f.name, k)).then_some((f, sql, value))
                        })
                        .collect();
                    for (f, sql_value, value) in changes.iter().filter(|(f, ..)| {
                        !matches!(f.ty, FieldType::User)
                            && !transitions.iter().any(|(t, _)| t.name == f.name)
                    }) {
                        let swap = Swap {
                            field: &f.name,
                            to: SwapTo::Value(value.clone()),
                            and: None,
                        };
                        let expected = update_expected(doc, &world, p, ei, k, Some(&swap));
                        let sql = format!(
                            "update {} set {} = {sql_value} where {}",
                            table(&e.name),
                            q(&f.name),
                            key(&world, ei, k)
                        );
                        let what = format!("change the {} of {} {r}", f.name, e.name);
                        note_hazard(&mut hazards, doc, &world, p, ei, k, &swap, &what, expected);
                        check(what, sql, expected, None);
                    }
                    for (g, values) in &transitions {
                        for (sql_g, value_g, word) in values {
                            if &world.field_value(ei, &g.name, k) == value_g {
                                continue;
                            }
                            let first = Swap {
                                field: &g.name,
                                to: SwapTo::Value(value_g.clone()),
                                and: None,
                            };
                            if update_expected(doc, &world, p, ei, k, Some(&first)) != Some(true) {
                                continue;
                            }
                            for (f, sql_f, value) in
                                changes.iter().filter(|(f, ..)| f.name != g.name)
                            {
                                let both_changed = Swap {
                                    field: &f.name,
                                    to: SwapTo::Value(value.clone()),
                                    and: Some(&first),
                                };
                                let expected =
                                    update_expected(doc, &world, p, ei, k, Some(&both_changed));
                                let sql = format!(
                                    "update {} set {} = {sql_g}, {} = {sql_f} where {}",
                                    table(&e.name),
                                    q(&g.name),
                                    q(&f.name),
                                    key(&world, ei, k)
                                );
                                let what = format!(
                                    "set the {} of {} {r} to {word} and change the {}",
                                    g.name, e.name, f.name
                                );
                                check(what, sql, expected, None);
                            }
                        }
                    }
                }
            }
            // Changing who owns a row, or what it belongs to: the rule must hold on the row as
            // the update leaves it, so a person can't take a row over, give it away, or move it
            // out of their scope.
            let Some(me) = &p.id else { continue };
            for (k, r) in ROWS.iter().enumerate().take(2) {
                // A `user` primary key is the owner. The key can only be changed to one no stored
                // row holds (the other rows' keys fail on the primary key whatever the rules say),
                // so the person takes the row over when they hold none, and hands it to the
                // fourth person, who holds none, either way. A row another stored row points at
                // can't change its key (the foreign key refuses), so it gets no such check.
                let pk = primary(e);
                let constrained = e
                    .constraints
                    .iter()
                    .any(|c| c.expr.contains(pk.name.as_str()));
                if pk.ty == FieldType::User && !constrained && world.pointing_at(ei, k).is_empty() {
                    let holds_row =
                        (0..2).any(|j| world.value(ei, pk, j).1 == Val::Text(me.clone()));
                    let mut targets = Vec::new();
                    if !holds_row {
                        targets.push((me.clone(), "themselves"));
                    }
                    targets.push((world.fourth.clone(), "a person who has no row"));
                    for (id, who) in targets {
                        let swap = Swap {
                            field: &pk.name,
                            to: SwapTo::User(id.clone()),
                            and: None,
                        };
                        let expected = update_expected(doc, &world, p, ei, k, Some(&swap));
                        let sql = format!(
                            "update {} set {} = {} where {}",
                            table(&e.name),
                            q(&pk.name),
                            lit(&id),
                            key(&world, ei, k)
                        );
                        let what = format!("set the {} of {} {r} to {who}", pk.name, e.name);
                        note_hazard(&mut hazards, doc, &world, p, ei, k, &swap, &what, expected);
                        check(what, sql, expected, None);
                    }
                }
                for f in e.fields.iter().filter(|f| !f.primary) {
                    let (to, value, words) = match &f.ty {
                        FieldType::User => {
                            let current = world.value(ei, f, k).1;
                            let mine = current == Val::Text(me.clone());
                            let (id, who) = if !mine {
                                (me.clone(), "themselves")
                            } else if current == Val::Text(world.alice.clone()) {
                                (world.bob.clone(), "bob")
                            } else {
                                (world.alice.clone(), "alice")
                            };
                            (SwapTo::User(id.clone()), lit(&id), format!("to {who}"))
                        }
                        FieldType::Ref(t) => {
                            let (ti, te) = world.entity(&t.name);
                            let j = world.ref_row(e, &f.name, k) ^ 1;
                            let words = format!("at {} {}", t.name, ROWS[j]);
                            (SwapTo::Row(j), world.value(ti, primary(te), j).0, words)
                        }
                        _ => continue,
                    };
                    // A value another row already holds would break a one-field unique rule.
                    let lone_unique = f.unique
                        || e.unique
                            .iter()
                            .any(|u| u.fields.len() == 1 && u.fields[0].name == f.name);
                    // ponytail: a constraint naming the field could reject the new value (e.g.
                    // `source != destination`), so such fields are skipped; parse the constraint
                    // and evaluate it on the new row if that leaves a gap.
                    let constrained = e
                        .constraints
                        .iter()
                        .any(|c| c.expr.contains(f.name.as_str()));
                    if constrained || (lone_unique && !matches!(&to, SwapTo::User(id) if id == me))
                    {
                        continue;
                    }
                    let swap = Swap {
                        field: &f.name,
                        to,
                        and: None,
                    };
                    // Postgres checks the new row against the update rules (`check`) and, because
                    // the update reads the row (`where`), against the select rules too.
                    let expected = update_expected(doc, &world, p, ei, k, Some(&swap));
                    let verb = if matches!(f.ty, FieldType::User) {
                        "set"
                    } else {
                        "point"
                    };
                    let sql = format!(
                        "update {} set {} = {value} where {}",
                        table(&e.name),
                        q(&f.name),
                        key(&world, ei, k)
                    );
                    let what = format!("{verb} the {} of {} {r} {words}", f.name, e.name);
                    note_hazard(&mut hazards, doc, &world, p, ei, k, &swap, &what, expected);
                    check(what, sql, expected, None);
                }
            }
            // Taking a row over and moving it in one update: a `user` field and a `ref` field
            // changed together. Each on its own may be refused while the pair is not, because
            // the new row can then be seen through the owner rule while the update rule is
            // satisfied by a different policy (PostgreSQL ORs the `check`s of the update
            // policies independently), so only the pair shows a loosened `check`.
            for (k, r) in ROWS.iter().enumerate().take(2) {
                for fu in e.fields.iter().filter(|f| f.ty == FieldType::User) {
                    let Some((to_u, lit_u, _)) = owner_swap(&world, e, fu, k, me) else {
                        continue;
                    };
                    for fr in e
                        .fields
                        .iter()
                        .filter(|f| matches!(f.ty, FieldType::Ref(_)))
                    {
                        let Some((to_r, lit_r, words_r)) = owner_swap(&world, e, fr, k, me) else {
                            continue;
                        };
                        let first = Swap {
                            field: &fu.name,
                            to: to_u.clone(),
                            and: None,
                        };
                        let both_changed = Swap {
                            field: &fr.name,
                            to: to_r,
                            and: Some(&first),
                        };
                        let expected = update_expected(doc, &world, p, ei, k, Some(&both_changed));
                        let sql = format!(
                            "update {} set {} = {lit_u}, {} = {lit_r} where {}",
                            table(&e.name),
                            q(&fu.name),
                            q(&fr.name),
                            key(&world, ei, k)
                        );
                        let what = format!(
                            "change the {} and the {} of {} {r} together (point it {words_r})",
                            fu.name, fr.name, e.name
                        );
                        check(what, sql, expected, None);
                    }
                }
            }
        }
    }

    for (hi, h) in doc.holes.iter().enumerate() {
        let ei = tables + hi;
        let role = format!("svetra_hole_{}", h.name);
        let declared = |list: &[NameRef], e: &Entity| list.iter().any(|r| r.name == e.name);
        for e in &doc.entities {
            let (want, words) = if declared(&h.writes, e) {
                ("select, insert, update, delete", "can change")
            } else if declared(&h.reads, e) {
                ("select", "can only read")
            } else {
                ("none", "can't touch")
            };
            checks.push(format!(
                "select is(svetra_test.privileges({}, {}), {}, {});",
                lit(&role),
                lit(&table(&e.name)),
                lit(want),
                lit(&format!("{} {words} {}", h.name, e.name))
            ));
        }
        // Role grants: only a hole with `grants` reaches them, and only for the roles it lists.
        if !doc.roles.is_empty() {
            let (want, words) = if h.grants.is_empty() {
                ("none", "can't give anyone a role")
            } else {
                ("select, insert, delete", "can give and take away roles")
            };
            checks.push(format!(
                "select is(svetra_test.privileges({}, '\"svetra\".\"role_grants\"'), {}, {});",
                lit(&role),
                lit(want),
                lit(&format!("{} {words}", h.name))
            ));
        }
        if !h.grants.is_empty() {
            for r in &doc.roles {
                let ok = h.grants.iter().any(|g| g.name == r.name);
                checks.push(format!(
                    "select is(svetra_test.try({}, null, $svetra${}$svetra$), '{}', {});",
                    lit(&role),
                    grant_to_alice(r),
                    if ok { "allowed" } else { "denied" },
                    lit(&format!(
                        "{} {} give the {} role",
                        h.name,
                        if ok { "can" } else { "can't" },
                        r.name
                    ))
                ));
            }
        }
        // No API role can run the body (or, for a trigger, the function that fires it), and
        // there is no function the app can call.
        let locked = |names: &[String], what: &str| {
            let list: Vec<String> = names.iter().map(|n| lit(n)).collect();
            let named = match list.as_slice() {
                [one] => format!("= {one}"),
                many => format!("in ({})", many.join(", ")),
            };
            format!(
                "select is((select count(*) from pg_roles r, pg_proc p join pg_namespace n on n.oid = p.pronamespace where r.rolname in ('anon', 'authenticated', 'service_role') and n.nspname = 'svetra_holes' and p.proname {named} and has_function_privilege(r.rolname, p.oid, 'execute')), 0::bigint, {});",
                lit(what)
            )
        };
        let no_rpc = format!(
            "select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'public' and p.proname = {}), 0::bigint, {});",
            lit(&h.name),
            lit(&format!("{} has no function the app can call", h.name))
        );
        // A scheduled hole has no caller: the clock runs it, as the migration's role.
        if let HoleKind::Schedule(sched) = &h.kind {
            let job = super::sql::job_name(&h.name);
            let call = format!("\"svetra_holes\".{}(null)", q(&h.name));
            checks.push(locked(
                std::slice::from_ref(&h.name),
                &format!(
                    "no one calling the API can run {}, or pass it a caller",
                    h.name
                ),
            ));
            checks.push(no_rpc);
            checks.push(format!(
                "select is((select count(*) from cron.job where jobname = {}), 1::bigint, {});",
                lit(&job),
                lit(&format!("{} has exactly one scheduled job", h.name))
            ));
            checks.push(format!(
                "select is((select schedule from cron.job where jobname = {}), {}, {});",
                lit(&job),
                lit(&sched.cron),
                lit(&format!("{} runs on the schedule {}", h.name, sched.cron))
            ));
            checks.push(format!(
                "select ok(coalesce((select position({} in command) from cron.job where jobname = {}), 0) > 0, {});",
                lit(&call),
                lit(&job),
                lit(&format!("the job of {} runs {}", h.name, h.name))
            ));
            continue;
        }
        // A triggered hole has no caller either: a change to a row runs it, as its own role.
        if let HoleKind::Trigger(t) = &h.kind {
            let on = table(&t.on.name);
            let fire = super::sql::fire_name(&h.name);
            early.push(locked(
                &[h.name.clone(), fire.clone()],
                &format!(
                    "no one calling the API can run {}, or the function that fires it",
                    h.name
                ),
            ));
            early.push(no_rpc);
            for (func, owner, what) in [
                (&h.name, Some(&role), "its own role"),
                (&fire, None, "the owner's rights"),
            ] {
                let owned = owner.map_or(String::new(), |o| {
                    format!(" and pg_get_userbyid(p.proowner) = {}", lit(o))
                });
                early.push(format!(
                    "select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'svetra_holes' and p.proname = {}{owned} and p.prosecdef and p.proconfig = array['search_path=\"\"']), 1::bigint, {});",
                    lit(func),
                    lit(&format!(
                        "{func} runs with {what} and an empty search path"
                    ))
                ));
            }
            for event in &t.events {
                let tg = super::sql::trigger_name(&h.name, *event);
                let word = super::sql::event_word(*event);
                // AFTER, FOR EACH ROW, and the event: PostgreSQL's tgtype bits 1 + 4, 16 or 8.
                let bits = 1 + match event {
                    Action::Insert => 4,
                    Action::Update => 16,
                    _ => 8,
                };
                let which = format!(
                    "t.tgrelid = {}::regclass and t.tgname = {}",
                    lit(&on),
                    lit(&tg)
                );
                early.push(format!(
                    "select is((select count(*) from pg_trigger t where {which} and t.tgenabled = 'O' and t.tgtype = {bits} and t.tgfoid = {}::regprocedure), 1::bigint, {});",
                    lit(&format!("\"svetra_holes\".{}()", q(&fire))),
                    lit(&format!(
                        "{} runs after a row of {} is {}, once per row",
                        h.name,
                        t.on.name,
                        past(*event)
                    ))
                ));
                let cond = super::sql::trigger_when(h, t, *event, &by_name_src)
                    .map_or("null".to_string(), |c| lit(&c));
                early.push(format!(
                    "select is((select svetra_test.when_of(t.oid) from pg_trigger t where {which}), svetra_test.trigger_when({}, {}, {cond}), {});",
                    lit(&on),
                    lit(word),
                    lit(&format!(
                        "{} fires on {word} only when {}",
                        h.name,
                        h.when.as_deref().unwrap_or("always")
                    ))
                ));
            }
            early.push(format!(
                "select is((select count(*) from pg_trigger t where t.tgfoid = {}::regprocedure and not t.tgisinternal), {}::bigint, {});",
                lit(&format!("\"svetra_holes\".{}()", q(&fire))),
                t.events.len(),
                lit(&format!("nothing else fires {}", h.name))
            ));
            // The hole's role can't write beyond its fence: the first entity it doesn't change.
            if let Some(ei) = (0..tables).find(|&i| !declared(&h.writes, world.entities[i])) {
                checks.push(format!(
                    "select is(svetra_test.try({}, null, $svetra${}$svetra$), 'denied', {});",
                    lit(&role),
                    insert(&world, ei, 2),
                    lit(&format!(
                        "{} can't create a row of {}",
                        h.name, world.entities[ei].name
                    ))
                ));
            }
            continue;
        }
        // Only the declared callers reach the hole, and only through its public function: the
        // inner one takes the caller's id as an argument, so calling it directly could forge it.
        for (role, who, may) in [
            ("anon", "anon", matches!(h.role, Role::Anon | Role::Anyone)),
            (
                "authenticated",
                "signed-in people",
                matches!(h.role, Role::Authenticated | Role::Anyone),
            ),
        ] {
            checks.push(format!(
                "select is(svetra_test.executes('{role}', 'public', {}), {may}, {});",
                lit(&h.name),
                lit(&format!(
                    "{who} {} call {}",
                    if may { "can" } else { "can't" },
                    h.name
                ))
            ));
            checks.push(format!(
                "select is(svetra_test.executes('{role}', 'svetra_holes', {}), false, {});",
                lit(&h.name),
                lit(&format!(
                    "{who} can't call the inside of {} directly",
                    h.name
                ))
            ));
        }
        let when = h
            .when
            .as_deref()
            .map(|w| expr::parse(w).expect("validated"));
        for p in &people {
            let role = if p.id.is_some() {
                "authenticated"
            } else {
                "anon"
            };
            let user = p.id.as_ref().map_or("null".into(), |id| format!("'{id}'"));
            let aal = if p.aal2 { ", 'aal2'" } else { "" };
            for (k, r) in ROWS.iter().enumerate().take(2) {
                let applies = match h.role {
                    Role::Anyone => true,
                    Role::Anon => p.id.is_none(),
                    Role::Authenticated => p.id.is_some(),
                };
                let ok = if !applies {
                    Some(false)
                } else if let Some(w) = &when {
                    let unsure = Cell::new(false);
                    let env = Env {
                        world: &world,
                        ei,
                        k,
                        person: p,
                        swap: None,
                        unsure: &unsure,
                    };
                    let v = eval(w, &env);
                    if unsure.get() {
                        None
                    } else {
                        Some(v == Val::Bool(true))
                    }
                } else {
                    Some(true)
                };
                let args: Vec<String> = world.entities[ei]
                    .fields
                    .iter()
                    .map(|f| world.value(ei, f, k).0)
                    .collect();
                let sql = format!("select \"public\".{}({})", q(&h.name), args.join(", "));
                let what = format!("call {} on the {r} rows", h.name);
                found.push(Check {
                    person: p.name.clone(),
                    entity: h.name.clone(),
                    action: "call",
                    row: *r,
                    expected: ok,
                    number: checks.len(),
                    because: vec![],
                    column_rules: vec![],
                });
                checks.push(match ok {
                    Some(ok) => format!(
                        "select is(svetra_test.call('{role}', {user}, $svetra${sql}$svetra${aal}), '{}', {});",
                        if ok { "let through" } else { "refused" },
                        lit(&format!("{} {} {what}", p.name, if ok { "can" } else { "can't" }))
                    ),
                    None => format!(
                        "select skip({}, 1);",
                        lit(&format!("{} {what}: a rule compares a time with text that isn't an ISO time", p.name))
                    ),
                });
            }
        }
        require_checks(&world, &people, h, ei, &mut checks);
    }

    // Once for the file: no job Svetra made is left over from a removed hole, and the clock is UTC.
    let jobs: Vec<String> = doc
        .holes
        .iter()
        .filter(|h| matches!(h.kind, HoleKind::Schedule(_)))
        .map(|h| lit(&super::sql::job_name(&h.name)))
        .collect();
    if !jobs.is_empty() {
        checks.push(format!(
            "select is((select count(*) from cron.job where jobname like 'svetra\\_%' and jobname not in ({})), 0::bigint, 'no scheduled job is left over from code that was removed');",
            jobs.join(", ")
        ));
        checks.push(
            "select ok(coalesce(current_setting('cron.timezone', true), 'UTC') in ('UTC', 'GMT'), 'scheduled times are in UTC');"
                .into(),
        );
    }

    // Triggered holes. Their bodies are hand-written, and a stub raises, so the triggers are
    // switched off while the test data and every other check run, and on again for the one check
    // that a change to a row fires the hole.
    let hole_triggers: Vec<(&Hole, &Trigger)> = doc
        .holes
        .iter()
        .filter_map(|h| match &h.kind {
            HoleKind::Trigger(t) => Some((h, t)),
            _ => None,
        })
        .collect();
    let switch = |state: &str| -> Vec<String> {
        hole_triggers
            .iter()
            .flat_map(|(h, t)| {
                t.events.iter().map(move |ev| {
                    let name = super::sql::trigger_name(&h.name, *ev);
                    format!(
                        "select lives_ok($svetra$alter table {} {state} trigger {}$svetra$, {});",
                        table(&t.on.name),
                        q(&name),
                        lit(&format!(
                            "{name} is switched {}",
                            if state == "enable" {
                                "on again"
                            } else {
                                "off while the test data goes in"
                            }
                        ))
                    )
                })
            })
            .collect()
    };
    if !hole_triggers.is_empty() {
        setup.splice(0..0, switch("disable"));
        checks.extend(switch("enable"));
        for (h, t) in &hole_triggers {
            if !t.events.contains(&Action::Insert) {
                continue;
            }
            let ei = world.by_name[t.on.name.as_str()];
            let when = h
                .when
                .as_deref()
                .map(|w| expr::parse(w).expect("validated"));
            // The first person and row the rules let insert, where the hole's condition holds.
            let pick = people.iter().find_map(|p| {
                (2..4).find_map(|k| {
                    if allowed(doc, &world, p, ei, k, Action::Insert, Phase::Old, None)
                        != Some(true)
                    {
                        return None;
                    }
                    let unsure = Cell::new(false);
                    let env = Env {
                        world: &world,
                        ei,
                        k,
                        person: p,
                        swap: None,
                        unsure: &unsure,
                    };
                    let fires = match &when {
                        None => true,
                        Some(w) => eval(w, &env) == Val::Bool(true) && !unsure.get(),
                    };
                    fires.then_some((p, k))
                })
            });
            let Some((p, k)) = pick else { continue };
            let row = table(&t.on.name);
            let probe = format!(
                "create or replace function \"svetra_holes\".{}(p_caller uuid, p_old {row}, p_new {row}) returns void language plpgsql security definer set search_path = '' as $probe$ begin raise exception 'svetra_fired' using errcode = 'SV001'; end $probe$",
                q(&h.name)
            );
            let role = if p.id.is_some() {
                "authenticated"
            } else {
                "anon"
            };
            let user = p.id.as_ref().map_or("null".into(), |id| format!("'{id}'"));
            let aal = if p.aal2 { "aal2" } else { "aal1" };
            // The probe replaces the body for this one statement and is undone with it.
            checks.push(format!(
                "select is(svetra_test.try('{role}', {user}, $svetra${}$svetra$, '{aal}', $svetra${probe}$svetra$), 'error SV001: svetra_fired', {});",
                insert(&world, ei, k),
                lit(&format!(
                    "{} creating {} {} fires {}",
                    p.name,
                    t.on.name,
                    ROWS[k],
                    h.name
                ))
            ));
        }
    }

    let names: Vec<&str> = people.iter().map(|p| p.name.as_str()).collect();
    let trigger_helper = if hole_triggers.is_empty() {
        ""
    } else {
        TRIGGER_HELPER
    };
    let early_section = if early.is_empty() {
        String::new()
    } else {
        format!(
            "-- Triggered holes: what is declared, checked before the test data goes in\n{}\n\n",
            early.join("\n")
        )
    };
    let input_helper = if doc.holes.iter().any(|h| !h.require.is_empty()) {
        INPUT_HELPER
    } else {
        ""
    };
    let text = format!(
        "-- Access tests: run with `supabase test db`. Every check runs as a real user, never the\n\
         -- service key, and the whole file rolls back. People: {}.\n\
         begin;\n\
         create extension if not exists pgtap;\n\
         {HELPER}{trigger_helper}{input_helper}\n\
         select plan({});\n\n\
         {early_section}\
         -- Test data, inserted as the database owner\n{}\n\n\
         -- Checks\n{}\n\n\
         select * from finish();\n\
         rollback;\n",
        names.join(", "),
        early.len() + setup.len() + checks.len(),
        setup.join("\n"),
        checks.join("\n")
    );
    // Checks are numbered after the early checks and the test data, from 1.
    for c in &mut found {
        c.number += early.len() + setup.len() + 1;
    }
    let people = people
        .iter()
        .map(|p| TestPerson {
            name: p.name.clone(),
            signed_in: p.id.is_some(),
            role: p.grants.first().map(|(r, scope)| {
                let entity = scope.as_ref().and_then(|_| {
                    doc.roles
                        .iter()
                        .find(|d| &d.name == r)
                        .and_then(|d| d.scope.as_ref().map(|s| s.name.clone()))
                });
                (r.clone(), entity)
            }),
            second_factor: p.aal2,
        })
        .collect();
    Analysis {
        text,
        people,
        checks: found,
        hazards,
        overlaps,
    }
}

/// An event as a past participle, for check names.
fn past(event: Action) -> &'static str {
    match event {
        Action::Insert => "inserted",
        Action::Update => "updated",
        _ => "deleted",
    }
}

/// A value to try for one input of a hole: what the rules see, the SQL to send, and words.
struct Try {
    val: Val,
    sql: String,
    words: String,
}

fn sql_of(v: &Val) -> String {
    match v {
        Val::Null => "null".into(),
        Val::Bool(b) => b.to_string(),
        Val::Num(n) => format!("{n}"),
        Val::Text(s) | Val::Date(s) => lit(s),
        Val::Time(t) => lit(&format!("{t}Z")),
    }
}

fn words_of(v: &Val) -> String {
    match v {
        Val::Null => "nothing".into(),
        Val::Bool(b) => b.to_string(),
        Val::Num(n) => format!("{n}"),
        Val::Text(s) | Val::Date(s) | Val::Time(s) => format!("'{s}'"),
    }
}

/// The literals a condition mentions, in order of appearance.
fn literals(e: &Expr, out: &mut Vec<Val>) {
    match e {
        Expr::Num(n) => out.push(Val::Num(*n)),
        Expr::Str(s) => out.push(Val::Text(s.clone())),
        Expr::Bool(b) => out.push(Val::Bool(*b)),
        Expr::Neg(x) | Expr::Not(x) => literals(x, out),
        Expr::Bin(_, a, b) => {
            literals(a, out);
            literals(b, out);
        }
        Expr::In(x, items) => {
            literals(x, out);
            items.iter().for_each(|i| literals(i, out));
        }
        Expr::Call(_, args) => args.iter().for_each(|a| literals(a, out)),
        Expr::Null | Expr::Path(_) => {}
    }
}

/// Whether the expression reads `field` as itself (`amount`), or through it (`source.owner`).
fn reads_input(e: &Expr, field: &str) -> (bool, bool) {
    match e {
        Expr::Path(p) => (p.len() == 1 && p[0] == field, p.len() == 2 && p[0] == field),
        Expr::Neg(x) | Expr::Not(x) => reads_input(x, field),
        Expr::Bin(_, a, b) => {
            let (x, y) = (reads_input(a, field), reads_input(b, field));
            (x.0 || y.0, x.1 || y.1)
        }
        Expr::In(x, items) => items.iter().fold(reads_input(x, field), |acc, i| {
            let r = reads_input(i, field);
            (acc.0 || r.0, acc.1 || r.1)
        }),
        Expr::Call(_, args) => args.iter().fold((false, false), |acc, a| {
            let r = reads_input(a, field);
            (acc.0 || r.0, acc.1 || r.1)
        }),
        _ => (false, false),
    }
}

/// The same type, comparing a reference by the entity it points at and not by where it was written.
fn same_type(a: &FieldType, b: &FieldType) -> bool {
    match (a, b) {
        (FieldType::Ref(x), FieldType::Ref(y)) => x.name == y.name,
        _ => a == b,
    }
}

/// Values to try for `field`, most telling first: another input's value (for `a != b`), each
/// literal of the condition and its neighbours, then nothing.
fn tries(world: &World, ei: usize, field: &Field, cond: &Expr) -> Vec<Try> {
    let mut out = Vec::new();
    let mut seen: Vec<Val> = Vec::new();
    let mut add = |out: &mut Vec<Try>, val: Val, sql: String, words: String| {
        if !seen.contains(&val) {
            seen.push(val.clone());
            out.push(Try { val, sql, words });
        }
    };
    for g in &world.entities[ei].fields {
        if g.name != field.name && same_type(&g.ty, &field.ty) && reads_input(cond, &g.name).0 {
            let (sql, val) = world.value(ei, g, 0);
            add(&mut out, val, sql, format!("the same as {}", g.name));
        }
    }
    let mut lits = Vec::new();
    literals(cond, &mut lits);
    let numeric = matches!(
        field.ty,
        FieldType::Int | FieldType::Float | FieldType::Decimal { .. } | FieldType::Money { .. }
    );
    let texty = matches!(field.ty, FieldType::Text | FieldType::Enum(_));
    for l in lits {
        let vals = match (&l, numeric, texty, &field.ty) {
            (Val::Num(n), true, _, _) => vec![Val::Num(*n), Val::Num(n + 1.0), Val::Num(n - 1.0)],
            (Val::Text(s), _, true, _) => {
                vec![Val::Text(s.clone()), Val::Text(format!("{s} svetra"))]
            }
            (Val::Bool(b), _, _, FieldType::Bool) => vec![Val::Bool(*b), Val::Bool(!*b)],
            _ => vec![],
        };
        for v in vals {
            add(&mut out, v.clone(), sql_of(&v), words_of(&v));
        }
    }
    add(&mut out, Val::Null, "null".into(), "nothing".into());
    out
}

/// Whether `p` may call `h` as far as `as` goes.
fn may_call(h: &Hole, p: &Person) -> bool {
    match h.role {
        Role::Anyone => true,
        Role::Anon => p.id.is_none(),
        Role::Authenticated => p.id.is_some(),
    }
}

/// For each `require` condition of `h`: one call, by someone `when` lets through, that breaks that
/// condition and keeps the ones before it, which must be refused as bad input with the condition's
/// own message. The values come from the condition's literals, so the proof is about the rules
/// as written. A condition no such call can be found for is skipped, and says so.
fn require_checks(world: &World, people: &[Person], h: &Hole, ei: usize, checks: &mut Vec<String>) {
    let when = h
        .when
        .as_deref()
        .map(|w| expr::parse(w).expect("validated"));
    let conds: Vec<Expr> = h
        .require
        .iter()
        .map(|r| expr::parse(r).expect("validated"))
        .collect();
    for (i, cond) in conds.iter().enumerate() {
        let mut found: Option<String> = None;
        'search: for field in world.entities[ei]
            .fields
            .iter()
            .filter(|f| reads_input(cond, &f.name).0)
        {
            // The rules can't follow a changed input through `source.owner`, so such inputs aren't tried.
            let hopped = |e: &Expr| reads_input(e, &field.name).1;
            if when.as_ref().is_some_and(hopped) || conds[..=i].iter().any(hopped) {
                continue;
            }
            for t in tries(world, ei, field, cond) {
                let swap = Swap {
                    field: &field.name,
                    to: SwapTo::Value(t.val.clone()),
                    and: None,
                };
                for p in people.iter().filter(|p| may_call(h, p)) {
                    let unsure = Cell::new(false);
                    let env = Env {
                        world,
                        ei,
                        k: 0,
                        person: p,
                        swap: Some(&swap),
                        unsure: &unsure,
                    };
                    let through = when
                        .as_ref()
                        .is_none_or(|w| eval(w, &env) == Val::Bool(true));
                    let earlier = conds[..i].iter().all(|c| eval(c, &env) == Val::Bool(true));
                    let breaks = eval(cond, &env) != Val::Bool(true);
                    if !(through && earlier && breaks) || unsure.get() {
                        continue;
                    }
                    let args: Vec<String> = world.entities[ei]
                        .fields
                        .iter()
                        .map(|f| {
                            if f.name == field.name {
                                t.sql.clone()
                            } else {
                                world.value(ei, f, 0).0
                            }
                        })
                        .collect();
                    let role = if p.id.is_some() {
                        "authenticated"
                    } else {
                        "anon"
                    };
                    let user = p.id.as_ref().map_or("null".into(), |id| format!("'{id}'"));
                    let aal = if p.aal2 { ", 'aal2'" } else { "" };
                    let sql = format!("select \"public\".{}({})", q(&h.name), args.join(", "));
                    found = Some(format!(
                        "select is(svetra_test.call_input('{role}', {user}, $svetra${sql}$svetra${aal}), {}, {});",
                        lit(&format!("{}: {}", h.name, h.require[i])),
                        lit(&format!(
                            "{} calling {} with {} {} is refused as bad input: {}",
                            p.name, h.name, field.name, t.words, h.require[i]
                        ))
                    ));
                    break 'search;
                }
            }
        }
        checks.push(found.unwrap_or_else(|| {
            format!(
                "select skip({}, 1);",
                lit(&format!(
                    "{}: no call found that gets past who may call it and breaks `{}`, so it isn't tried",
                    h.name, h.require[i]
                ))
            )
        }));
    }
}

fn lives(sql: &str, what: &str) -> String {
    format!(
        "select lives_ok($svetra${sql}$svetra$, {});",
        lit(&format!("test data: {what}"))
    )
}

fn key(world: &World, ei: usize, k: usize) -> String {
    let pk = primary(world.entities[ei]);
    format!("{} = {}", q(&pk.name), world.value(ei, pk, k).0)
}

fn select(world: &World, ei: usize, k: usize) -> String {
    format!(
        "select 1 from {} where {}",
        table(&world.entities[ei].name),
        key(world, ei, k)
    )
}

fn update(world: &World, ei: usize, k: usize) -> String {
    let pk = q(&primary(world.entities[ei]).name);
    format!(
        "update {} set {pk} = {pk} where {}",
        table(&world.entities[ei].name),
        key(world, ei, k)
    )
}

fn delete(world: &World, ei: usize, k: usize) -> String {
    format!(
        "delete from {} where {}",
        table(&world.entities[ei].name),
        key(world, ei, k)
    )
}

fn insert(world: &World, ei: usize, k: usize) -> String {
    let e = world.entities[ei];
    let cols: Vec<String> = e.fields.iter().map(|f| q(&f.name)).collect();
    let vals: Vec<String> = e.fields.iter().map(|f| world.value(ei, f, k).0).collect();
    format!(
        "insert into {} ({}) values ({})",
        table(&e.name),
        cols.join(", "),
        vals.join(", ")
    )
}
