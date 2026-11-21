//! Safe change (docs/change.md): what a change to `svetra.yaml` does to a running app. Compares
//! two valid designs as typed models, classifies every change by what happens to the data, and
//! compares who can see and change what with the access tests' evaluator.

use crate::compile::names::{q, table};
use crate::diag::{Code, Diagnostic};
use crate::model::*;
use crate::understand::view;
use crate::understand::words::{plural, singular};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Data {
    Safe,
    Breaking,
    Destructive,
}

impl Data {
    pub fn word(self) -> &'static str {
        match self {
            Data::Safe => "safe",
            Data::Breaking => "breaking",
            Data::Destructive => "destructive",
        }
    }
}

/// One change, classified.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// A stable id, such as `remove_field` or `wider_access` (docs/change.md).
    pub kind: &'static str,
    pub path: String,
    pub data: Data,
    /// `wider` or `narrower` for access changes.
    pub access: Option<&'static str>,
    pub message: String,
    pub hint: Option<String>,
    /// For access changes: the `explain` sentences that allow it now.
    pub because: Vec<String>,
}

pub struct Plan {
    pub changes: Vec<Change>,
    /// SVT030 and SVT031: problems with `was`.
    pub diagnostics: Vec<Diagnostic>,
    /// `alter ... rename` statements, in order: tables, then columns.
    pub renames: Vec<String>,
}

impl Plan {
    /// Kinds that need an explicit yes: destructive data changes and wider access.
    pub fn needs_yes(&self) -> Vec<&'static str> {
        let mut kinds: Vec<&'static str> = Vec::new();
        for c in &self.changes {
            if (c.data == Data::Destructive || c.access == Some("wider"))
                && !kinds.contains(&c.kind)
            {
                kinds.push(c.kind);
            }
        }
        kinds
    }
}

fn change(kind: &'static str, path: String, data: Data, message: String) -> Change {
    Change {
        kind,
        path,
        data,
        access: None,
        message,
        hint: None,
        because: Vec::new(),
    }
}

fn hint(mut c: Change, h: &str) -> Change {
    c.hint = Some(h.into());
    c
}

fn diag(code: Code, path: String, at: &NameRef, message: String, hint: &str) -> Diagnostic {
    Diagnostic {
        code,
        path,
        line: at.pos.line,
        col: at.pos.col,
        message,
        hint: hint.into(),
    }
}

/// The same type, ignoring where a `ref` was written, and following a renamed target.
fn same_type(a: &FieldType, b: &FieldType, refs: &HashMap<&str, &str>) -> bool {
    match (a, b) {
        (FieldType::Ref(x), FieldType::Ref(y)) => {
            refs.get(x.name.as_str())
                .copied()
                .unwrap_or(x.name.as_str())
                == y.name
        }
        _ => a == b,
    }
}

/// Is `to` able to hold every value `from` holds, without rounding or rejecting any?
fn widens(from: &FieldType, to: &FieldType, refs: &HashMap<&str, &str>) -> bool {
    use FieldType::*;
    let fits = |p: u8, s: u8, p2: u8, s2: u8| s2 >= s && p2 - s2 >= p - s;
    match (from, to) {
        (a, b) if a == b => true,
        (Int, Decimal { precision, scale } | Money { precision, scale }) => precision - scale >= 19,
        (
            Decimal {
                precision: p,
                scale: s,
            }
            | Money {
                precision: p,
                scale: s,
            },
            Decimal {
                precision: p2,
                scale: s2,
            }
            | Money {
                precision: p2,
                scale: s2,
            },
        ) => fits(*p, *s, *p2, *s2),
        (Enum(a), Enum(b)) => a.iter().all(|v| b.contains(v)),
        (Ref(a), Ref(b)) => refs.get(a.name.as_str()) == Some(&b.name.as_str()),
        _ => false,
    }
}

