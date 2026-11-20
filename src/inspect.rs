//! `svetra check` (docs/check.md): read an existing database's access configuration and report,
//! in plain English, where data is open to people who shouldn't reach it.

use crate::compile::names::lit;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl Severity {
    pub fn word(self) -> &'static str {
        match self {
            Severity::Critical => "critical",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub kind: &'static str,
    pub severity: Severity,
    /// `schema.name`, with a function's arguments.
    pub object: String,
    pub message: String,
    pub fix: String,
}

/// The read-only catalog query: one JSON document (docs/check.md, What it reads).
pub fn query(schemas: &[String]) -> String {
    let list: Vec<String> = schemas.iter().map(|s| lit(s)).collect();
    let list = list.join(", ");
    let table_priv = |role: &str, p: &str| {
        format!(
            "case when to_regrole('{role}') is null then false else has_table_privilege('{role}', c.oid, '{p}') end"
        )
    };
    let roles = |obj: &dyn Fn(&str, &str) -> String| {
        ["anon", "authenticated"]
            .iter()
            .map(|r| {
                format!(
                    "'{r}', json_build_object('select', {}, 'insert', {}, 'update', {}, 'delete', {})",
                    obj(r, "select"),
                    obj(r, "insert"),
                    obj(r, "update"),
                    obj(r, "delete")
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let fn_exec = |role: &str| {
        format!(
            "case when to_regrole('{role}') is null then false else has_function_privilege('{role}', p.oid, 'execute') end"
        )
    };
    format!(
        "select json_build_object(\
'tables', (select coalesce(json_agg(json_build_object('schema', n.nspname, 'name', c.relname, 'rls', c.relrowsecurity, {roles}) order by n.nspname, c.relname), '[]') \
from pg_class c join pg_namespace n on n.oid = c.relnamespace where c.relkind in ('r', 'p') and n.nspname in ({list})), \
'policies', (select coalesce(json_agg(json_build_object('schema', schemaname, 'table', tablename, 'name', policyname, 'permissive', permissive, 'roles', roles, 'cmd', cmd, 'using', qual, 'check', with_check) order by schemaname, tablename, policyname), '[]') \
from pg_policies where schemaname in ({list})), \
'views', (select coalesce(json_agg(json_build_object('schema', n.nspname, 'name', c.relname, \
'security_invoker', coalesce((select option_value from pg_options_to_table(c.reloptions) where option_name = 'security_invoker') in ('true', 'on', '1'), false), \
'anon_select', {va}, 'authenticated_select', {vu}, 'definition', pg_get_viewdef(c.oid)) order by n.nspname, c.relname), '[]') \
from pg_class c join pg_namespace n on n.oid = c.relnamespace where c.relkind in ('v', 'm') and n.nspname in ({list})), \
'functions', (select coalesce(json_agg(json_build_object('schema', n.nspname, 'name', p.proname, 'args', pg_get_function_identity_arguments(p.oid), \
'security_definer', p.prosecdef, 'search_path_set', coalesce(exists (select 1 from unnest(p.proconfig) x where x like 'search_path=%'), false), \
'anon_execute', {fa}, 'authenticated_execute', {fu}) order by n.nspname, p.proname, pg_get_function_identity_arguments(p.oid)), '[]') \
from pg_proc p join pg_namespace n on n.oid = p.pronamespace where p.prokind = 'f' and n.nspname in ({list})))",
        roles = roles(&table_priv),
        va = table_priv("anon", "select"),
        vu = table_priv("authenticated", "select"),
        fa = fn_exec("anon"),
        fu = fn_exec("authenticated"),
    )
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_default()
}

fn b(v: &Value, k: &str) -> bool {
    v[k].as_bool().unwrap_or(false)
}

/// Is a deparsed condition just `true`?
fn always(cond: &Value) -> bool {
    cond.as_str().is_some_and(|c| {
        c.trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim()
            == "true"
    })
}

fn roles(p: &Value) -> Vec<String> {
    match &p["roles"] {
        Value::Array(a) => a
            .iter()
            .filter_map(|r| r.as_str().map(String::from))
            .collect(),
        // `pg_policies.roles` is a name[]; some clients render it as "{anon,authenticated}".
        Value::String(t) => t
            .trim_matches(|c| c == '{' || c == '}')
            .split(',')
            .map(|r| r.trim().trim_matches('"').to_string())
            .filter(|r| !r.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Every finding in a snapshot, most severe first, then by object (docs/check.md, Findings).
pub fn findings(snapshot: &Value) -> Vec<Finding> {
    let mut out = Vec::new();
    let empty = Vec::new();
    let list = |k: &str| snapshot[k].as_array().unwrap_or(&empty).clone();
    let policies = list("policies");
    for t in list("tables") {
        let object = format!("{}.{}", s(&t, "schema"), s(&t, "name"));
        let can = |role: &str, p: &str| t[role][p].as_bool().unwrap_or(false);
        let any = |role: &str| {
            ["select", "insert", "update", "delete"]
                .iter()
                .any(|p| can(role, p))
        };
        let reachable = any("anon") || any("authenticated");
        if !reachable {
            continue;
        }
        let mine: Vec<&Value> = policies
            .iter()
            .filter(|p| s(p, "schema") == s(&t, "schema") && s(p, "table") == s(&t, "name"))
            .collect();
        if !b(&t, "rls") {
            let who = if any("anon") {
                "Anyone on the internet"
            } else {
                "Every signed-in person"
            };
            let role = if any("anon") { "anon" } else { "authenticated" };
            let verbs: Vec<&str> = [
                ("select", "read"),
                ("insert", "add"),
                ("update", "change"),
                ("delete", "delete"),
            ]
            .iter()
            .filter(|(p, _)| can(role, p))
            .map(|(_, w)| *w)
            .collect();
            out.push(Finding {
                kind: "rls_off",
                severity: Severity::Critical,
                message: format!(
                    "{who} can {} every row of {object}: row-level security is off.",
                    crate::understand::words::list(&verbs.iter().map(|v| v.to_string()).collect::<Vec<_>>(), "and")
                ),
                fix: format!(
                    "alter table {object} enable row level security; then add policies, or describe the table in svetra.yaml and let Svetra write and test them."
                ),
                object,
            });
            continue;
        }
        if mine.is_empty() {
            out.push(Finding {
                kind: "no_policy",
                severity: Severity::Info,
                message: format!("Nobody can reach {object} through the API: row-level security is on with no policies. Intended?"),
                fix: "If the app should reach it, add policies; if not, revoke the API's privileges on it.".into(),
                object,
            });
        }
    }
    for p in &policies {
        if s(p, "permissive") != "PERMISSIVE" {
            continue;
        }
        let object = format!("{}.{}", s(p, "schema"), s(p, "table"));
        let name = s(p, "name");
        let rs = roles(p);
        let anyone = rs.iter().any(|r| r == "anon" || r == "public");
        let signed_in = rs.iter().any(|r| r == "authenticated");
        let cmd = s(p, "cmd");
        // A policy named for the backend, with no `to service_role`, applies to everyone. The
        // service role skips row-level security anyway, so only the wording and the fix change.
        let lower = name.to_lowercase();
        let for_backend = ["service", "system", "backend"]
            .iter()
            .any(|w| lower.contains(w))
            && !rs.iter().any(|r| r == "service_role");
        let (backend_note, backend_fix) = if for_backend {
            (
                " The policy's name says it is for the service role, but it has no `to service_role`, so it applies to everyone.",
                Some(
                    "Delete the policy: the service role skips row-level security and doesn't need one, or add `to service_role` to it.",
                ),
            )
        } else {
            ("", None)
        };
        let reads = matches!(cmd, "SELECT" | "ALL");
        let writes = matches!(cmd, "INSERT" | "UPDATE" | "DELETE" | "ALL");
        // The condition that decides: `using` for reads, updates and deletes; `check` for inserts.
        let open_write = writes
            && match cmd {
                "INSERT" => always(&p["check"]),
                _ => always(&p["using"]) && (p["check"].is_null() || always(&p["check"])),
            };
        // An open INSERT lets people add rows, not change the ones that exist: often a sign-up
        // or contact form, so it is a warning, and it is the only open write that is not critical.
        let adds_only = cmd == "INSERT";
        if anyone && open_write && !adds_only {
            out.push(Finding {
                kind: "open_write",
                severity: Severity::Critical,
                message: format!("Anyone on the internet can change every row of {object} (policy {name}).{backend_note}"),
                fix: backend_fix.unwrap_or("Replace `true` with a condition on who owns the row, such as owner = auth.uid().").into(),
                object: object.clone(),
            });
        } else if anyone && open_write {
            out.push(Finding {
                kind: "open_insert",
                severity: Severity::Warning,
                message: format!(
                    "Anyone on the internet can add rows to {object} (policy {name}). Fine for a sign-up or contact form; a problem otherwise."
                ),
                fix: "Limit it to signed-in people, or to the service role, if only your app should add rows.".into(),
                object: object.clone(),
            });
        } else if signed_in && !anyone && open_write && !adds_only {
            out.push(Finding {
                kind: "signed_in_write",
                severity: Severity::Warning,
                message: format!(
                    "Every signed-in person can change every row of {object} (policy {name}).{backend_note}"
                ),
                fix: backend_fix.unwrap_or("Limit it to the row's owner or a role, such as owner = auth.uid().").into(),
                object: object.clone(),
            });
        }
        if anyone && reads && always(&p["using"]) {
            out.push(Finding {
                kind: "open_read",
                severity: Severity::Warning,
                message: format!("Anyone on the internet can read every row of {object} (policy {name}). Fine for public data; a leak otherwise.{backend_note}"),
                fix: backend_fix.unwrap_or("If the rows aren't public, replace `true` with a condition on who may see them.").into(),
                object: object.clone(),
            });
        }
        let text = format!("{} {}", s(p, "using"), s(p, "check"));
        if text.contains("user_metadata") || text.contains("raw_user_meta_data") {
            out.push(Finding {
                kind: "user_metadata",
                severity: Severity::Warning,
                message: format!("Policy {name} on {object} trusts user_metadata, which every person can change about themselves."),
                fix: "Decide roles from a table people can't write, such as Svetra's roles, or app_metadata.".into(),
                object: object.clone(),
            });
        }
    }
    for v in list("views") {
        let object = format!("{}.{}", s(&v, "schema"), s(&v, "name"));
        if b(&v, "security_invoker") || !(b(&v, "anon_select") || b(&v, "authenticated_select")) {
            continue;
        }
        let users = s(&v, "definition").contains("auth.users");
        out.push(Finding {
            kind: "view_bypass",
            severity: if users { Severity::Critical } else { Severity::Warning },
            message: if users {
                format!("{object} exposes auth.users to the API, with its owner's rights: people's emails and sign-in details may be readable.")
            } else {
                format!("{object} runs with its owner's rights, so it skips the row-level security of the tables it reads.")
            },
            fix: format!("alter view {object} set (security_invoker = true); or revoke select on it from anon and authenticated."),
            object,
        });
    }
    for f in list("functions") {
        if !b(&f, "security_definer") || !(b(&f, "anon_execute") || b(&f, "authenticated_execute"))
        {
            continue;
        }
        let object = format!("{}.{}({})", s(&f, "schema"), s(&f, "name"), s(&f, "args"));
        let anon = b(&f, "anon_execute");
        out.push(Finding {
            kind: "definer_function",
            severity: if anon { Severity::Warning } else { Severity::Info },
            message: format!(
                "{} can call {object}, which runs with its owner's rights. Check it validates the caller.",
                if anon { "Anyone on the internet" } else { "Every signed-in person" }
            ),
            fix: "Revoke execute from roles that shouldn't call it, and check auth.uid() inside it.".into(),
            object: object.clone(),
        });
        if !b(&f, "search_path_set") {
            out.push(Finding {
                kind: "mutable_search_path",
                severity: Severity::Warning,
                message: format!(
                    "{object} has no fixed search_path, so a caller can make it run their own code."
                ),
                fix: format!("alter function {object} set search_path = '';"),
                object,
            });
        }
    }
    out.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then(a.object.cmp(&b.object))
            .then(a.kind.cmp(b.kind))
    });
    out
}

pub fn json(findings: &[Finding]) -> Value {
    let count = |sev: Severity| findings.iter().filter(|f| f.severity == sev).count();
    json!({
        "findings": findings.iter().map(|f| json!({
            "kind": f.kind, "severity": f.severity.word(), "object": f.object, "message": f.message, "fix": f.fix,
        })).collect::<Vec<_>>(),
        "counts": { "critical": count(Severity::Critical), "warning": count(Severity::Warning), "info": count(Severity::Info) },
    })
}

pub fn human(findings: &[Finding], schemas: &[String], source: &str) -> String {
    let count = |sev: Severity| findings.iter().filter(|f| f.severity == sev).count();
    let (c, w, i) = (
        count(Severity::Critical),
        count(Severity::Warning),
        count(Severity::Info),
    );
    if findings.is_empty() {
        return format!(
            "svetra check: nothing found in {} ({source}).\n",
            schemas.join(", ")
        );
    }
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    let mut out = format!(
        "svetra check: {}, {}, {} in {} ({source})\n",
        plural(c, "critical"),
        plural(w, "warning"),
        plural(i, "note"),
        schemas.join(", ")
    )
    .replace("criticals", "critical");
    for f in findings {
        out += &format!(
            "\n  {:<9} {}\n  {:<9} {}\n  {:<9} fix: {}\n",
            f.severity.word(),
            f.object,
            "",
            f.message,
            "",
            f.fix
        );
    }
    out
}

/// A connection string with any password replaced, for printing.
pub fn redact(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.into();
    };
    match rest.split_once('@') {
        Some((creds, host)) => {
            let user = creds.split(':').next().unwrap_or_default();
            format!("{scheme}://{user}:***@{host}")
        }
        None => url.into(),
    }
}

/// The `svetra check --migrations` report (docs/check.md): the usual findings, one line about how
/// much of the migrations could be read, and the ones that couldn't.
pub fn human_migrations(
    findings: &[Finding],
    schemas: &[String],
    o: &crate::migrations::Outcome,
) -> String {
    let plural = |n: usize| format!("{n} migration{}", if n == 1 { "" } else { "s" });
    let mut out = if o.applied == 0 {
        if o.files == 1 {
            "svetra check: the migration could not be applied, so nothing was checked.\n"
                .to_string()
        } else {
            format!(
                "svetra check: none of the {} migrations could be applied, so nothing was checked.\n",
                o.files
            )
        }
    } else {
        human(
            findings,
            schemas,
            &format!("{}, {} applied", plural(o.files), o.applied),
        )
    };
    if !o.failed.is_empty() {
        let width = o.failed.iter().map(|(f, _)| f.len()).max().unwrap_or(0);
        out += &format!(
            "\n{} could not be applied{}:\n",
            plural(o.failed.len()),
            if o.applied == 0 {
                ""
            } else {
                ", so the findings above may be incomplete"
            }
        );
        for (file, error) in &o.failed {
            out += &format!("  {file:<width$}  {error}\n");
        }
    }
    out += "\nThis reads your migrations, not your live database. A table fixed in the dashboard, or made\noutside the migrations, isn't seen.\n";
    out
}

/// `--json` for `--migrations`: the usual document plus what the migrations did.
pub fn json_migrations(findings: &[Finding], o: &crate::migrations::Outcome) -> Value {
    let mut v = json(findings);
    v["migrations"] = json!({
        "files": o.files,
        "applied": o.applied,
        "failed": o.failed.iter().map(|(f, e)| json!({ "file": f, "error": e })).collect::<Vec<_>>(),
    });
    v
}
