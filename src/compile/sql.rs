//! The SQL schema (docs/compiler.md, SQL). Sections are emitted in a fixed order, and
//! entities and fields in file order, so the output is byte-identical for the same input.

use super::expr::{Ctx, sql as expr_sql};
use super::names::{lit, q, snake, table};
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
    if !doc.holes.is_empty() {
        let holes: Vec<String> = doc
            .holes
            .iter()
            .map(|h| hole(h, doc, &entities, bodies))
            .collect();
        out += &format!(
            "\n-- Holes\ncreate schema if not exists \"svetra_holes\";\n\n{}",
            holes.join("\n")
        );
    }
    out
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

/// One hole (docs/compiler.md, Holes): a role holding exactly the declared access, the body
/// function that runs as that role behind the `when` guard, and the function the API calls.
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
    let returns = h
        .output
        .as_ref()
        .map_or("void".into(), |t| param_type(t, entities));
    let inputs: Vec<(String, String)> = h
        .input
        .iter()
        .map(|p| (p.name.clone(), param_type(&p.ty, entities)))
        .collect();
    let types: Vec<&str> = inputs.iter().map(|(_, t)| t.as_str()).collect();
    let inner_sig = format!(
        "\"svetra_holes\".{}(uuid{})",
        q(&h.name),
        types.iter().map(|t| format!(", {t}")).collect::<String>()
    );
    let inner_params: Vec<String> = std::iter::once("p_caller uuid".to_string())
        .chain(inputs.iter().map(|(n, t)| format!("p_{n} {t}")))
        .collect();
    let guard = h.when.as_deref().map(|w| {
        let fields = crate::typeck::hole_entity(h, entities);
        let cx = Ctx {
            entity: &fields,
            entities,
            params: true,
        };
        expr_sql(&expr::parse(w).expect("validated expression"), &cx)
    });
    let body = bodies
        .get(&h.name)
        .cloned()
        .unwrap_or_else(|| stub(&h.name));
    out.push(format!(
        "create function \"svetra_holes\".{}({})\n  returns {returns} language plpgsql security definer set search_path = ''\n  as $svetra$\nbegin\n{}{}{}end\n$svetra$;",
        q(&h.name),
        inner_params.join(", "),
        guard.map_or(String::new(), |g| format!(
            "  if not coalesce({g}, false) then\n    raise exception 'not allowed to call {}' using errcode = '42501';\n  end if;\n",
            h.name
        )),
        body,
        if body.ends_with('\n') { "" } else { "\n" }
    ));
    out.push(format!("alter function {inner_sig} owner to {role};"));
    out.push(format!("revoke all on function {inner_sig} from public;"));
    out.push(format!(
        "grant execute on function {inner_sig} to current_user;"
    ));
    let outer_sig = format!("\"public\".{}({})", q(&h.name), types.join(", "));
    let outer_params: Vec<String> = inputs.iter().map(|(n, t)| format!("{n} {t}")).collect();
    let args: String = inputs.iter().map(|(n, _)| format!(", {n}")).collect();
    let call = format!("\"svetra_holes\".{}((select auth.uid()){args})", q(&h.name));
    let run = if h.output.is_some() {
        format!("return {call};")
    } else {
        format!("perform {call};")
    };
    out.push(format!(
        "create function \"public\".{}({})\n  returns {returns} language plpgsql security definer set search_path = ''\n  as $svetra$ begin {run} end $svetra$;",
        q(&h.name),
        outer_params.join(", ")
    ));
    out.push(format!("revoke all on function {outer_sig} from public;"));
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
    };
    let cond = expr_sql(&expr::parse(&p.when).expect("validated expression"), &cx);
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
                Action::Update => ("update", format!("using ({cond})\n  with check ({cond})")),
            };
            let name = if p.allow.len() == 1 {
                p.name.clone()
            } else {
                format!("{}_{action}", p.name)
            };
            format!(
                "create policy {} on {}\n  for {action} to {role}\n  {clauses};",
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
            let target = match &f.ty {
                FieldType::Ref(r) => {
                    let target = entities.get(r.name.as_str()).expect("validated reference");
                    let pk = target
                        .fields
                        .iter()
                        .find(|x| x.primary)
                        .expect("validated primary key");
                    format!("{} ({})", table(&target.name), q(&pk.name))
                }
                FieldType::User => "\"auth\".\"users\" (\"id\")".into(),
                _ => return None,
            };
            let name = q(&format!("{t}_{}_fkey", f.name));
            Some(format!(
                "alter table {} add constraint {name} foreign key ({}) references {target};",
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
