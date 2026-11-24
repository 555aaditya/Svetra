//! The SQL schema (docs/compiler.md, SQL). Sections are emitted in a fixed order, and
//! entities and fields in file order, so the output is byte-identical for the same input.

use super::expr::{Ctx, sql as expr_sql};
use super::names::{lit, q, snake, table};
use crate::diag::{Code, Diagnostic};
use crate::expr;
use crate::model::*;
use std::collections::HashMap;

/// The full schema for a validated document: tables, foreign keys, indexes, access, policies and
/// holes, with every hole's body a stub.
pub fn schema(doc: &Document) -> String {
    schema_with(doc, &HashMap::new())
}

/// The first line of a hole body that hasn't been written yet. `compile` writes it to
/// `supabase/holes/<name>.sql` when that file doesn't exist.
pub fn stub(hole: &str) -> String {
    format!("  raise exception '{hole} is not written yet: edit supabase/holes/{hole}.sql';\n")
}

/// A problem with a hand-written hole body that would make the generated function invalid, found
/// without a database: Svetra puts the body between its own `begin` and `end`, so the body must be
/// statements, each ending with `;`. An agent that writes a whole function body, ending in `end`
/// with no `;`, leaves a stray `end` and a migration that doesn't parse (SVT032).
pub fn body_problem(hole: &str, body: &str) -> Option<Diagnostic> {
    let mut text = body.trim_end();
    // Trailing comments don't count: `-- note` lines and a final `/* note */`.
    loop {
        let before = text;
        if text.ends_with("*/")
            && let Some(start) = text.rfind("/*")
        {
            text = text[..start].trim_end();
        }
        if let Some(last) = text.lines().next_back()
            && last.trim_start().starts_with("--")
        {
            text = text[..text.len() - last.len()].trim_end();
        }
        if text == before {
            break;
        }
    }
    if text.is_empty() || text.ends_with(';') {
        return None;
    }
    let line = text.lines().count().max(1);
    let wrapped = text
        .split_whitespace()
        .next()
        .is_some_and(|w| w.eq_ignore_ascii_case("begin"));
    let (message, hint) = if wrapped {
        (
            format!(
                "The body of hole `{hole}` is a whole begin … end block with no `;` after it, so it breaks the function Svetra wraps around it: Svetra already adds the outer begin and end."
            ),
            "Delete the outer `begin` and `end` and keep only the statements, or end the block with `end;`.",
        )
    } else {
        (
            format!(
                "The body of hole `{hole}` doesn't end with `;`, so the function Svetra wraps around it won't parse."
            ),
            "End the last statement with `;`.",
        )
    };
    Some(Diagnostic {
        code: Code::Svt032,
        path: format!("holes.{hole}.body"),
        line,
        col: 0,
        message,
        hint: hint.into(),
    })
}

