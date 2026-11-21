//! `svetra view` (docs/view.md): who can see and change what, as one page. Every answer comes
//! from the access tests' evaluator and every sentence from `explain`, so the page can't say
//! something the tests or `explain` don't.

use super::explain::parts;
use super::words::{plural, sentence, singular};
use crate::compile::access_tests::{Analysis, Check, TestPerson, analyse};
use crate::model::*;
use serde_json::{Value, json};

const ACTIONS: [(&str, Action); 4] = [
    ("see", Action::Select),
    ("create", Action::Insert),
    ("change", Action::Update),
    ("delete", Action::Delete),
];

fn person_label(p: &TestPerson) -> String {
    let base = match (p.name.as_str(), &p.role) {
        ("anon", _) => return "Someone not signed in".into(),
        ("alice", _) => return "A signed-in person".into(),
        ("bob", _) => return "Another signed-in person".into(),
        ("carol", _) => return "A signed-in person who owns nothing".into(),
        (_, Some((r, None))) => r.clone(),
        (_, Some((r, Some(s)))) => format!("{r} at a {}", singular(s)),
        (name, None) => name.to_string(),
    };
    if p.name.ends_with("without a second factor") {
        format!("{base} without a second factor")
    } else {
        base
    }
}

/// Which row this is, from the person's side: theirs, someone else's, or in their scope.
fn row_label(p: &TestPerson, e: &Entity, row: char) -> String {
    let one = singular(&e.name);
    let first = matches!(row, 'a' | 'c');
    let owned = e.fields.iter().any(|f| f.ty == FieldType::User);
    let scope = p.role.as_ref().and_then(|(_, s)| s.as_deref());
    let points_at = |s: &str| {
        e.fields
            .iter()
            .any(|f| matches!(&f.ty, FieldType::Ref(t) if t.name == s))
    };
    // The people who own rows see them as theirs or not; a scoped role sees them as in its
    // scope or not; anyone else sees two different rows.
    let base = match (p.name.as_str(), scope) {
        ("alice", _) | ("bob", _) if owned => {
            if first == (p.name == "alice") {
                format!("their own {one}")
            } else {
                format!("someone else's {one}")
            }
        }
        (_, Some(s)) if s == e.name => {
            if first {
                format!("their {one}")
            } else {
                format!("another {one}")
            }
        }
        (_, Some(s)) if points_at(s) => {
            let s = singular(s);
            if first {
                format!("a {one} at their {s}")
            } else {
                format!("a {one} at another {s}")
            }
        }
        _ if owned && first => format!("one person's {one}"),
        _ if owned => format!("another person's {one}"),
        _ if first => format!("one {one}"),
        _ => format!("another {one}"),
    };
    if matches!(row, 'c' | 'd') {
        format!("a new one: {base}")
    } else {
        base
    }
}

fn access(checks: &[&Check]) -> &'static str {
    if checks.is_empty() || checks.iter().all(|c| c.expected == Some(false)) {
        "none"
    } else if checks.iter().any(|c| c.expected.is_none()) {
        "unknown"
    } else if checks.iter().all(|c| c.expected == Some(true)) {
        "all"
    } else {
        "some"
    }
}

