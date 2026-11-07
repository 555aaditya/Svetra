//! Rules that need the whole document: names, duplicates, primary keys, references,
//! access, money, indexes and expressions.

use crate::diag::{Code, Diagnostic};
use crate::expr;
use crate::model::*;
use crate::typeck;
use std::collections::{HashMap, HashSet};

/// Words that mark a field as money for SVT005.
const MONEY_WORDS: &[&str] = &["amount", "price", "balance", "cost", "total", "fee"];

/// Sign-in methods for `auth.sign_in`: email, and the OAuth providers that need only a
/// client id and secret (docs/format.md, Authentication).
pub const SIGN_IN: &[&str] = &[
    "email",
    "apple",
    "bitbucket",
    "discord",
    "facebook",
    "github",
    "gitlab",
    "google",
    "linkedin_oidc",
    "notion",
    "slack_oidc",
    "spotify",
    "twitch",
    "twitter",
    "x",
    "zoom",
];

/// Supabase's `password_requirements` values, plus `none`.
const REQUIRE: &[&str] = &[
    "none",
    "letters_digits",
    "lower_upper_letters_digits",
    "lower_upper_letters_digits_symbols",
];

pub fn check(doc: &Document) -> Vec<Diagnostic> {
    let mut ck = Checker {
        entities: doc.entities.iter().map(|e| (e.name.as_str(), e)).collect(),
        out: Vec::new(),
    };
    ck.names(doc);
    ck.duplicates(doc);
    for e in &doc.entities {
        ck.entity(e);
    }
    ck.policies(doc);
    ck.holes(doc);
    ck.roles(doc);
    ck.access(doc);
    ck.anon_only(doc);
    ck.expressions(doc);
    ck.auth(doc);
    ck.out
}

struct Checker<'a> {
    entities: HashMap<&'a str, &'a Entity>,
    out: Vec<Diagnostic>,
}