/// The full schema, with each hole's body taken from `bodies` (by hole name), or a stub.
pub fn schema_with(doc: &Document, bodies: &HashMap<String, String>) -> String {
    let entities: HashMap<&str, &Entity> =
        doc.entities.iter().map(|e| (e.name.as_str(), e)).collect();
    let tables: Vec<String> = doc
        .entities
        .iter()
        .map(|e| create_table(e, &entities))
        .collect();
    let mut out = format!("-- Tables\n{}", tables.join("\n"));
    let fks: Vec<String> = doc
        .entities
        .iter()
        .flat_map(|e| foreign_keys(e, &entities))
        .collect();
    if !fks.is_empty() {
        out += &format!("\n-- Foreign keys\n{}\n", fks.join("\n"));
    }
    let idx: Vec<String> = doc.entities.iter().flat_map(indexes).collect();
    if !idx.is_empty() {
        out += &format!("\n-- Indexes\n{}\n", idx.join("\n"));
    }
    if !doc.roles.is_empty() {
        out += &format!("\n-- Roles\n{}", roles(&doc.roles));
    }
    out += &format!(
        "\n-- Access\n{}\n",
        doc.entities
            .iter()
            .flat_map(access)
            .collect::<Vec<_>>()
            .join("\n")
    );
    let policies: Vec<String> = doc
        .policies
        .iter()
        .flat_map(|p| policy(p, &entities))
        .collect();
    if !policies.is_empty() {
        out += &format!("\n-- Policies\n{}\n", policies.join("\n\n"));
    }
    let guards: Vec<String> = doc
        .entities
        .iter()
        .filter_map(|e| guard(e, doc, &entities))
        .collect();
    if !guards.is_empty() {
        out += &format!(
            "\n-- Guards\ncreate schema if not exists \"svetra\";\n\n{}",
            guards.join("\n")
        );
    }
    if doc.entities.iter().any(|e| e.audit) || doc.holes.iter().any(|h| h.audit) {
        out += &format!("\n-- Audit\n{}", audit(doc));
    }
    if !doc.holes.is_empty() {
        let holes: Vec<String> = doc
            .holes
            .iter()
            .map(|h| hole(h, doc, &entities, bodies))
            .collect();
        let scheduled: Vec<String> = doc
            .holes
            .iter()
            .filter(|h| matches!(h.kind, HoleKind::Schedule(_)))
            .map(|h| lit(&job_name(&h.name)))
            .collect();
        // pg_cron is switched on only when a design schedules something. The stand-in has a
        // stub `cron` schema instead of the extension.
        let cron_on = if scheduled.is_empty() {
            String::new()
        } else {
            "do $$ begin if to_regnamespace('cron') is null then create extension pg_cron; end if; end $$;\n".to_string()
        };
        // Jobs Svetra made for holes that are no longer declared. `_` is a wildcard in `like`.
        let cleanup = if scheduled.is_empty() {
            String::new()
        } else {
            format!(
                "select cron.unschedule(jobid) from cron.job\n  where jobname like 'svetra\\_%' and jobname not in ({});\n",
                scheduled.join(", ")
            )
        };
        out += &format!(
            "\n-- Holes\ncreate schema if not exists \"svetra_holes\";\n{cron_on}\n{}{cleanup}",
            holes.join("\n")
        );
    }
    out
}

/// The audit log, its append-only trigger and one row trigger per audited entity
/// (docs/compiler.md, Audit).
fn audit(doc: &Document) -> String {
    let mut out = String::from(
        r#"create schema if not exists "svetra";
create table "svetra"."audit_log" (
  "id" bigint generated always as identity primary key,
  "at" timestamptz not null default now(),
  "actor" uuid,
  "kind" text not null check ("kind" in ('row', 'call')),
  "subject" text not null,
  "action" text not null check ("action" in ('insert', 'update', 'delete', 'call')),
  "old" jsonb,
  "new" jsonb
);
alter table "svetra"."audit_log" enable row level security;
revoke all on table "svetra"."audit_log" from public, anon, authenticated;

create function "svetra"."audit_frozen"() returns trigger language plpgsql as $svetra$
begin
  raise exception 'the audit log is append-only' using errcode = '42501';
end $svetra$;
create trigger "svetra_audit_frozen" before update or delete on "svetra"."audit_log"
  for each row execute function "svetra"."audit_frozen"();
create trigger "svetra_audit_frozen_truncate" before truncate on "svetra"."audit_log"
  for each statement execute function "svetra"."audit_frozen"();
"#,
    );
    if doc.entities.iter().any(|e| e.audit) {
        out += r#"
create function "svetra"."audit_row"() returns trigger language plpgsql
  security definer set search_path = '' as $svetra$
begin
  insert into "svetra"."audit_log" ("actor", "kind", "subject", "action", "old", "new")
  values ((select auth.uid()), 'row', tg_table_name, lower(tg_op),
    case when tg_op <> 'INSERT' then to_jsonb(old) end,
    case when tg_op <> 'DELETE' then to_jsonb(new) end);
  return null;
end $svetra$;
"#;
    }
    for e in doc.entities.iter().filter(|e| e.audit) {
        out += &format!(
            "create trigger \"svetra_audit\" after insert or update or delete on {}\n  for each row execute function \"svetra\".\"audit_row\"();\n",
            table(&e.name)
        );
    }
    out
}