/// The view model (docs/view.md, The view model).
pub fn model(doc: &Document, src: &str) -> Value {
    let Analysis { people, checks, .. } = analyse(doc);
    let pick = |person: &str, entity: &str, action: &str| -> Vec<&Check> {
        checks
            .iter()
            .filter(|c| c.person == person && c.entity == entity && c.action == action)
            .collect()
    };
    let mut cells = Vec::new();
    for p in &people {
        for e in &doc.entities {
            let lines = parts::access(doc, e);
            // `explain` gives a sentence to each permissive policy; a restrictive one narrows them.
            let policies: Vec<&Policy> = doc
                .policies
                .iter()
                .filter(|x| x.on.name == e.name && !x.restrictive)
                .collect();
            let sentence_of = |name: &str| {
                policies
                    .iter()
                    .position(|x| x.name == name)
                    .map(|i| lines[i].clone())
            };
            for (verb, action) in ACTIONS {
                let found = pick(&p.name, &e.name, verb);
                let rows: Vec<Value> = found
                    .iter()
                    .map(|c| {
                        json!({
                            "row": c.row.to_string(),
                            "label": row_label(p, e, c.row),
                            "allowed": c.expected,
                        })
                    })
                    .collect();
                let access = if e.private { "none" } else { access(&found) };
                let mut because: Vec<String> = Vec::new();
                for c in &found {
                    for name in &c.because {
                        if let Some(s) = sentence_of(name)
                            && !because.contains(&s)
                        {
                            because.push(s);
                        }
                    }
                }
                if because.is_empty() {
                    // What rules out the rest: "Nobody can see bookings any other way.", or
                    // the private-entity line.
                    because = lines
                        .iter()
                        .skip(if e.private { 0 } else { policies.len() })
                        .filter(|l| e.private || l.split([' ', ',']).any(|w| w == verb))
                        .cloned()
                        .collect();
                }
                let only: Vec<String> = policies
                    .iter()
                    .zip(&lines)
                    .filter(|(x, _)| x.allow.contains(&action))
                    .map(|(_, l)| l.clone())
                    .collect();
                let mut cell = json!({
                    "person": p.name,
                    "entity": e.name,
                    "action": verb,
                    "access": access,
                    "rows": rows,
                    "because": because,
                    "rules": only,
                    "tests": found.iter().map(|c| c.number).collect::<Vec<_>>(),
                });
                // Which fields an update may change, when a rule limits them (docs/view.md).
                if action == Action::Update {
                    let limits: Vec<Value> = policies
                        .iter()
                        .zip(&lines)
                        .filter(|(x, _)| found.iter().any(|c| c.column_rules.contains(&x.name)))
                        .map(|(x, l)| {
                            let fields: Vec<String> =
                                x.only.iter().flatten().map(|f| f.name.clone()).collect();
                            json!({
                                "fields": fields,
                                "sentence": parts::only_sentence(&fields),
                                "rule": l,
                            })
                        })
                        .collect();
                    if !limits.is_empty() {
                        cell["only"] = Value::Array(limits);
                    }
                }
                cells.push(cell);
            }
        }
    }
    let holes: Vec<Value> = doc
        .holes
        .iter()
        .map(|h| {
            let callers: Vec<Value> = people
                .iter()
                .map(|p| {
                    let found = pick(&p.name, &h.name, "call");
                    json!({
                        "person": p.name,
                        "access": access(&found),
                        "tests": found.iter().map(|c| c.number).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({ "name": h.name, "lines": parts::hole(doc, h), "callers": callers })
        })
        .collect();
    json!({
        "svetra": doc.version,
        "system": doc.system.name,
        "revision": crate::compile::revision(src),
        "people": people.iter().map(|p| {
            let mut v = json!({ "id": p.name, "label": person_label(p) });
            if let Some((r, s)) = &p.role {
                v["role"] = json!(r);
                if let Some(s) = s {
                    v["scope"] = json!(s);
                }
            }
            v
        }).collect::<Vec<_>>(),
        "entities": doc.entities.iter().map(|e| json!({
            "name": e.name,
            "label": sentence(&plural(&e.name)),
            "private": e.private,
            "fields": parts::fields(e),
            "links": e.fields.iter().filter_map(|f| match &f.ty {
                FieldType::Ref(t) => Some(json!({ "field": f.name, "to": t.name })),
                _ => None,
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "cells": cells,
        "holes": holes,
        "signing_in": parts::signing_in(doc),
        "before_going_live": parts::before_going_live(doc),
    })
}

/// The view model as pretty JSON, with a final newline.
pub fn json(doc: &Document, src: &str) -> String {
    serde_json::to_string_pretty(&model(doc, src)).unwrap_or_default() + "\n"
}

/// The page: one self-contained HTML file with the model embedded.
pub fn page(doc: &Document, src: &str) -> String {
    // `</` can't appear inside a script element, so the embedded JSON escapes it.
    let data = serde_json::to_string(&model(doc, src))
        .unwrap_or_default()
        .replace("</", "<\\/");
    include_str!("view.html")
        .replace("{{title}}", &doc.system.name)
        .replace("{{data}}", &data)
}