/// Every change from `old` to `new`, both valid. `old_src` and `new_src` are their texts, for
/// the access comparison.
pub fn plan(old: &Document, old_src: &str, new: &Document, new_src: &str) -> Plan {
    let mut changes = Vec::new();
    let mut diagnostics = Vec::new();
    let mut renames = Vec::new();
    let old_by: HashMap<&str, &Entity> =
        old.entities.iter().map(|e| (e.name.as_str(), e)).collect();

    // Which old entity each new one continues: its `was`, or the same name.
    let mut matched: Vec<(&Entity, &Entity)> = Vec::new();
    for e in &new.entities {
        let path = format!("entities.{}", e.name);
        match &e.was {
            Some(w) if old_by.contains_key(e.name.as_str()) => diagnostics.push(diag(
                Code::Svt031,
                format!("{path}.was"),
                w,
                format!("{} is already called {} in the design the database has, so `was: {}` does nothing.", e.name, e.name, w.name),
                &format!("Remove `was: {}`; the rename is applied.", w.name),
            )),
            Some(w) => match old_by.get(w.name.as_str()) {
                Some(o) => {
                    matched.push((o, e));
                    renames.push(format!("alter table {} rename to {};", table(&o.name), q(&crate::compile::names::snake(&e.name))));
                    changes.push(change(
                        "rename_entity",
                        path,
                        Data::Safe,
                        format!("rename {} to {}: every row is kept", w.name, e.name),
                    ));
                    continue;
                }
                None => diagnostics.push(diag(
                    Code::Svt030,
                    format!("{path}.was"),
                    w,
                    format!("`was: {}` names no entity in the design the database has.", w.name),
                    "Fix the old name, or remove `was` if this is a new entity.",
                )),
            },
            None => {}
        }
        if let Some(o) = old_by.get(e.name.as_str()) {
            matched.push((o, e));
        } else {
            changes.push(change(
                "add_entity",
                path,
                Data::Safe,
                format!("add {}", plural(&e.name)),
            ));
        }
    }
    // Old names to new, for refs and access cells.
    let names: HashMap<&str, &str> = matched
        .iter()
        .map(|(o, n)| (o.name.as_str(), n.name.as_str()))
        .collect();
    for o in &old.entities {
        if !names.contains_key(o.name.as_str()) {
            let mut c = change(
                "remove_entity",
                format!("entities.{}", o.name),
                Data::Destructive,
                format!("remove {}: every {} is deleted", o.name, singular(&o.name)),
            );
            if let Some(n) = new
                .entities
                .iter()
                .find(|n| !old_by.contains_key(n.name.as_str()) && n.was.is_none())
            {
                c.hint = Some(format!(
                    "Did you rename {} to {}? Add `was: {}` to {}, or this deletes every {}.",
                    o.name,
                    n.name,
                    o.name,
                    n.name,
                    singular(&o.name)
                ));
            }
            changes.push(c);
        }
    }

    for (o, n) in &matched {
        entity(o, n, &names, &mut changes, &mut diagnostics, &mut renames);
    }
    roles(old, new, &mut changes);
    holes(old, new, &mut changes);
    policies(old, new, &names, &mut changes);
    if auth_words(old.auth.as_ref()) != auth_words(new.auth.as_ref()) {
        changes.push(change(
            "change_auth",
            "auth".into(),
            Data::Safe,
            "change how people sign in".into(),
        ));
    }
    access(old, old_src, new, new_src, &names, &mut changes);
    Plan {
        changes,
        diagnostics,
        renames,
    }
}