/// The guard for fields an update may not change (docs/column-rules.md): a `before update` row
/// trigger that refuses, as SQLSTATE 42501, an update through the API that changes a field the
/// policies covering the person don't free. `None` when no policy on the entity has an `only`.
fn guard(e: &Entity, doc: &Document, entities: &HashMap<&str, &Entity>) -> Option<String> {
    let updates: Vec<&Policy> = doc
        .policies
        .iter()
        .filter(|p| p.on.name == e.name && p.allow.contains(&Action::Update))
        .collect();
    let limited = |p: &Policy, field: &str| {
        p.only
            .as_ref()
            .is_some_and(|only| !only.iter().any(|f| f.name == field))
    };
    let fields: Vec<&Field> = e
        .fields
        .iter()
        .filter(|f| !f.primary && updates.iter().any(|p| limited(p, &f.name)))
        .collect();
    if fields.is_empty() {
        return None;
    }
    let cx = Ctx {
        entity: e,
        entities,
        params: false,
        old: true,
        row: None,
    };
    // Whether a policy covers the person: their database role, and the policy's `when` on the old row.
    // A null counts as not covering, so a missing value can never free a field.
    let applies = |p: &Policy| {
        let role = match p.role {
            Role::Authenticated => "current_user = 'authenticated'",
            Role::Anon => "current_user = 'anon'",
            Role::Anyone => "current_user in ('anon', 'authenticated')",
        };
        let when = expr_sql(&expr::parse(&p.when).expect("validated expression"), &cx);
        format!("coalesce({role} and ({when}), false)")
    };
    let table_name = snake(&e.name);
    let mut body = String::new();
    for f in fields {
        // Locked by a restrictive policy that covers the person and leaves the field out, or
        // by every permissive policy that covers them (none leaves it free).
        let restrictive: Vec<String> = updates
            .iter()
            .filter(|p| p.restrictive && limited(p, &f.name))
            .map(|p| applies(p))
            .collect();
        let free: Vec<String> = updates
            .iter()
            .filter(|p| !p.restrictive && !limited(p, &f.name))
            .map(|p| applies(p))
            .collect();
        let mut locked = Vec::new();
        if !restrictive.is_empty() {
            locked.push(format!("({})", restrictive.join(" or ")));
        }
        locked.push(if free.is_empty() {
            "true".to_string()
        } else {
            format!("not ({})", free.join(" or "))
        });
        let message = lit(&format!("{table_name}: {} can't be changed here", f.name));
        body += &format!(
            "  if new.{c} is distinct from old.{c} and ({}) then\n    raise exception {message} using errcode = '42501';\n  end if;\n",
            locked.join(" or "),
            c = q(&f.name)
        );
    }
    let function = q(&format!("guard_{table_name}"));
    Some(format!(
        "create function \"svetra\".{function}() returns trigger language plpgsql as $svetra$\nbegin\n  -- Only people calling through the API are checked: the roles row-level security applies to.\n  if current_user not in ('anon', 'authenticated') then return new; end if;\n{body}  return new;\nend $svetra$;\ncreate trigger \"svetra_guard\" before update on {}\n  for each row execute function \"svetra\".{function}();\n",
        table(&e.name)
    ))
}

fn param_type(t: &ParamType, entities: &HashMap<&str, &Entity>) -> String {
    match t {
        ParamType::FieldOf { entity, field } => {
            let f = entities[entity.name.as_str()]
                .fields
                .iter()
                .find(|f| &f.name == field)
                .expect("validated hole input");
            column_type(&f.ty)
        }
        ParamType::Entity(e) => table(&e.name),
        ParamType::Type(t) => column_type(t),
    }
}

/// The pg_cron job of a scheduled hole.
pub fn job_name(hole: &str) -> String {
    format!("svetra_{hole}")
}

/// The Postgres trigger of a triggered hole for one event (`Insert`, `Update` or `Delete`).
pub fn trigger_name(hole: &str, event: Action) -> String {
    format!("svetra_{hole}_{}", event_word(event))
}

