//! Translate checked expressions to SQL (docs/compiler.md, Expressions in SQL).
//! Input has already passed `typeck`, so paths resolve and types are sound.

use super::names::{lit, q, snake, table};
use crate::expr::{Expr, Op};
use crate::model::{Entity, FieldType};
use std::collections::HashMap;

/// Where the expression is emitted: the entity it belongs to and every entity by name.
pub struct Ctx<'a> {
    pub entity: &'a Entity,
    pub entities: &'a HashMap<&'a str, &'a Entity>,
}

pub fn sql(e: &Expr, cx: &Ctx) -> String {
    match e {
        Expr::Num(n) => format!("{n}"),
        Expr::Str(s) => lit(s),
        Expr::Bool(b) => b.to_string(),
        Expr::Null => "null".into(),
        Expr::Path(p) => path(p, cx),
        // has_role: a global role is checked once per query; a scoped role per row.
        Expr::Call(_, args) => match args.as_slice() {
            [Expr::Str(role)] => format!("(select \"svetra\".\"has_role\"({}))", lit(role)),
            [Expr::Str(role), scope] => {
                format!("\"svetra\".\"has_role\"({}, {})", lit(role), sql(scope, cx))
            }
            _ => unreachable!("typeck checks has_role's arguments"),
        },
        Expr::Neg(x) => format!("-{}", child(x, 7, cx)),
        // Always parenthesise a compound operand: `not (a = b)` reads clearly in a diff.
        Expr::Not(x) => format!("not {}", child(x, 8, cx)),
        Expr::In(x, items) => {
            let list: Vec<String> = items.iter().map(|i| sql(i, cx)).collect();
            format!("{} in ({})", child(x, 5, cx), list.join(", "))
        }
        Expr::Bin(op @ (Op::Eq | Op::Ne), l, r)
            if matches!(**l, Expr::Null) || matches!(**r, Expr::Null) =>
        {
            let x = if matches!(**l, Expr::Null) { r } else { l };
            let not = if *op == Op::Ne { " not" } else { "" };
            format!("{} is{not} null", child(x, 5, cx))
        }
        Expr::Bin(op, l, r) => {
            let p = prec(e);
            format!("{} {} {}", child(l, p, cx), sym(*op), child_right(r, p, cx))
        }
    }
}

fn sym(op: Op) -> &'static str {
    match op {
        Op::Or => "or",
        Op::And => "and",
        Op::Eq => "=",
        Op::Ne => "<>",
        Op::Lt => "<",
        Op::Le => "<=",
        Op::Gt => ">",
        Op::Ge => ">=",
        Op::Add => "+",
        Op::Sub => "-",
        Op::Mul => "*",
        Op::Div => "/",
    }
}

/// Binding strength, matching both the Svetra grammar and SQL.
fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Bin(Op::Or, ..) => 1,
        Expr::Bin(Op::And, ..) => 2,
        Expr::Not(_) => 3,
        Expr::Bin(Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge, ..) | Expr::In(..) => 4,
        Expr::Bin(Op::Add | Op::Sub, ..) => 5,
        Expr::Bin(Op::Mul | Op::Div, ..) => 6,
        Expr::Neg(_) => 7,
        _ => 8,
    }
}

/// Parenthesise a left operand only when it binds looser than its parent.
fn child(e: &Expr, parent: u8, cx: &Ctx) -> String {
    if prec(e) < parent {
        format!("({})", sql(e, cx))
    } else {
        sql(e, cx)
    }
}

/// Right operands also keep parentheses at equal strength, so `a - (b - c)` stays correct.
fn child_right(e: &Expr, parent: u8, cx: &Ctx) -> String {
    if prec(e) <= parent {
        format!("({})", sql(e, cx))
    } else {
        sql(e, cx)
    }
}

fn path(p: &[String], cx: &Ctx) -> String {
    match p {
        [f] => q(f),
        [a, b] if a == "auth" && b == "user" => "(select auth.uid())".into(),
        [r, f] => {
            // One hop through a ref field: a scalar subquery on the referenced row's primary key.
            let target = cx.entity.fields.iter().find_map(|x| match &x.ty {
                FieldType::Ref(t) if &x.name == r => cx.entities.get(t.name.as_str()),
                _ => None,
            });
            let target = target.expect("typeck resolved the reference");
            let pk = target
                .fields
                .iter()
                .find(|x| x.primary)
                .expect("validated primary key");
            format!(
                "(select t.{} from {} t where t.{} = {}.{})",
                q(f),
                table(&target.name),
                q(&pk.name),
                q(&snake(&cx.entity.name)),
                q(r)
            )
        }
        _ => unreachable!("typeck rejects deeper paths"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parse;
    use crate::load::load;

    fn to_sql(entity: &str, src: &str) -> String {
        let (doc, diags) = load(include_str!("../../tests/fixtures/finledger.yaml"));
        assert!(diags.is_empty());
        let entities: HashMap<&str, &Entity> =
            doc.entities.iter().map(|e| (e.name.as_str(), e)).collect();
        let cx = Ctx {
            entity: entities[entity],
            entities: &entities,
        };
        sql(&parse(src).unwrap(), &cx)
    }

    #[test]
    fn operators_nulls_and_literals() {
        assert_eq!(to_sql("Account", "balance >= 0"), "\"balance\" >= 0");
        assert_eq!(
            to_sql("Transaction", "source != destination"),
            "\"source\" <> \"destination\""
        );
        assert_eq!(to_sql("Account", "status == null"), "\"status\" is null");
        assert_eq!(
            to_sql("Account", "null != status"),
            "\"status\" is not null"
        );
        assert_eq!(
            to_sql("Account", "status in ['active', 'it''s']"),
            "\"status\" in ('active', 'it''s')"
        );
        assert_eq!(
            to_sql("Account", "balance > 0.5 and not (status == 'closed')"),
            "\"balance\" > 0.5 and not (\"status\" = 'closed')"
        );
    }

    #[test]
    fn parentheses_follow_precedence() {
        assert_eq!(
            to_sql("Account", "(balance - 1) - 2 > 0"),
            "\"balance\" - 1 - 2 > 0"
        );
        assert_eq!(
            to_sql("Account", "balance - (1 - 2) > 0"),
            "\"balance\" - (1 - 2) > 0"
        );
        assert_eq!(
            to_sql("Account", "(balance > 0 or balance < 0) and status != null"),
            "(\"balance\" > 0 or \"balance\" < 0) and \"status\" is not null"
        );
    }

    #[test]
    fn auth_user_and_one_hop_paths() {
        assert_eq!(
            to_sql("Account", "owner == auth.user"),
            "\"owner\" = (select auth.uid())"
        );
        assert_eq!(
            to_sql("Transaction", "source.owner == auth.user"),
            "(select t.\"owner\" from \"public\".\"account\" t where t.\"id\" = \"transaction\".\"source\") = (select auth.uid())"
        );
    }
}