fn entity(
    o: &Entity,
    n: &Entity,
    names: &HashMap<&str, &str>,
    changes: &mut Vec<Change>,
    diagnostics: &mut Vec<Diagnostic>,
    renames: &mut Vec<String>,
) {
    let path = format!("entities.{}", n.name);
    let old_by: HashMap<&str, &Field> = o.fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut kept: Vec<&str> = Vec::new();
    for f in &n.fields {
        let fpath = format!("{path}.fields.{}", f.name);
        let from = match &f.was {
            Some(w) if old_by.contains_key(f.name.as_str()) => {
                diagnostics.push(diag(
                    Code::Svt031,
                    format!("{fpath}.was"),
                    w,
                    format!("{}.{} already exists in the design the database has, so `was: {}` does nothing.", n.name, f.name, w.name),
                    &format!("Remove `was: {}`; the rename is applied.", w.name),
                ));
                old_by.get(f.name.as_str())
            }
            Some(w) => match old_by.get(w.name.as_str()) {
                Some(of) => {
                    renames.push(format!(
                        "alter table {} rename column {} to {};",
                        table(&n.name),
                        q(&w.name),
                        q(&f.name)
                    ));
                    changes.push(change(
                        "rename_field",
                        fpath.clone(),
                        Data::Safe,
                        format!(
                            "rename {}.{} to {}: every value is kept",
                            n.name, w.name, f.name
                        ),
                    ));
                    Some(of)
                }
                None => {
                    diagnostics.push(diag(
                        Code::Svt030,
                        format!("{fpath}.was"),
                        w,
                        format!(
                            "`was: {}` names no field of {} in the design the database has.",
                            w.name, o.name
                        ),
                        "Fix the old name, or remove `was` if this is a new field.",
                    ));
                    None
                }
            },
            None => old_by.get(f.name.as_str()),
        };
        let Some(of) = from else {
            let ready = !f.required || f.default.is_some() || f.auto_now;
            changes.push(if ready {
                change(
                    "add_field",
                    fpath,
                    Data::Safe,
                    format!("add {}.{}", n.name, f.name),
                )
            } else {
                hint(
                    change(
                        "add_field",
                        fpath,
                        Data::Breaking,
                        format!(
                            "add {}.{}, required with no default: existing {} have no value",
                            n.name,
                            f.name,
                            plural(&n.name)
                        ),
                    ),
                    "Add a `default`, or add it as optional first and fill it in.",
                )
            });
            continue;
        };
        kept.push(of.name.as_str());
        if !same_type(&of.ty, &f.ty, names) {
            match (&of.ty, &f.ty) {
                (FieldType::Enum(a), FieldType::Enum(b)) => {
                    if b.iter().any(|v| !a.contains(v)) {
                        changes.push(change(
                            "add_enum_value",
                            fpath.clone(),
                            Data::Safe,
                            format!("{}.{} gains values", n.name, f.name),
                        ));
                    }
                    let gone: Vec<&String> = a.iter().filter(|v| !b.contains(v)).collect();
                    if !gone.is_empty() {
                        let list: Vec<&str> = gone.iter().map(|s| s.as_str()).collect();
                        changes.push(hint(
                            change(
                                "remove_enum_value",
                                fpath.clone(),
                                Data::Breaking,
                                format!(
                                    "{}.{} loses {}: rows holding it fail the new rule",
                                    n.name,
                                    f.name,
                                    list.join(", ")
                                ),
                            ),
                            "Change those rows first, or keep the value.",
                        ));
                    }
                }
                (a, b) if widens(a, b, names) => changes.push(change(
                    "change_type",
                    fpath.clone(),
                    Data::Safe,
                    format!("{}.{} can hold more values", n.name, f.name),
                )),
                _ => changes.push(hint(
                    change(
                        "change_type",
                        fpath.clone(),
                        Data::Destructive,
                        format!(
                            "{}.{} changes type: existing values can be rejected or rounded",
                            n.name, f.name
                        ),
                    ),
                    "Add a new field instead and copy the values over, then remove the old one.",
                )),
            }
        }
        if !of.required && f.required && !f.primary {
            changes.push(hint(
                change(
                    "make_required",
                    fpath.clone(),
                    Data::Breaking,
                    format!(
                        "{}.{} becomes required: rows with no value fail",
                        n.name, f.name
                    ),
                ),
                "Fill in every empty value first.",
            ));
        }
        if of.required && !f.required {
            changes.push(change(
                "make_optional",
                fpath.clone(),
                Data::Safe,
                format!("{}.{} becomes optional", n.name, f.name),
            ));
        }
        if of.on_delete != f.on_delete {
            let (data, what) = match f.on_delete {
                OnDelete::Cascade => (Data::Breaking, "now deletes this row too"),
                OnDelete::SetNull => (Data::Safe, "now clears this field"),
                OnDelete::Restrict => (Data::Safe, "is now refused while this row points at it"),
            };
            let mut c = change(
                "change_on_delete",
                fpath.clone(),
                data,
                format!(
                    "{}.{}: deleting the row it points at {what}",
                    n.name, f.name
                ),
            );
            if f.on_delete == OnDelete::Cascade {
                c.hint = Some("Check that losing these rows with their parent is intended.".into());
            }
            changes.push(c);
        }
        if !of.unique && f.unique {
            changes.push(hint(
                change(
                    "add_unique",
                    fpath.clone(),
                    Data::Breaking,
                    format!(
                        "{}.{} must be unique: existing duplicates fail",
                        n.name, f.name
                    ),
                ),
                "Remove duplicates first.",
            ));
        }
        if of.unique && !f.unique {
            changes.push(change(
                "remove_unique",
                fpath,
                Data::Safe,
                format!("{}.{} no longer has to be unique", n.name, f.name),
            ));
        }
    }
    for of in &o.fields {
        if !kept.contains(&of.name.as_str()) {
            let mut c = change(
                "remove_field",
                format!("{path}.fields.{}", of.name),
                Data::Destructive,
                format!("remove {}.{}: every value is deleted", o.name, of.name),
            );
            // A new field of the same type that nothing explains is probably a rename.
            if let Some(nf) = n.fields.iter().find(|nf| {
                nf.was.is_none()
                    && same_type(&of.ty, &nf.ty, names)
                    && !o.fields.iter().any(|x| x.name == nf.name)
            }) {
                c.hint = Some(format!(
                    "Did you rename {} to {}? Add `was: {}` to {}, or this loses every value.",
                    of.name, nf.name, of.name, nf.name
                ));
            }
            changes.push(c);
        }
    }
    // Constraints by name; unique lists and indexes by their fields.
    let field_name = |name: &str| -> String {
        n.fields
            .iter()
            .find(|f| f.was.as_ref().is_some_and(|w| w.name == name))
            .map_or(name.to_string(), |f| f.name.clone())
    };
    for c in &n.constraints {
        match o.constraints.iter().find(|x| x.name == c.name) {
            None => changes.push(hint(
                change(
                    "add_constraint",
                    format!("{path}.constraints.{}", c.name),
                    Data::Breaking,
                    format!(
                        "new rule on {}: {}; existing rows may break it",
                        plural(&n.name),
                        c.name
                    ),
                ),
                "Check existing rows against it first.",
            )),
            Some(x) if x.expr != c.expr => changes.push(hint(
                change(
                    "change_constraint",
                    format!("{path}.constraints.{}", c.name),
                    Data::Breaking,
                    format!(
                        "rule {} on {} changes; existing rows may break it",
                        c.name,
                        plural(&n.name)
                    ),
                ),
                "Check existing rows against it first.",
            )),
            _ => {}
        }
    }
    for x in &o.constraints {
        if !n.constraints.iter().any(|c| c.name == x.name) {
            changes.push(change(
                "remove_constraint",
                format!("{path}.constraints.{}", x.name),
                Data::Safe,
                format!("rule {} on {} is removed", x.name, plural(&n.name)),
            ));
        }
    }
    let lists = |ls: &[FieldList], map: bool| -> Vec<Vec<String>> {
        ls.iter()
            .map(|l| {
                l.fields
                    .iter()
                    .map(|f| {
                        if map {
                            field_name(&f.name)
                        } else {
                            f.name.clone()
                        }
                    })
                    .collect()
            })
            .collect()
    };
    for (kind_add, kind_remove, data_add, a, b) in [
        (
            "add_unique",
            "remove_unique",
            Data::Breaking,
            lists(&o.unique, true),
            lists(&n.unique, false),
        ),
        (
            "add_index",
            "remove_index",
            Data::Safe,
            lists(&o.indexes, true),
            lists(&n.indexes, false),
        ),
    ] {
        for l in b.iter().filter(|l| !a.contains(l)) {
            let mut c = change(
                kind_add,
                format!(
                    "{path}.{}",
                    if kind_add == "add_index" {
                        "indexes"
                    } else {
                        "unique"
                    }
                ),
                data_add,
                format!(
                    "{} on {} ({})",
                    if kind_add == "add_index" {
                        "new index"
                    } else {
                        "new unique rule"
                    },
                    plural(&n.name),
                    l.join(", ")
                ),
            );
            if data_add == Data::Breaking {
                c.hint = Some("Remove duplicates first.".into());
            }
            changes.push(c);
        }
        for l in a.iter().filter(|l| !b.contains(l)) {
            changes.push(change(
                kind_remove,
                path.clone(),
                Data::Safe,
                format!(
                    "{} on {} ({}) is removed",
                    if kind_remove == "remove_index" {
                        "index"
                    } else {
                        "unique rule"
                    },
                    plural(&n.name),
                    l.join(", ")
                ),
            ));
        }
    }
    if o.audit != n.audit {
        changes.push(audit_change(path.clone(), n.audit, &plural(&n.name)));
    }
    if !o.private && n.private {
        changes.push(hint(
            change(
                "make_private",
                path.clone(),
                Data::Breaking,
                format!(
                    "{} become private: the app can no longer reach them",
                    plural(&n.name)
                ),
            ),
            "Move any app code that reads them into custom code first.",
        ));
    }
    if o.private && !n.private {
        changes.push(change(
            "make_public",
            path,
            Data::Safe,
            format!(
                "{} become reachable from the app, through their rules",
                plural(&n.name)
            ),
        ));
    }
}