/// The SQL word of a trigger event, lower case.
pub fn event_word(event: Action) -> &'static str {
    match event {
        Action::Insert => "insert",
        Action::Update => "update",
        _ => "delete",
    }
}

/// The function a triggered hole's triggers call, which calls the body.
pub fn fire_name(hole: &str) -> String {
    format!("{hole}__fire")
}

/// One hole (docs/compiler.md, Holes): a role holding exactly the declared access, the body
/// function that runs as that role behind the `when` guard, and the function the API calls
/// (a scheduled hole has no such function: a pg_cron job calls the body instead).
fn hole(
    h: &Hole,
    doc: &Document,
    entities: &HashMap<&str, &Entity>,
    bodies: &HashMap<String, String>,
) -> String {
    let role = q(&format!("svetra_hole_{}", h.name));
    let mut out = vec![
        // Roles belong to the whole server and survive a database reset, so create it only once.
        format!(
            "do $$ begin create role {role} nologin; exception when duplicate_object then null; end $$;"
        ),
        // The migration's own role must be a member to hand the role a function.
        format!("grant {role} to current_user;"),
        format!("grant usage, create on schema \"svetra_holes\" to {role};"),
        format!("grant usage on schema \"public\" to {role};"),
    ];
    let declared = |list: &[NameRef], e: &Entity| list.iter().any(|r| r.name == e.name);
    for e in &doc.entities {
        let (privileges, policy) = if declared(&h.writes, e) {
            (
                "select, insert, update, delete",
                "for all to {role} using (true) with check (true)",
            )
        } else if declared(&h.reads, e) {
            ("select", "for select to {role} using (true)")
        } else {
            continue;
        };
        out.push(format!(
            "grant {privileges} on table {} to {role};",
            table(&e.name)
        ));
        out.push(format!(
            "create policy {role} on {} {};",
            table(&e.name),
            policy.replace("{role}", &role)
        ));
    }
    if !doc.roles.is_empty() {
        out.push(format!("grant usage on schema \"svetra\" to {role};"));
        out.push(format!(
            "grant execute on function \"svetra\".\"has_role\"(text, uuid) to {role};"
        ));
    }
    // A hole that grants roles may add and remove grants of those roles only: the policy keeps
    // every other role, such as admin, out of its reach.
    if !h.grants.is_empty() {
        let listed: Vec<String> = h.grants.iter().map(|g| lit(&g.name)).collect();
        let only = format!("\"role\" in ({})", listed.join(", "));
        out.push(format!(
            "grant select, insert, delete on table \"svetra\".\"role_grants\" to {role};"
        ));
        out.push(format!(
            "create policy {role} on \"svetra\".\"role_grants\" for all to {role} using ({only}) with check ({only});"
        ));
    }
    let returns = h
        .output
        .as_ref()
        .map_or("void".into(), |t| param_type(t, entities));
    // A triggered hole takes the changed row, before and after, as its inputs.
    let row_type = match &h.kind {
        HoleKind::Trigger(t) => Some(table(&t.on.name)),
        _ => None,
    };
    let inputs: Vec<(String, String)> = match &row_type {
        Some(row) => vec![("old".into(), row.clone()), ("new".into(), row.clone())],
        None => h
            .input
            .iter()
            .map(|p| (p.name.clone(), param_type(&p.ty, entities)))
            .collect(),
    };
    let types: Vec<&str> = inputs.iter().map(|(_, t)| t.as_str()).collect();
    let inner_sig = format!(
        "\"svetra_holes\".{}(uuid{})",
        q(&h.name),
        types.iter().map(|t| format!(", {t}")).collect::<String>()
    );
    let inner_params: Vec<String> = std::iter::once("p_caller uuid".to_string())
        .chain(inputs.iter().map(|(n, t)| format!("p_{n} {t}")))
        .collect();
    let fields = crate::typeck::hole_entity(h, entities);
    let cx = Ctx {
        entity: &fields,
        entities,
        params: true,
        old: false,
        row: None,
    };
    // A trigger's `when` is the trigger's own WHEN clause, so the body has no guard.
    let guard = h
        .when
        .as_deref()
        .filter(|_| row_type.is_none())
        .map(|w| expr_sql(&expr::parse(w).expect("validated expression"), &cx));
    // Bad input is refused after who-may-call, with `using message` so a `%` in the condition is
    // text and not a placeholder.
    let requirements: String = h
        .require
        .iter()
        .map(|r| {
            format!(
                "  if not coalesce({}, false) then\n    raise exception using message = {}, errcode = '22023';\n  end if;\n",
                expr_sql(&expr::parse(r).expect("validated expression"), &cx),
                lit(&format!("{}: {r}", h.name))
            )
        })
        .collect();
    let body = bodies
        .get(&h.name)
        .cloned()
        .unwrap_or_else(|| stub(&h.name));
    out.push(format!(
        "create function \"svetra_holes\".{}({})\n  returns {returns} language plpgsql security definer set search_path = ''\n  as $svetra$\nbegin\n{}{}{}{}end\n$svetra$;",
        q(&h.name),
        inner_params.join(", "),
        guard.map_or(String::new(), |g| format!(
            "  if not coalesce({g}, false) then\n    raise exception 'not allowed to call {}' using errcode = '42501';\n  end if;\n",
            h.name
        )),
        requirements,
        body,
        if body.ends_with('\n') { "" } else { "\n" }
    ));
    out.push(format!("alter function {inner_sig} owner to {role};"));
    // Supabase grants execute on new functions to anon and authenticated directly, not only
    // through PUBLIC, so both are revoked by name.
    out.push(format!(
        "revoke all on function {inner_sig} from public, anon, authenticated;"
    ));
    out.push(format!(
        "grant execute on function {inner_sig} to current_user;"
    ));
    if h.kind != HoleKind::Function {
        // No function in `public`: nothing in the API can run it. Supabase also lets
        // service_role run new functions, and the stand-in has no such role.
        out.push(revoke_service_role(&inner_sig));
    }
    if let HoleKind::Schedule(sched) = &h.kind {
        out.push(format!(
            "select cron.schedule({}, {},\n  $job$select {}(null)\n    where pg_try_advisory_xact_lock(hashtext({}))$job$);",
            lit(&job_name(&h.name)),
            lit(&sched.cron),
            inner_sig.trim_end_matches("(uuid)"),
            lit(&job_name(&h.name)),
        ));
        return out.join("\n") + "\n";
    }
    if let HoleKind::Trigger(t) = &h.kind {
        out.extend(triggers(h, t, entities));
        return out.join("\n") + "\n";
    }
    let outer_sig = format!("\"public\".{}({})", q(&h.name), types.join(", "));
    let outer_params: Vec<String> = inputs.iter().map(|(n, t)| format!("{n} {t}")).collect();
    let args: String = inputs.iter().map(|(n, _)| format!(", {n}")).collect();
    let call = format!("\"svetra_holes\".{}((select auth.uid()){args})", q(&h.name));
    let run = if h.output.is_some() {
        format!("return {call};")
    } else {
        format!("perform {call};")
    };
    // The record is written here, by the migration's role, so the hole's own role never touches
    // the log. A guard that refuses rolls it back with everything else.
    let log = if h.audit {
        let inputs = if inputs.is_empty() {
            "'{}'::jsonb".to_string()
        } else {
            let pairs: Vec<String> = inputs
                .iter()
                .map(|(n, _)| format!("{}, {n}", lit(n)))
                .collect();
            format!("jsonb_build_object({})", pairs.join(", "))
        };
        format!(
            " insert into \"svetra\".\"audit_log\" (\"actor\", \"kind\", \"subject\", \"action\", \"new\") values ((select auth.uid()), 'call', {}, 'call', {inputs});",
            lit(&h.name)
        )
    } else {
        String::new()
    };
    out.push(format!(
        "create function \"public\".{}({})\n  returns {returns} language plpgsql security definer set search_path = ''\n  as $svetra$ begin{log} {run} end $svetra$;",
        q(&h.name),
        outer_params.join(", ")
    ));
    out.push(format!(
        "revoke all on function {outer_sig} from public, anon, authenticated;"
    ));
    let callers = match h.role {
        Role::Authenticated => "authenticated",
        Role::Anon => "anon",
        Role::Anyone => "anon, authenticated",
    };
    out.push(format!(
        "grant execute on function {outer_sig} to {callers};"
    ));
    out.join("\n") + "\n"
}

