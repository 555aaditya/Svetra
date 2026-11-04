//! Access tests (docs/compiler.md, Access tests): pgTAP checks that the database enforces
//! the rules in `svetra.yaml`, run as real users. Expected results come from evaluating
//! the rules here, never from the generated SQL, so the two can disagree when one is wrong.

use super::names::{lit, q, table};
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
}

struct World<'a> {
    entities: Vec<&'a Entity>,
    by_name: HashMap<&'a str, usize>,
    alice: String,
    bob: String,
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
            FieldType::User => text(
                if k.is_multiple_of(2) {
                    &self.alice
                } else {
                    &self.bob
                }
                .clone(),
            ),
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

/// Where a rule is evaluated: one row of one entity, for one person.
struct Env<'w, 'a> {
    world: &'w World<'a>,
    ei: usize,
    k: usize,
    person: &'w Person,
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
            [f] => env.world.field_value(env.ei, f, env.k),
            [r, f] => {
                let e = env.world.entities[env.ei];
                let row = env.world.ref_row(e, r, env.k);
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
                    .any(|(g, s)| g == role && *s == scope),
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

/// Does some rule let `person` do `action` on row `k` of entity `ei`? `None` when a rule that
/// could decide it can't be evaluated, so the check is skipped rather than guessed.
fn allowed(
    doc: &Document,
    world: &World,
    person: &Person,
    ei: usize,
    k: usize,
    a: Action,
) -> Option<bool> {
    let name = &world.entities[ei].name;
    let mut unknown = false;
    for p in &doc.policies {
        let applies = match p.role {
            Role::Anyone => true,
            Role::Anon => person.id.is_none(),
            Role::Authenticated => person.id.is_some(),
        };
        if !(applies && &p.on.name == name && p.allow.contains(&a)) {
            continue;
        }
        let unsure = Cell::new(false);
        let env = Env {
            world,
            ei,
            k,
            person,
            unsure: &unsure,
        };
        let v = eval(&expr::parse(&p.when).expect("validated"), &env);
        if unsure.get() {
            unknown = true;
        } else if v == Val::Bool(true) {
            return Some(true);
        }
    }
    if unknown { None } else { Some(false) }
}

/// Both must hold: `None` if either is unknown and the other doesn't already rule it out.
fn both(x: Option<bool>, y: Option<bool>) -> Option<bool> {
    match (x, y) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

/// Entities in an order where every ref target comes first; file order breaks ties and cycles.
fn insert_order(world: &World) -> Vec<usize> {
    let mut done: Vec<usize> = Vec::new();
    while done.len() < world.entities.len() {
        let ready = (0..world.entities.len()).find(|&i| {
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
            (0..world.entities.len())
                .find(|i| !done.contains(i))
                .expect("some entity is left")
        });
        done.push(next);
    }
    done
}

const HELPER: &str = r#"-- Run one statement as one person, then undo it. Returns allowed, denied or the error.
create schema svetra_test;
create function svetra_test.try(p_role text, p_user uuid, p_sql text) returns text
language plpgsql as $$
declare n bigint;
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role)::text, true);
    execute p_sql;
    get diagnostics n = row_count;
    raise exception 'svetra_undo';
  exception when others then
    if sqlerrm = 'svetra_undo' then
      return case when n > 0 then 'allowed' else 'denied' end;
    elsif sqlstate = '42501' then
      return 'denied';
    end if;
    return 'error ' || sqlstate || ': ' || sqlerrm;
  end;
end $$;
"#;

/// The whole test file for a validated document.
pub fn tests(doc: &Document) -> String {
    let mut people = vec![
        Person {
            name: "anon".into(),
            id: None,
            grants: vec![],
        },
        Person {
            name: "alice".into(),
            id: Some(person_id(1)),
            grants: vec![],
        },
        Person {
            name: "bob".into(),
            id: Some(person_id(2)),
            grants: vec![],
        },
    ];
    let world = World {
        entities: doc.entities.iter().collect(),
        by_name: doc
            .entities
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.as_str(), i))
            .collect(),
        alice: person_id(1),
        bob: person_id(2),
    };
    let globals = doc.roles.iter().filter(|r| r.scope.is_none());
    let scoped = doc.roles.iter().filter(|r| r.scope.is_some());
    for r in globals.chain(scoped) {
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
        people.push(Person {
            name,
            id: Some(id),
            grants: vec![(r.name.clone(), scope)],
        });
    }

    let mut setup = Vec::new();
    let users: Vec<String> = people
        .iter()
        .filter_map(|p| {
            p.id.as_ref()
                .map(|id| format!("('{id}', '{}@svetra.test')", p.name.replace(' ', ".")))
        })
        .collect();
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
    for ei in insert_order(&world) {
        for (k, r) in ROWS.iter().enumerate().take(2) {
            let what = format!("{} {r}", world.entities[ei].name);
            setup.push(lives(&insert(&world, ei, k), &what));
        }
    }

    let mut checks = Vec::new();
    for (ei, e) in world.entities.iter().enumerate() {
        for p in &people {
            let mut check = |verb: &str, k: usize, sql: String, ok: Option<bool>| {
                let role = if p.id.is_some() {
                    "authenticated"
                } else {
                    "anon"
                };
                let user = p.id.as_ref().map_or("null".into(), |id| format!("'{id}'"));
                let what = format!("{verb} {} {}", e.name, ROWS[k]);
                checks.push(match ok {
                    Some(ok) => format!(
                        "select is(svetra_test.try('{role}', {user}, $svetra${sql}$svetra$), '{}', {});",
                        if ok { "allowed" } else { "denied" },
                        lit(&format!("{} {} {what}", p.name, if ok { "can" } else { "can't" }))
                    ),
                    None => format!(
                        "select skip({}, 1);",
                        lit(&format!("{} {what}: a rule compares a time with text that isn't an ISO time", p.name))
                    ),
                });
            };
            if e.private {
                if p.name == "anon" || p.name == "alice" {
                    check("see", 0, select(&world, ei, 0), Some(false));
                    check("create", 2, insert(&world, ei, 2), Some(false));
                }
                continue;
            }
            let can = |k, a| allowed(doc, &world, p, ei, k, a);
            for k in 0..2 {
                check("see", k, select(&world, ei, k), can(k, Action::Select));
            }
            for k in 0..2 {
                let ok = both(can(k, Action::Select), can(k, Action::Update));
                check("change", k, update(&world, ei, k), ok);
            }
            for k in 0..2 {
                let ok = both(can(k, Action::Select), can(k, Action::Delete));
                check("delete", k, delete(&world, ei, k), ok);
            }
            for k in 2..4 {
                check("create", k, insert(&world, ei, k), can(k, Action::Insert));
            }
        }
    }

    let names: Vec<&str> = people.iter().map(|p| p.name.as_str()).collect();
    format!(
        "-- Access tests: run with `supabase test db`. Every check runs as a real user, never the\n\
         -- service key, and the whole file rolls back. People: {}.\n\
         begin;\n\
         create extension if not exists pgtap;\n\
         {HELPER}\n\
         select plan({});\n\n\
         -- Test data, inserted as the database owner\n{}\n\n\
         -- Checks\n{}\n\n\
         select * from finish();\n\
         rollback;\n",
        names.join(", "),
        setup.len() + checks.len(),
        setup.join("\n"),
        checks.join("\n")
    )
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