fn pascal(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn snake(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl<'a> Checker<'a> {
    fn d(&mut self, code: Code, path: String, at: Pos, message: String, hint: &str) {
        self.out.push(Diagnostic {
            code,
            path,
            line: at.line,
            col: at.col,
            message,
            hint: hint.into(),
        });
    }

    fn name(&mut self, kind: &str, name: &str, path: String, at: Pos, pascal_case: bool) {
        let (ok, style) = if pascal_case {
            (pascal(name), "PascalCase")
        } else {
            (snake(name), "snake_case")
        };
        if !ok {
            self.d(
                Code::Svt021,
                path,
                at,
                format!("{kind} name `{name}` must be {style}."),
                &format!("Rename it in {style}."),
            );
        }
    }

    fn names(&mut self, doc: &Document) {
        if !doc.system.name.is_empty() {
            self.name(
                "System",
                &doc.system.name,
                "system".into(),
                doc.system.pos,
                true,
            );
        }
        for e in &doc.entities {
            let ep = format!("entities.{}", e.name);
            self.name("Entity", &e.name, ep.clone(), e.pos, true);
            for f in &e.fields {
                self.name(
                    "Field",
                    &f.name,
                    format!("{ep}.fields.{}", f.name),
                    f.pos,
                    false,
                );
            }
            for c in &e.constraints {
                self.name(
                    "Constraint",
                    &c.name,
                    format!("{ep}.constraints.{}", c.name),
                    c.pos,
                    false,
                );
            }
        }
        for p in &doc.policies {
            self.name(
                "Policy",
                &p.name,
                format!("policies.{}", p.name),
                p.pos,
                false,
            );
        }
        for h in &doc.holes {
            self.name("Hole", &h.name, format!("holes.{}", h.name), h.pos, false);
        }
        for r in &doc.roles {
            self.name("Role", &r.name, format!("roles.{}", r.name), r.pos, false);
        }
    }

    /// Constraint, policy and hole names become database object names, so they must be unique file-wide.
    fn duplicates(&mut self, doc: &Document) {
        let mut seen = HashSet::new();
        let constraints = doc.entities.iter().flat_map(|e| {
            e.constraints.iter().map(move |c| {
                (
                    c.name.as_str(),
                    format!("entities.{}.constraints.{}", e.name, c.name),
                    c.pos,
                )
            })
        });
        let policies = doc
            .policies
            .iter()
            .map(|p| (p.name.as_str(), format!("policies.{}", p.name), p.pos));
        let holes = doc
            .holes
            .iter()
            .map(|h| (h.name.as_str(), format!("holes.{}", h.name), h.pos));
        for (name, path, at) in constraints.chain(policies).chain(holes) {
            if !seen.insert(name) {
                let hint = "Constraint, policy and hole names must be unique across the file.";
                self.d(
                    Code::Svt024,
                    path,
                    at,
                    format!("The name `{name}` is used more than once."),
                    hint,
                );
            }
        }
    }

    fn entity(&mut self, e: &Entity) {
        let ep = format!("entities.{}", e.name);
        let primaries = e.fields.iter().filter(|f| f.primary).count();
        if primaries != 1 {
            let msg = format!(
                "{} has {primaries} primary fields; it needs exactly one.",
                e.name
            );
            self.d(
                Code::Svt022,
                ep.clone(),
                e.pos,
                msg,
                "Mark one field primary: true, usually id: { type: uuid, primary: true }.",
            );
        }
        let names: HashSet<&str> = e.fields.iter().map(|f| f.name.as_str()).collect();
        for (key, lists) in [("unique", &e.unique), ("indexes", &e.indexes)] {
            for f in lists.iter().flat_map(|l| &l.fields) {
                if !names.contains(f.name.as_str()) {
                    let msg = format!("`{key}` names `{}`, which {} doesn't have.", f.name, e.name);
                    self.d(
                        Code::Svt026,
                        format!("{ep}.{key}"),
                        f.pos,
                        msg,
                        "Use a field of this entity.",
                    );
                }
            }
        }
        for f in &e.fields {
            let fp = format!("{ep}.fields.{}", f.name);
            if let FieldType::Ref(target) = &f.ty {
                self.reference(target, &fp);
            }
            // `user` fields reference auth.users and are what policies filter on, so they need indexes too.
            if matches!(f.ty, FieldType::Ref(_) | FieldType::User) {
                let leads = |l: &FieldList| l.fields.first().is_some_and(|x| x.name == f.name);
                if !f.unique && !e.unique.iter().chain(&e.indexes).any(leads) {
                    let msg = format!(
                        "`{}` is a reference with no index leading on it; the compiler will add one.",
                        f.name
                    );
                    self.d(
                        Code::Svt010,
                        fp.clone(),
                        f.pos,
                        msg,
                        "Nothing to do, or declare the index yourself.",
                    );
                }
            }
            let words: Vec<&str> = f.name.split('_').collect();
            if f.ty == FieldType::Float && words.iter().any(|w| MONEY_WORDS.contains(w)) {
                let msg = format!(
                    "`{}` looks like money but uses float, which loses cents.",
                    f.name
                );
                self.d(
                    Code::Svt005,
                    fp,
                    f.pos,
                    msg,
                    "Use { type: money, precision: 18, scale: 6 }.",
                );
            }
        }
    }

    /// SVT002 for a reference to an entity. Returns the entity if it exists.
    fn reference(&mut self, r: &NameRef, path: &str) -> Option<&'a Entity> {
        let found = self.entities.get(r.name.as_str()).copied();
        if found.is_none() {
            let msg = format!("No entity named `{}`.", r.name);
            self.d(
                Code::Svt002,
                path.into(),
                r.pos,
                msg,
                "Use an entity declared under `entities`.",
            );
        }
        found
    }

    /// A scoped role needs an existing scope entity (SVT002) with a uuid primary key (SVT006).
    /// SVT008 for invalid sign-in settings, and SVT009 when there are none at all.
    fn auth(&mut self, doc: &Document) {
        let Some(a) = &doc.auth else {
            self.d(
                Code::Svt009,
                "auth".into(),
                doc.system.pos,
                "No `auth` section, so Supabase's defaults apply: passwords of 6 characters, and no email confirmation.".into(),
                "Add `auth: { sign_in: [email] }` for safe defaults: 10-character passwords and confirmed emails.",
            );
            return;
        };
        let bad = |what: &str, got: &str, allowed: &[&str]| {
            (
                format!("`{got}` isn't a {what}."),
                format!("Use one of: {}.", allowed.join(", ")),
            )
        };
        if a.sign_in.is_empty() {
            self.d(
                Code::Svt008,
                "auth.sign_in".into(),
                a.pos,
                "`auth.sign_in` lists no sign-in method.".into(),
                "Add at least one, such as `email`.",
            );
        }
        let mut seen = HashSet::new();
        for m in &a.sign_in {
            if !SIGN_IN.contains(&m.name.as_str()) {
                let (msg, hint) = bad("sign-in method", &m.name, SIGN_IN);
                self.d(Code::Svt008, "auth.sign_in".into(), m.pos, msg, &hint);
            } else if !seen.insert(m.name.as_str()) {
                let msg = format!("`{}` is listed twice in `auth.sign_in`.", m.name);
                self.d(
                    Code::Svt008,
                    "auth.sign_in".into(),
                    m.pos,
                    msg,
                    "Remove the second one.",
                );
            }
        }
        if let Some(s) = &a.signup
            && !["open", "invite_only"].contains(&s.name.as_str())
        {
            let (msg, hint) = bad("signup setting", &s.name, &["open", "invite_only"]);
            self.d(Code::Svt008, "auth.signup".into(), s.pos, msg, &hint);
        }
        if let Some(p) = &a.password {
            if let Some((n, at)) = p.min_length
                && n < 8
            {
                let msg = format!("Passwords of {n} characters are too easy to guess.");
                self.d(
                    Code::Svt008,
                    "auth.password.min_length".into(),
                    at,
                    msg,
                    "Use at least 8; 10 or more is better.",
                );
            }
            if let Some(r) = &p.require
                && !REQUIRE.contains(&r.name.as_str())
            {
                let (msg, hint) = bad("password rule", &r.name, REQUIRE);
                self.d(
                    Code::Svt008,
                    "auth.password.require".into(),
                    r.pos,
                    msg,
                    &hint,
                );
            }
        }
        if !a.email() {
            let set = a
                .password
                .as_ref()
                .map(|p| ("password", p.pos))
                .or(a.confirm_email.map(|(_, at)| ("confirm_email", at)));
            if let Some((key, at)) = set {
                let msg = format!(
                    "`auth.{key}` only applies to email sign-in, which `sign_in` doesn't list."
                );
                self.d(
                    Code::Svt008,
                    format!("auth.{key}"),
                    at,
                    msg,
                    "Add `email` to `sign_in`, or remove this setting.",
                );
            }
        }
    }

    fn roles(&mut self, doc: &Document) {
        for r in &doc.roles {
            let Some(scope) = &r.scope else { continue };
            if let Some(e) = self.reference(scope, &format!("roles.{}.scope", r.name))
                && !e
                    .fields
                    .iter()
                    .any(|f| f.primary && f.ty == FieldType::Uuid)
            {
                let msg = format!(
                    "Role `{}` is scoped to {}, whose primary key isn't a uuid.",
                    r.name, e.name
                );
                self.d(
                    Code::Svt006,
                    format!("roles.{}.scope", r.name),
                    scope.pos,
                    msg,
                    "Scope roles to an entity with id: { type: uuid, primary: true }.",
                );
            }
        }
    }

    fn policies(&mut self, doc: &Document) {
        for p in &doc.policies {
            let pp = format!("policies.{}.on", p.name);
            if let Some(e) = self.reference(&p.on, &pp)
                && e.private
            {
                let msg = format!(
                    "{} is private, so policy `{}` has no effect.",
                    e.name, p.name
                );
                self.d(
                    Code::Svt027,
                    pp,
                    p.pos,
                    msg,
                    "Remove the policy, or make the entity public.",
                );
            }
        }
    }

    fn holes(&mut self, doc: &Document) {
        for h in &doc.holes {
            let hp = format!("holes.{}", h.name);
            for r in h.reads.iter().chain(&h.writes) {
                self.reference(r, &hp);
            }
            let params = h
                .input
                .iter()
                .map(|p| (&p.ty, format!("{hp}.input.{}", p.name)));
            let output = h.output.iter().map(|t| (t, format!("{hp}.output")));
            for (ty, path) in params.chain(output) {
                match ty {
                    ParamType::Entity(r) => {
                        self.reference(r, &path);
                    }
                    ParamType::FieldOf { entity, field } => {
                        if let Some(e) = self.reference(entity, &path)
                            && !e.fields.iter().any(|f| &f.name == field)
                        {
                            let msg = format!("{} has no field `{field}`.", e.name);
                            self.d(
                                Code::Svt002,
                                path,
                                entity.pos,
                                msg,
                                "Use a field of that entity.",
                            );
                        }
                    }
                    ParamType::Type(FieldType::Ref(r)) => {
                        self.reference(r, &path);
                    }
                    ParamType::Type(_) => {}
                }
            }
        }
    }

    /// SVT003: constraints and policy conditions must type-check as booleans in their scope.
    fn expressions(&mut self, doc: &Document) {
        let entities = self.entities.clone();
        for e in &doc.entities {
            let scope = typeck::Scope::new(e, &entities, false, &doc.roles);
            for c in &e.constraints {
                if let Err(problem) = typeck::check(&c.expr, &scope) {
                    let (code, msg) = problem.split();
                    self.d(
                        code,
                        format!("entities.{}.constraints.{}", e.name, c.name),
                        c.pos,
                        msg,
                        "Fix the expression; see Expressions in docs/format.md.",
                    );
                }
            }
        }
        for p in &doc.policies {
            let Some(e) = entities.get(p.on.name.as_str()) else {
                continue;
            };
            let scope = typeck::Scope::new(e, &entities, true, &doc.roles);
            if let Err(problem) = typeck::check(&p.when, &scope) {
                let (code, msg) = problem.split();
                self.d(
                    code,
                    format!("policies.{}.when", p.name),
                    p.pos,
                    msg,
                    "Fix the expression; see Expressions in docs/format.md.",
                );
            }
        }
        for h in &doc.holes {
            let Some(when) = &h.when else { continue };
            let inputs = typeck::hole_entity(h, &entities);
            let scope = typeck::Scope::new(&inputs, &entities, true, &doc.roles);
            if let Err(problem) = typeck::check(when, &scope) {
                let (code, msg) = problem.split();
                self.d(
                    code,
                    format!("holes.{}.when", h.name),
                    h.pos,
                    msg,
                    "Fix the expression; it uses the hole's inputs. See Holes in docs/format.md.",
                );
            }
        }
    }

    /// SVT007: a logged-out-only policy that grants more than signed-in people get. Signed-in
    /// people are covered only by a policy for the same action whose condition is the same
    /// expression or `true`; anything narrower means signing in would take access away.
    fn anon_only(&mut self, doc: &Document) {
        let cond = |s: &str| expr::parse(s).ok();
        for p in doc.policies.iter().filter(|p| p.role == Role::Anon) {
            let mine = cond(&p.when);
            for a in &p.allow {
                let covered = doc.policies.iter().any(|q| {
                    let theirs = cond(&q.when);
                    q.on.name == p.on.name
                        && q.role != Role::Anon
                        && q.allow.contains(a)
                        && (theirs == Some(expr::Expr::Bool(true)) || theirs == mine)
                });
                if !covered {
                    let verb = match a {
                        Action::Select => "see",
                        Action::Insert => "create",
                        Action::Update => "change",
                        Action::Delete => "delete",
                    };
                    let msg = format!(
                        "Logged-out visitors can {verb} {} rows through `{}` that signed-in people may not be able to {verb}.",
                        p.on.name, p.name
                    );
                    self.d(
                        Code::Svt007,
                        format!("policies.{}.as", p.name),
                        p.pos,
                        msg,
                        "Use `as: anyone` so signed-in people get the same access.",
                    );
                }
            }
        }
    }

    /// SVT001: exposed entities need a policy. SVT004: every entity needs some write path.
    fn access(&mut self, doc: &Document) {
        for e in &doc.entities {
            let policies: Vec<&Policy> = doc
                .policies
                .iter()
                .filter(|p| p.on.name == e.name)
                .collect();
            let ep = format!("entities.{}", e.name);
            if !e.private && policies.is_empty() {
                let msg = format!("{} is exposed through the API but has no policy.", e.name);
                self.d(
                    Code::Svt001,
                    ep.clone(),
                    e.pos,
                    msg,
                    &format!("Add a policy on {}, or mark it private: true.", e.name),
                );
            }
            let api_write = policies.iter().any(|p| {
                p.allow
                    .iter()
                    .any(|a| matches!(a, Action::Insert | Action::Update))
            });
            let hole_write = doc
                .holes
                .iter()
                .any(|h| h.writes.iter().any(|w| w.name == e.name));
            if !api_write && !hole_write {
                let msg = format!(
                    "Nothing can write {}: no policy allows insert or update, and no hole writes it.",
                    e.name
                );
                self.d(
                    Code::Svt004,
                    ep,
                    e.pos,
                    msg,
                    "Add a write policy, or list it in a hole's `writes`.",
                );
            }
        }
    }
}