fn roles(old: &Document, new: &Document, changes: &mut Vec<Change>) {
    for r in &new.roles {
        match old.roles.iter().find(|x| x.name == r.name) {
            None => changes.push(change("add_role", format!("roles.{}", r.name), Data::Safe, format!("add the {} role", r.name))),
            Some(x) if x.scope.as_ref().map(|s| &s.name) != r.scope.as_ref().map(|s| &s.name) => changes.push(hint(
                change("change_role_scope", format!("roles.{}", r.name), Data::Destructive, format!("the {} role changes what it's held for: existing grants point at the wrong rows", r.name)),
                "Add a new role instead, and grant it again.",
            )),
            _ => {}
        }
    }
    for x in &old.roles {
        if !new.roles.iter().any(|r| r.name == x.name) {
            changes.push(change(
                "remove_role",
                format!("roles.{}", x.name),
                Data::Destructive,
                format!("remove the {} role: everyone who holds it loses it", x.name),
            ));
        }
    }
}

fn signature(h: &Hole) -> String {
    format!(
        "{:?} -> {:?}",
        h.input.iter().map(|p| (&p.name, &p.ty)).collect::<Vec<_>>(),
        h.output
    )
}

/// Turning `audit` on or off (docs/change.md). Neither touches data; turning it off is listed so it
/// is seen, because what happens next is no longer recorded.
fn audit_change(path: String, on: bool, what: &str) -> Change {
    if on {
        change(
            "add_audit",
            path,
            Data::Safe,
            format!("{what} start being recorded in the audit log"),
        )
    } else {
        hint(
            change(
                "remove_audit",
                path,
                Data::Safe,
                format!("{what} are no longer recorded in the audit log"),
            ),
            "Records already in the log stay. Turn audit back on to keep recording.",
        )
    }
}

