//! `svetra infer --check-draft` (docs/infer.md, Differential testing): run a draft's generated
//! access tests against a copy of the original database and say where they disagree.
//!
//! This module reads the draft and the tests' output. Starting the copy is `migrations::Copy`.

use crate::compile::access_tests::{Analysis, analyse};
use std::collections::BTreeMap;

/// One line of pgTAP output: a check that ran.
#[derive(Debug, PartialEq)]
pub struct Tap {
    pub number: usize,
    pub ok: bool,
    pub description: String,
    /// For a failed `is` check, what the database gave and what the draft expects.
    pub have: Option<String>,
    pub want: Option<String>,
    /// For a failed test-data insert, the database's message.
    pub died: Option<String>,
}

/// The `ok` and `not ok` lines of a pgTAP run, each with the diagnostics under it.
pub fn parse_tap(out: &str) -> Vec<Tap> {
    let mut taps: Vec<Tap> = Vec::new();
    for line in out.lines() {
        let (ok, rest) = if let Some(r) = line.strip_prefix("not ok ") {
            (false, r)
        } else if let Some(r) = line.strip_prefix("ok ") {
            (true, r)
        } else {
            let Some(last) = taps.last_mut() else {
                continue;
            };
            let note = line.strip_prefix('#').map(str::trim);
            match note {
                Some(n) if n.starts_with("have:") => {
                    last.have = Some(n["have:".len()..].trim().trim_matches('\'').to_string())
                }
                Some(n) if n.starts_with("want:") => {
                    last.want = Some(n["want:".len()..].trim().trim_matches('\'').to_string())
                }
                Some(n) if n.starts_with("died:") && last.died.is_none() => {
                    // "died: 23503: insert or update on table ..." -> the message.
                    let m = n["died:".len()..].trim();
                    last.died = Some(m.split_once(": ").map_or(m, |(_, t)| t).to_string());
                }
                _ => {}
            }
            continue;
        };
        let Some((n, description)) = rest.split_once(" - ") else {
            continue;
        };
        let Ok(number) = n.trim().parse() else {
            continue;
        };
        taps.push(Tap {
            number,
            ok,
            description: description.to_string(),
            have: None,
            want: None,
            died: None,
        });
    }
    taps
}

/// The entities whose access the draft says less about than the database: a policy left as an
/// `UNTRANSLATED` comment, a `policies:` note that the draft is narrower, or row-level security
/// off. Read from the comments in the entity's block, so porting a policy and deleting its
/// comment puts the entity back in the check.
pub fn narrower(yaml: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut entity: Option<&str> = None;
    let mut in_entities = false;
    for line in yaml.lines() {
        if !line.starts_with(' ') && !line.starts_with('#') && !line.is_empty() {
            in_entities = line.starts_with("entities:");
            entity = None;
            continue;
        }
        if !in_entities {
            continue;
        }
        if let Some(name) = line
            .strip_prefix("  ")
            .filter(|l| !l.starts_with(' ') && !l.starts_with('#'))
            .and_then(|l| l.strip_suffix(':'))
        {
            entity = Some(name);
            continue;
        }
        let Some(e) = entity else { continue };
        let body = line.trim_start();
        let why = if body.starts_with("# UNTRANSLATED policy ") || body.starts_with("# policies: ")
        {
            "the draft is narrower than the database: a policy was left untranslated"
        } else if body.starts_with("# row-level security is off here") {
            "row-level security is off in the database, so there is no access rule to compare"
        } else {
            continue;
        };
        out.entry(e.to_string()).or_insert_with(|| why.to_string());
    }
    out
}

/// What the check found for one entity.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Agrees { checks: usize },
    Skipped(String),
    Disagrees { checks: usize, failed: Vec<String> },
}

#[derive(Debug, PartialEq)]
pub struct Report {
    pub entities: Vec<(String, Verdict)>,
    /// Failed checks that belong to no entity.
    pub other: Vec<String>,
}

impl Report {
    pub fn disagreements(&self) -> usize {
        self.entities
            .iter()
            .map(|(_, v)| match v {
                Verdict::Disagrees { failed, .. } => failed.len(),
                _ => 0,
            })
            .sum::<usize>()
            + self.other.len()
    }
}

/// The entity a check belongs to: the analysis knows it by number, and a check it doesn't list
/// (a column or hazard check) names it in its description.
fn entity_of<'a>(a: &'a Analysis, names: &[&'a str], t: &Tap) -> Option<&'a str> {
    if let Some(c) = a.checks.iter().find(|c| c.number == t.number) {
        return names.iter().copied().find(|n| *n == c.entity);
    }
    let text = t
        .description
        .strip_prefix("test data: ")
        .unwrap_or(&t.description);
    names
        .iter()
        .copied()
        .filter_map(|n| word_at(text, n).map(|at| (at, n)))
        .min()
        .map(|(_, n)| n)
}

/// Where `word` first appears in `text` as a whole word.
fn word_at(text: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = text[from..].find(word) {
        let at = from + i;
        let end = at + word.len();
        let before = text[..at].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            return Some(at);
        }
        from = end;
    }
    None
}