fn revoke_service_role(sig: &str) -> String {
    format!(
        "do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on function {sig} from service_role; end if; end $$;"
    )
}

/// The condition of a triggered hole's trigger for one event, as SQL for its `WHEN` clause: bare
/// fields are the row after the change (`new`, or `old` for a delete) and `old.<field>` the row
/// before. `None` when the hole has no `when`.
pub fn trigger_when(
    h: &Hole,
    t: &Trigger,
    event: Action,
    entities: &HashMap<&str, &Entity>,
) -> Option<String> {
    let on = entities
        .get(t.on.name.as_str())
        .expect("validated trigger entity");
    let cx = Ctx {
        entity: on,
        entities,
        params: false,
        old: false,
        row: Some(if event == Action::Delete {
            "old"
        } else {
            "new"
        }),
    };
    h.when
        .as_deref()
        .map(|w| expr_sql(&expr::parse(w).expect("validated expression"), &cx))
}

/// A triggered hole's trigger function and its triggers, one per event (docs/compiler.md,
/// Triggered holes). The trigger function is `security definer` so that whoever made the change
/// fires it without being able to run anything in `svetra_holes`.
fn triggers(h: &Hole, t: &Trigger, entities: &HashMap<&str, &Entity>) -> Vec<String> {
    let fire = format!("\"svetra_holes\".{}", q(&fire_name(&h.name)));
    let mut out = vec![
        format!(
            "create function {fire}()\n  returns trigger language plpgsql security definer set search_path = ''\n  as $svetra$\nbegin\n  if pg_trigger_depth() > 3 then\n    raise exception '{} is part of a trigger loop' using errcode = '54001';\n  end if;\n  perform {}((select auth.uid()),\n    case when tg_op = 'INSERT' then null else old end,\n    case when tg_op = 'DELETE' then null else new end);\n  return null;\nend\n$svetra$;",
            h.name,
            format!("\"svetra_holes\".{}", q(&h.name))
        ),
        format!("revoke all on function {fire}() from public, anon, authenticated;"),
        revoke_service_role(&format!("{fire}()")),
    ];
    for event in &t.events {
        let when = trigger_when(h, t, *event, entities);
        out.push(format!(
            "create trigger {} after {} on {}\n  for each row{}\n  execute function {fire}();",
            q(&trigger_name(&h.name, *event)),
            event_word(*event),
            table(&t.on.name),
            when.map_or(String::new(), |w| format!(" when ({w})"))
        ));
    }
    out
}