/// What starts a hole: a scheduled one is told apart by its cron expression.
fn starts(h: &Hole) -> Option<&str> {
    match &h.kind {
        HoleKind::Schedule(s) => Some(&s.cron),
        HoleKind::Function => None,
    }
}

fn holes(old: &Document, new: &Document, changes: &mut Vec<Change>) {
    for h in &new.holes {
        let path = format!("holes.{}", h.name);
        if let Some(x) = old.holes.iter().find(|x| x.name == h.name)
            && x.audit != h.audit
        {
            changes.push(audit_change(
                path.clone(),
                h.audit,
                &format!("calls of {}", h.name),
            ));
        }
        match old.holes.iter().find(|x| x.name == h.name) {
            None => changes.push(change("add_hole", path, Data::Safe, match starts(h) {
                Some(_) => format!("add custom code {}, which runs by itself on a schedule", h.name),
                None => format!("add custom code {}", h.name),
            })),
            Some(x) if starts(x) != starts(h) => changes.push(hint(
                change("change_schedule", path, Data::Breaking, format!("custom code {} starts differently: it runs at other times, or is now called by people or by the clock instead", h.name)),
                "Check what the body does at the new times, and update any app code that called it.",
            )),
            Some(x) if signature(x) != signature(h) => changes.push(hint(
                change("change_hole", path, Data::Breaking, format!("custom code {} takes or returns something different: app code calling it must change", h.name)),
                "Update the app's calls, using the new typed function in src/svetra.gen.ts.",
            )),
            _ => {}
        }
    }
    for x in &old.holes {
        if !new.holes.iter().any(|h| h.name == x.name) {
            changes.push(match starts(x) {
                Some(_) => hint(
                    change(
                        "remove_hole",
                        format!("holes.{}", x.name),
                        Data::Breaking,
                        format!("remove custom code {}: it stops running on its schedule", x.name),
                    ),
                    &format!("The generated schema unschedules its job while another scheduled hole remains. If it was the last, run select cron.unschedule('svetra_{}'); by hand.", x.name),
                ),
                None => hint(
                    change(
                        "remove_hole",
                        format!("holes.{}", x.name),
                        Data::Breaking,
                        format!("remove custom code {}: app code calling it fails", x.name),
                    ),
                    "Remove the app's calls first.",
                ),
            });
        }
    }
}