/// The entity a failed "delete is blocked" check says still points at the row.
fn blocker<'a>(names: &[&'a str], t: &Tap) -> Option<&'a str> {
    let before = t.description.strip_suffix(" still points at it")?;
    let tail = before.rsplit_once(": ")?.1;
    names.iter().copied().find(|n| {
        tail.strip_prefix(*n)
            .is_some_and(|r| r.len() == 2 && r.starts_with(' '))
    })
}

fn failure(t: &Tap) -> String {
    match (&t.have, &t.want) {
        (Some(h), Some(w)) => format!(
            "{} (the draft says {w}, the database says {h})",
            t.description
        ),
        _ => t.description.clone(),
    }
}

/// Sort the tests' results into one verdict per entity.
pub fn judge(
    a: &Analysis,
    entities: &[&str],
    skipped: &BTreeMap<String, String>,
    taps: &[Tap],
) -> Report {
    // An entity whose test rows the database refused can't be judged on access.
    let mut refused: BTreeMap<&str, String> = BTreeMap::new();
    for t in taps.iter().filter(|t| !t.ok) {
        if let Some(rest) = t.description.strip_prefix("test data: ")
            && let Some(e) = entity_of(a, entities, t)
            && rest.starts_with(e)
        {
            let why = t.died.clone().unwrap_or_else(|| "the insert failed".into());
            refused.entry(e).or_insert(why);
        }
    }
    let mut report = Report {
        entities: Vec::new(),
        other: Vec::new(),
    };
    for &e in entities {
        let mine: Vec<&Tap> = taps
            .iter()
            .filter(|t| entity_of(a, entities, t) == Some(e))
            .collect();
        let verdict = if let Some(why) = skipped.get(e) {
            Verdict::Skipped(why.clone())
        } else if let Some(why) = refused.get(e) {
            Verdict::Skipped(format!(
                "the database refused the draft's test rows, so its checks can't say anything about access: {why}"
            ))
        } else {
            let failed: Vec<String> = mine
                .iter()
                .filter(|t| !t.ok)
                // A delete blocked by rows that the database refused is the same refusal.
                .filter(|t| !blocker(entities, t).is_some_and(|b| refused.contains_key(b)))
                .map(|t| failure(t))
                .collect();
            // The checks on access; the test rows' own inserts aren't counted.
            let checks = mine
                .iter()
                .filter(|t| !t.description.starts_with("test data: "))
                .count();
            if failed.is_empty() {
                Verdict::Agrees { checks }
            } else {
                Verdict::Disagrees { checks, failed }
            }
        };
        report.entities.push((e.to_string(), verdict));
    }
    for t in taps.iter().filter(|t| !t.ok) {
        if entity_of(a, entities, t).is_none() {
            report.other.push(failure(t));
        }
    }
    report
}

impl Report {
    pub fn human(&self, file: &str, source: &str, notes: &[String]) -> String {
        let width = self
            .entities
            .iter()
            .map(|(e, _)| e.len())
            .max()
            .unwrap_or(0);
        let mut out = format!(
            "Differential check of {file} against a copy of the original database ({source}).\n"
        );
        for n in notes {
            out += &format!("  note: {n}\n");
        }
        for (e, v) in &self.entities {
            let (word, what) = match v {
                Verdict::Agrees { checks } => ("agrees", format!("{checks} checks")),
                Verdict::Skipped(why) => ("skipped", why.clone()),
                Verdict::Disagrees { checks, failed } => {
                    ("DISAGREES", format!("{} of {checks} checks", failed.len()))
                }
            };
            out += &format!("  {e:<width$}  {word:<9}  {what}\n");
        }
        for (e, v) in &self.entities {
            if let Verdict::Disagrees { failed, .. } = v {
                out += &format!("\n{e}: the draft and the database disagree on\n");
                for f in failed {
                    out += &format!("  {f}\n");
                }
            }
        }
        if !self.other.is_empty() {
            out += "\nOther checks that failed\n";
            for f in &self.other {
                out += &format!("  {f}\n");
            }
        }
        let n = self.disagreements();
        out += &if n == 0 {
            "\nEvery entity that was checked does what the draft says.\n".to_string()
        } else {
            format!(
                "\n{n} check{} disagree. Each one is a place where the database does something the draft doesn't say, or the draft says something the database doesn't do.\n",
                if n == 1 { "" } else { "s" }
            )
        };
        out
    }
}