/// Row-level security on every table, so a table with no policy denies everyone.
/// Private entities also lose every grant, so the generated API can't reach them at all.
fn access(e: &Entity) -> Vec<String> {
    let mut out = vec![format!(
        "alter table {} enable row level security;",
        table(&e.name)
    )];
    if e.private {
        out.push(format!(
            "revoke all on table {} from anon, authenticated;",
            table(&e.name)
        ));
    }
    out
}

/// One `create policy` per action (docs/compiler.md, Policies).
fn policy(p: &Policy, entities: &HashMap<&str, &Entity>) -> Vec<String> {
    let e = entities
        .get(p.on.name.as_str())
        .expect("validated policy target");
    let cx = Ctx {
        entity: e,
        entities,
        params: false,
        old: false,
        row: None,
    };
    let cond = expr_sql(&expr::parse(&p.when).expect("validated expression"), &cx);
    // The condition on the row after an update: `check`, or `when` when there is none.
    let after = p.check.as_ref().map_or_else(
        || cond.clone(),
        |c| expr_sql(&expr::parse(c).expect("validated expression"), &cx),
    );
    let kind = if p.restrictive { " as restrictive" } else { "" };
    let role = match p.role {
        Role::Authenticated => "authenticated",
        Role::Anon => "anon",
        Role::Anyone => "anon, authenticated",
    };
    p.allow
        .iter()
        .map(|a| {
            let (action, clauses) = match a {
                Action::Select => ("select", format!("using ({cond})")),
                Action::Delete => ("delete", format!("using ({cond})")),
                Action::Insert => ("insert", format!("with check ({cond})")),
                Action::Update => ("update", format!("using ({cond})\n  with check ({after})")),
            };
            let name = if p.allow.len() == 1 {
                p.name.clone()
            } else {
                format!("{}_{action}", p.name)
            };
            format!(
                "create policy {} on {}{kind}\n  for {action} to {role}\n  {clauses};",
                q(&name),
                table(&e.name)
            )
        })
        .collect()
}