fn policies(
    old: &Document,
    new: &Document,
    names: &HashMap<&str, &str>,
    changes: &mut Vec<Change>,
) {
    let key = |p: &Policy| {
        let only: Option<Vec<&str>> = p
            .only
            .as_ref()
            .map(|o| o.iter().map(|f| f.name.as_str()).collect());
        format!(
            "{:?} {:?} {} {:?} {:?} {}",
            p.allow, p.role, p.when, p.check, only, p.restrictive
        )
    };
    for p in &new.policies {
        let path = format!("policies.{}", p.name);
        match old.policies.iter().find(|x| x.name == p.name) {
            None => changes.push(change(
                "add_policy",
                path,
                Data::Safe,
                format!("add rule {}", p.name),
            )),
            Some(x)
                if key(x) != key(p)
                    || names.get(x.on.name.as_str()) != Some(&p.on.name.as_str()) =>
            {
                changes.push(change(
                    "change_policy",
                    path.clone(),
                    Data::Safe,
                    format!("change rule {}", p.name),
                ));
                changes.extend(only_change(x, p, new, &path));
            }
            _ => {}
        }
    }
    for x in &old.policies {
        if !new.policies.iter().any(|p| p.name == x.name) {
            changes.push(change(
                "remove_policy",
                format!("policies.{}", x.name),
                Data::Safe,
                format!("remove rule {}", x.name),
            ));
        }
    }
}