/// Why a draft can't be checked, or the tests that check it.
pub fn prepare(yaml: &str) -> Result<(Analysis, Vec<String>), String> {
    let diags = crate::report::validate(yaml);
    if crate::report::has_errors(&diags) {
        return Err(format!(
            "the draft doesn't validate, so there are no tests to run. Fix it first:\n{}",
            crate::report::human("the draft", &diags)
        ));
    }
    let (doc, _) = crate::load::load(yaml);
    if !doc.roles.is_empty() || !doc.holes.is_empty() {
        return Err(
            "the draft has roles or holes, which the copy of the database doesn't have. Check a draft with policies only."
                .into(),
        );
    }
    let names = doc.entities.iter().map(|e| e.name.clone()).collect();
    Ok((analyse(&doc), names))
}

/// Run the tests and judge them. `out` is psql's output for the generated file.
pub fn judge_run(
    a: &Analysis,
    entities: &[String],
    yaml: &str,
    out: &str,
) -> Result<Report, String> {
    let taps = parse_tap(out);
    if taps.is_empty() {
        let why = out
            .lines()
            .find(|l| l.contains("ERROR"))
            .unwrap_or("it printed no checks");
        return Err(format!("the tests didn't run: {why}"));
    }
    if let Some(t) = taps
        .iter()
        .find(|t| !t.ok && t.description == "test data: people")
    {
        return Err(format!(
            "the copy refused the test users: {}",
            t.died.clone().unwrap_or_default()
        ));
    }
    let names: Vec<&str> = entities.iter().map(String::as_str).collect();
    Ok(judge(a, &names, &narrower(yaml), &taps))
}

/// A schema-only dump of the listed schemas of the database at `url`, read with `pg_dump`.
/// It reads the database and never writes to it.
pub fn dump(url: &str, schemas: &[String]) -> Result<String, crate::migrations::Fail> {
    use crate::migrations::Fail;
    if let Some(s) = schemas
        .iter()
        .find(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
    {
        return Err(Fail(format!("'{s}' isn't a schema name")));
    }
    let mut cmd = std::process::Command::new("pg_dump");
    cmd.args([
        "--schema-only",
        "--no-owner",
        "--no-comments",
        "--dbname",
        url,
    ]);
    for s in schemas {
        cmd.arg(format!("--schema={s}"));
    }
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
        Ok(o) => Err(Fail(format!(
            "pg_dump refused: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ))),
        Err(_) => Err(Fail(
            "pg_dump isn't installed, and --db needs it to copy the schema. Install the PostgreSQL client tools, or use --migrations."
                .into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_lines_keep_what_the_database_gave() {
        let out = "1..3\nok 1 - test data: people\nnot ok 2 - anon can see Products a\n# Failed test 2: \"anon can see Products a\"\n#         have: 'denied'\n#         want: 'allowed'\nnot ok 3 - test data: Profiles a\n#     died: 23503: insert or update on table \"profiles\" violates foreign key\n#     died: second\n";
        let taps = parse_tap(out);
        assert_eq!(taps.len(), 3);
        assert!(taps[0].ok);
        assert_eq!(taps[1].have.as_deref(), Some("denied"));
        assert_eq!(taps[1].want.as_deref(), Some("allowed"));
        assert_eq!(
            taps[2].died.as_deref(),
            Some("insert or update on table \"profiles\" violates foreign key")
        );
    }

    #[test]
    fn a_draft_is_narrower_where_its_block_says_so() {
        let yaml = "# UNTRANSLATED policy outside: not in an entity\nsystem: App\n\nentities:\n  Orders:\n    # policies: 5 of 6 translated. The draft is NARROWER\n    fields:\n      id: { type: uuid, primary: true }\n  Items:\n    # row-level security is off here, so no rule\n    private: true\n  Products:\n    fields:\n      id: { type: uuid, primary: true }\n    # UNTRANSLATED index x: CREATE INDEX\n  Notes:\n    # UNTRANSLATED policy p: CREATE POLICY\npolicies:\n  p:\n    # UNTRANSLATED policy q: later\n";
        let n = narrower(yaml);
        let names: Vec<&str> = n.keys().map(String::as_str).collect();
        assert_eq!(names, ["Items", "Notes", "Orders"]);
    }

    #[test]
    fn entity_names_match_whole_words_only() {
        assert_eq!(word_at("alice can see Orders a", "Orders"), Some(14));
        assert_eq!(word_at("alice can see BigOrders a", "Orders"), None);
        assert_eq!(word_at("x Orders_old, Orders a", "Orders"), Some(14));
    }

    #[test]
    fn a_blocked_delete_names_the_entity_that_blocks_it() {
        let t = Tap {
            number: 9,
            ok: false,
            description: "alice can't delete Orders a: OrderItems a still points at it".into(),
            have: None,
            want: None,
            died: None,
        };
        assert_eq!(blocker(&["Orders", "OrderItems"], &t), Some("OrderItems"));
        assert_eq!(blocker(&["Orders"], &t), None);
    }
}