fn column_type(t: &FieldType) -> String {
    match t {
        FieldType::Text | FieldType::Enum(_) => "text".into(),
        FieldType::Int => "bigint".into(),
        FieldType::Float => "double precision".into(),
        FieldType::Bool => "boolean".into(),
        FieldType::Uuid | FieldType::Ref(_) | FieldType::User => "uuid".into(),
        FieldType::Timestamp => "timestamptz".into(),
        FieldType::Date => "date".into(),
        FieldType::Decimal { precision, scale } | FieldType::Money { precision, scale } => {
            format!("numeric({precision}, {scale})")
        }
        FieldType::Json => "jsonb".into(),
    }
}

fn literal(l: &Literal) -> String {
    match l {
        Literal::Bool(b) => b.to_string(),
        Literal::Int(i) => i.to_string(),
        Literal::Float(f) => f.to_string(),
        Literal::Str(s) => lit(s),
    }
}

fn column(f: &Field) -> String {
    let mut col = format!("{} {}", q(&f.name), column_type(&f.ty));
    if f.primary {
        col += " primary key";
        if f.ty == FieldType::Uuid {
            col += " default gen_random_uuid()";
        }
    } else if f.required {
        col += " not null";
    }
    if let Some(d) = &f.default {
        col += &format!(" default {}", literal(d));
    } else if f.auto_now {
        col += " default now()";
    }
    col
}

fn create_table(e: &Entity, entities: &HashMap<&str, &Entity>) -> String {
    let t = snake(&e.name);
    let mut lines: Vec<String> = e.fields.iter().map(column).collect();
    for f in &e.fields {
        if let FieldType::Enum(values) = &f.ty {
            let list: Vec<String> = values.iter().map(|v| lit(v)).collect();
            lines.push(format!(
                "constraint {} check ({} in ({}))",
                q(&format!("{t}_{}_values", f.name)),
                q(&f.name),
                list.join(", ")
            ));
        }
    }
    let cx = Ctx {
        entity: e,
        entities,
        params: false,
        old: false,
        row: None,
    };
    for c in &e.constraints {
        let parsed = expr::parse(&c.expr).expect("validated expression");
        lines.push(format!(
            "constraint {} check ({})",
            q(&c.name),
            expr_sql(&parsed, &cx)
        ));
    }
    let single = e
        .fields
        .iter()
        .filter(|f| f.unique && !f.primary)
        .map(|f| vec![f.name.as_str()]);
    let lists = e
        .unique
        .iter()
        .map(|l| l.fields.iter().map(|x| x.name.as_str()).collect::<Vec<_>>());
    for cols in single.chain(lists) {
        let quoted: Vec<String> = cols.iter().map(|c| q(c)).collect();
        lines.push(format!(
            "constraint {} unique ({})",
            q(&format!("{t}_{}_key", cols.join("_"))),
            quoted.join(", ")
        ));
    }
    format!(
        "create table {} (\n  {}\n);\n",
        table(&e.name),
        lines.join(",\n  ")
    )
}