/// The access line for a change to a policy's `only` (docs/change.md, column rules): a shorter
/// list, or gaining one, locks fields (narrower); a longer list, or losing it, frees them (wider).
/// Fields are matched through `was`, so renaming a listed field changes nothing.
fn only_change(old: &Policy, new: &Policy, doc: &Document, path: &str) -> Option<Change> {
    let entity = doc.entities.iter().find(|e| e.name == new.on.name);
    let was = |f: &str| -> String {
        entity
            .and_then(|e| e.fields.iter().find(|x| x.name == f))
            .and_then(|x| x.was.as_ref())
            .map_or(f, |w| w.name.as_str())
            .to_string()
    };
    let before: Option<Vec<&str>> = old
        .only
        .as_ref()
        .map(|o| o.iter().map(|f| f.name.as_str()).collect());
    let after: Option<Vec<(String, &str)>> = new
        .only
        .as_ref()
        .map(|o| o.iter().map(|f| (was(&f.name), f.name.as_str())).collect());
    let things = plural(&new.on.name);
    let (wider, message) = match (&before, &after) {
        (None, None) => return None,
        (None, Some(a)) => (
            false,
            format!(
                "Rule {} now lets an update change only {} of {things} (was: every field).",
                new.name,
                a.iter().map(|(_, n)| *n).collect::<Vec<_>>().join(", "),
            ),
        ),
        (Some(b), None) => (
            true,
            format!(
                "Rule {} now lets an update change every field of {things} (was: only {}).",
                new.name,
                b.join(", ")
            ),
        ),
        (Some(b), Some(a)) => {
            let added: Vec<&str> = a
                .iter()
                .filter(|(o, _)| !b.contains(&o.as_str()))
                .map(|(_, n)| *n)
                .collect();
            let removed: Vec<&str> = b
                .iter()
                .filter(|o| !a.iter().any(|(x, _)| x == *o))
                .copied()
                .collect();
            // Freeing a field is wider even if another was locked in the same edit.
            if !added.is_empty() {
                (
                    true,
                    format!(
                        "Rule {} now also lets an update change {}.",
                        new.name,
                        added.join(", ")
                    ),
                )
            } else if !removed.is_empty() {
                (
                    false,
                    format!(
                        "Rule {} no longer lets an update change {}.",
                        new.name,
                        removed.join(", ")
                    ),
                )
            } else {
                return None;
            }
        }
    };
    let mut c = change(
        if wider {
            "wider_access"
        } else {
            "narrower_access"
        },
        path.to_string(),
        Data::Safe,
        message,
    );
    c.access = Some(if wider { "wider" } else { "narrower" });
    Some(c)
}

fn auth_words(a: Option<&Auth>) -> Option<String> {
    a.map(|a| {
        let methods: Vec<&str> = a.sign_in.iter().map(|m| m.name.as_str()).collect();
        format!(
            "{methods:?} {} {} {} {}",
            a.invite_only(),
            a.confirms_email(),
            a.min_length(),
            a.requirements()
        )
    })
}

fn rank(access: &str) -> Option<u8> {
    match access {
        "none" => Some(0),
        "some" => Some(1),
        "all" => Some(2),
        _ => None,
    }
}

fn words(access: &str) -> &'static str {
    match access {
        "none" => "none",
        "some" => "some",
        "all" => "all",
        _ => "can't tell",
    }
}

