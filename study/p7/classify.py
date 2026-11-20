#!/usr/bin/env python3
"""Classify every surviving mutant of a mutation-study run (Svetra's scripts/mutate.py).

  classify.py round1/results.json round2/results.json > classification.md

Classes, for a mutant the access tests didn't catch:
  equivalent  the deployed app behaves the same for every person and every row
  masked      it behaves the same only because another layer still holds; a second change
              would expose it
  out-of-scope it changes only what a hole's own code can do, which the access tests don't run
  real        someone gets access the design doesn't give them
Each rule below states its reason. Anything no rule matches is UNCLASSIFIED, to be decided by
hand, so the counts can't quietly absorb a new kind of survivor.
"""
import json
import sys
from collections import Counter

def classify(r):
    app, op, t = r["app"], r["operator"], r["target"]
    if op == "policy-to-public" and t == "post.public_read":
        return "equivalent", "the rule already applies to both anon and signed-in people"
    if op == "policy-to-public" and ".svetra_hole_" in t:
        return "equivalent", "a private table: anon and signed-in people have no table privileges"
    if op == "policy-to-public":
        return "equivalent", "the rule depends on auth.uid() or has_role(), which never hold for anon"
    if app == "posts" and t == "post.posts_by_author_select":
        return "equivalent", "public_read already lets anyone see every post"
    if app == "posts" and op == "policy-all-actions" and t.startswith("post.posts_by_author_"):
        return "equivalent", "the added rule repeats posts_by_author, which already allows every action"
    if app == "posts" and op in ("table-leak-read", "table-leak-read-anon") and t == "post":
        return "equivalent", "public_read already lets anyone see every post"
    if app == "posts" and t == "audit_log" and op.startswith("table-"):
        return "equivalent", "a private table: anon and signed-in people have no table privileges"
    if app == "posts" and op == "policy-drop" and ".svetra_hole_" in t:
        return "out-of-scope", "only the hole's own code loses access; the tests don't run hole bodies"
    # Before the general update rules: public_read lets anyone see every post, so USING (true)
    # lets anyone reach a post, and setting its author to themselves passes WITH CHECK.
    if op in ("policy-check-true", "policy-using-true") and app == "posts":
        return "real", "a person can take a post over or give it away by changing its author"
    # bookings has two update policies on one table (staff and admins). PostgreSQL combines their
    # USING conditions with OR and their WITH CHECK conditions with OR, independently, so loosening
    # one policy's condition lets the other policy's person through. Verified on PostgreSQL 16
    # (data/z3/README.md): each of these four is allowed in a scenario the original refuses.
    if app == "bookings" and op == "policy-using-true" and t in ("booking.admins_manage_all_update", "booking.staff_manage_local_update"):
        return "real", "a customer who is also staff at another location can move their own booking there: the staff rule's WITH CHECK passes and the select rule shows the row"
    if app == "bookings" and op == "policy-check-true" and t in ("booking.admins_manage_all_update", "booking.staff_manage_local_update"):
        return "real", "staff at one location can take a booking: move it elsewhere and make themselves its customer, because the loosened WITH CHECK replaces the staff rule's"
    # Only where a table has a single update policy do the next two rules hold.
    if op == "policy-using-true" and t.endswith("_update"):
        return "masked", "WITH CHECK is the same rule and still rejects the updated row"
    if op == "policy-check-true" and "admin" in t:
        return "equivalent", "whoever passes USING (an admin) may already make any change"
    if op == "policy-check-true" and t == "booking.staff_manage_local_update":
        return "masked", "the select rules still apply to the updated row, so staff can't move a booking away"
    if op == "role-grants-writable":
        return "masked", "row-level security on role_grants, with no rules, still refuses every write"
    if op == "role-ignore-role" and app == "clinic":
        return "equivalent", "the app has only one role"
    if op == "role-ignore-role" and app == "bookings":
        return "masked", "the global and the scoped role never share a scope, so the scope check tells them apart"
    if op == "role-ignore-scope" and app == "clinic":
        return "equivalent", "the app has no scoped roles"
    if op == "hole-extra-grant" and app == "finledger":
        return "equivalent", "the hole already declares every table"
    if op == "hole-wrapper-anon" and app == "finledger":
        return "masked", "the guard (source.owner == auth.user) refuses anon; the declared callers are wider than designed"
    if op == "hole-body-callable":
        return "real", "the inside of the hole runs with a caller id the caller chooses"
    if app == "finledger" and t in ("transaction.owner_reads_own_transactions", "transaction"):
        return "real", "every signed-in person can read every transaction"
    if op == "role-ignore-person":
        return "real", "anyone with a second factor counts as an admin"
    if op == "role-grants-open":
        return "real", "a person can grant themselves any role"
    return "UNCLASSIFIED", ""

def table(path, title):
    rs = json.load(open(path))
    live = [r for r in rs if r["outcome"] != "stillborn"]
    caught = [r for r in live if r["outcome"] in ("killed", "error")]
    survivors = [r for r in live if r["outcome"] == "survived"]
    classes = Counter(classify(r)[0] for r in survivors)
    discount = classes["equivalent"] + classes["masked"] + classes["out-of-scope"]
    real = len(live) - discount
    out = [f"## {title}", "",
           f"- Live mutants: {len(live)}; caught: {len(caught)} ({100*len(caught)/len(live):.1f}%)",
           f"- Survivors: {len(survivors)}: " + ", ".join(f"{k} {v}" for k, v in sorted(classes.items())),
           f"- Caught among mutants that change access: {len(caught)} of {real} ({100*len(caught)/real:.1f}%)", "",
           "| App | Operator | Target | Class | Why |", "|---|---|---|---|---|"]
    for r in survivors:
        c, why = classify(r)
        out.append(f"| {r['app']} | `{r['operator']}` | `{r['target']}` | {c} | {why} |")
    return "\n".join(out) + "\n"

print("# Surviving mutants, classified\n")
print("Generated by `classify.py` from the study's `results.json` files.\n")
for i, p in enumerate(sys.argv[1:], 1):
    print(table(p, f"Round {i}: `{p}`"))