fn foreign_keys(e: &Entity, entities: &HashMap<&str, &Entity>) -> Vec<String> {
    let t = snake(&e.name);
    e.fields
        .iter()
        .filter_map(|f| {
            let (target, action) = match &f.ty {
                FieldType::Ref(r) => {
                    let target = entities.get(r.name.as_str()).expect("validated reference");
                    let pk = target
                        .fields
                        .iter()
                        .find(|x| x.primary)
                        .expect("validated primary key");
                    let action = match f.on_delete {
                        OnDelete::Restrict => "",
                        OnDelete::Cascade => " on delete cascade",
                        OnDelete::SetNull => " on delete set null",
                    };
                    (format!("{} ({})", table(&target.name), q(&pk.name)), action)
                }
                FieldType::User => ("\"auth\".\"users\" (\"id\")".into(), ""),
                _ => return None,
            };
            let name = q(&format!("{t}_{}_fkey", f.name));
            Some(format!(
                "alter table {} add constraint {name} foreign key ({}) references {target}{action};",
                table(&e.name),
                q(&f.name)
            ))
        })
        .collect()
}

fn index(e: &Entity, cols: &[&str]) -> String {
    let quoted: Vec<String> = cols.iter().map(|c| q(c)).collect();
    let name = q(&format!("{}_{}_idx", snake(&e.name), cols.join("_")));
    format!(
        "create index {name} on {} ({});",
        table(&e.name),
        quoted.join(", ")
    )
}

/// Declared indexes, then one for each reference field nothing already leads with (SVT010).
fn indexes(e: &Entity) -> Vec<String> {
    let declared = e
        .indexes
        .iter()
        .map(|l| l.fields.iter().map(|x| x.name.as_str()).collect::<Vec<_>>());
    let mut out: Vec<String> = declared.map(|cols| index(e, &cols)).collect();
    for f in &e.fields {
        let leads = |l: &FieldList| l.fields.first().is_some_and(|x| x.name == f.name);
        let covered = f.unique || f.primary || e.unique.iter().chain(&e.indexes).any(leads);
        if matches!(f.ty, FieldType::Ref(_) | FieldType::User) && !covered {
            out.push(index(e, &[f.name.as_str()]));
        }
    }
    out
}

/// The private role store and the `has_role` check (docs/compiler.md, Roles). The table is
/// unreachable from the API, and `has_role` is `security definer` with a fixed search path, so
/// policies that call it never run other tables' policies.
fn roles(roles: &[RoleDef]) -> String {
    let names: Vec<String> = roles.iter().map(|r| lit(&r.name)).collect();
    let mfa: Vec<String> = roles
        .iter()
        .filter(|r| r.mfa)
        .map(|r| lit(&r.name))
        .collect();
    // Roles with `mfa: true` count only once the session is verified with a second factor.
    let mfa = if mfa.is_empty() {
        String::new()
    } else {
        format!(
            "\n      and (g.\"role\" not in ({}) or coalesce((select auth.jwt() ->> 'aal'), '') = 'aal2')",
            mfa.join(", ")
        )
    };
    format!(
        r#"create schema if not exists "svetra";
create table "svetra"."role_grants" (
  "user_id" uuid not null references "auth"."users" ("id") on delete cascade,
  "role" text not null check ("role" in ({})),
  "scope_id" uuid,
  constraint "role_grants_key" unique nulls not distinct ("user_id", "role", "scope_id")
);
alter table "svetra"."role_grants" enable row level security;

create function "svetra"."has_role"(p_role text, p_scope uuid default null)
  returns boolean language sql stable security definer set search_path = ''
  as $$ select exists (
    select 1 from "svetra"."role_grants" g
    where g."user_id" = (select auth.uid()) and g."role" = p_role and g."scope_id" is not distinct from p_scope{mfa}
  ) $$;
revoke all on function "svetra"."has_role"(text, uuid) from public;
grant usage on schema "svetra" to anon, authenticated;
grant execute on function "svetra"."has_role"(text, uuid) to anon, authenticated;
"#,
        names.join(", ")
    )
}