/// Who can see and change what, before and after, cell by cell (docs/change.md, Access).
fn access(
    old: &Document,
    old_src: &str,
    new: &Document,
    new_src: &str,
    names: &HashMap<&str, &str>,
    changes: &mut Vec<Change>,
) {
    let (a, b) = (view::model(old, old_src), view::model(new, new_src));
    let label = |m: &Value, id: &str| {
        m["people"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["id"] == id))
            .and_then(|p| p["label"].as_str())
            .unwrap_or(id)
            .to_string()
    };
    let empty = Vec::new();
    let before: HashMap<(String, String, String), &Value> = a["cells"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|c| {
            let e = c["entity"].as_str().unwrap_or_default();
            let e = names.get(e).copied().unwrap_or(e);
            (
                (
                    c["person"].as_str().unwrap_or_default().to_string(),
                    e.to_string(),
                    c["action"].as_str().unwrap_or_default().to_string(),
                ),
                c,
            )
        })
        .collect();
    for c in b["cells"].as_array().unwrap_or(&empty) {
        let (person, entity, action) = (
            c["person"].as_str().unwrap_or_default(),
            c["entity"].as_str().unwrap_or_default(),
            c["action"].as_str().unwrap_or_default(),
        );
        let Some(was) = before.get(&(person.into(), entity.into(), action.into())) else {
            continue;
        };
        let (from, to) = (
            was["access"].as_str().unwrap_or("unknown"),
            c["access"].as_str().unwrap_or("unknown"),
        );
        if from == to {
            continue;
        }
        let who = label(&b, person);
        let things = plural(entity);
        let (kind, dir, message) = match (rank(from), rank(to)) {
            (Some(f), Some(t)) if t > f => (
                "wider_access",
                "wider",
                format!(
                    "{who} can now {action} {} {things} (was: {}).",
                    words(to),
                    words(from)
                ),
            ),
            (Some(_), Some(0)) => (
                "narrower_access",
                "narrower",
                format!(
                    "{who} can no longer {action} {things} (was: {}).",
                    words(from)
                ),
            ),
            (Some(_), Some(_)) => (
                "narrower_access",
                "narrower",
                format!(
                    "{who} can now {action} only {} {things} (was: {}).",
                    words(to),
                    words(from)
                ),
            ),
            // Either side can't be decided: report it rather than call it unchanged.
            _ => (
                "wider_access",
                "wider",
                format!(
                    "{who}: {action} {things} can't be told apart (was: {}, now: {}).",
                    words(from),
                    words(to)
                ),
            ),
        };
        let because = c["because"]
            .as_array()
            .map(|v| {
                v.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        changes.push(Change {
            kind,
            path: format!("entities.{entity}"),
            data: Data::Safe,
            access: Some(dir),
            message,
            hint: None,
            because,
        });
    }
}

/// The committed version of `file` in git `HEAD`, the default design the database has.
/// Returns its text and a name for it, such as `HEAD:svetra.yaml`.
pub fn committed(file: &std::path::Path) -> Result<(String, String), String> {
    let dir = file
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let name = file
        .file_name()
        .map_or("svetra.yaml".into(), |n| n.to_string_lossy().to_string());
    match std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["show", &format!("HEAD:./{name}")])
        .output()
    {
        Ok(o) if o.status.success() => Ok((
            String::from_utf8_lossy(&o.stdout).to_string(),
            format!("HEAD:{name}"),
        )),
        _ => Err(format!(
            "{name} isn't committed in git HEAD here, so there's nothing to compare with. Pass --from OLD, the design the database has"
        )),
    }
}

/// The plan as JSON (docs/change.md, Output).
pub fn json(plan: &Plan, from: &str, writes: &[String]) -> Value {
    json!({
        "from": from,
        "changes": plan.changes.iter().map(|c| json!({
            "kind": c.kind,
            "path": c.path,
            "data": c.data.word(),
            "access": c.access,
            "message": c.message,
            "hint": c.hint,
            "because": c.because,
        })).collect::<Vec<_>>(),
        "needs_yes": plan.needs_yes(),
        "renames": plan.renames,
        "writes": writes,
    })
}

/// The plan for people (docs/change.md, Output).
pub fn human(plan: &Plan, system: &str, from: &str) -> String {
    let mut out = format!(
        "{} change{} to {system} (from {from})\n\n",
        plan.changes.len(),
        if plan.changes.len() == 1 { "" } else { "s" }
    );
    if plan.changes.is_empty() {
        return format!("No changes to {system} (from {from}).\n");
    }
    for c in &plan.changes {
        let tag = c.access.unwrap_or(match c.kind {
            "rename_entity" | "rename_field" => "rename",
            _ => c.data.word(),
        });
        out += &format!("  {tag:<12} {}\n", c.message);
        for b in &c.because {
            out += &format!("  {:<12}   because: {b}\n", "");
        }
        if let Some(h) = &c.hint {
            out += &format!("  {:<12}   hint: {h}\n", "");
        }
    }
    out
}
