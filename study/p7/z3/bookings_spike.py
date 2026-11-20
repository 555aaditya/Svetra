#!/usr/bin/env python3
"""Z3 spike (Arun Sharma's suggestion, 2026-11-19): read the bookings policies as formulas and ask
Z3 what each policy-level mutant of the mutation study does to who can reach a row, without a
database. Then compare with what the study found by running the tests.

  python3 bookings_spike.py ../round1/results.json ../round2/results.json

Scope, stated plainly: it models only the `bookings` application and the seven policy operators.
A person is described by atoms (owns the row, staff at its location, admin); anon has none of
them, because auth.uid() is null for anon and it holds no role. Every condition in this
application is a boolean over those atoms, so Z3 is more than the problem needs; the point is the
pipeline (policy text to formula to counterexample), which also handles richer conditions.
It does not model table privileges. An update also needs the select policies to show the row before and after, as observed on PostgreSQL 16.
"""
import json
import sys
from z3 import Bool, BoolVal, Or, And, Not, Solver, sat, substitute

own, staff, admin = Bool("own"), Bool("staff"), Bool("admin")      # the row as it is
own2, staff2 = Bool("own_new"), Bool("staff_new")                  # the row after an update
T, F = BoolVal(True), BoolVal(False)

# (name, table, command, roles, using, check). Conditions are lambdas of (own, staff) so that an
# update's USING reads the old row and its WITH CHECK reads the new one.
AUTH = frozenset({"authenticated"})
def P(name, table, cmd, using=None, check=None, roles=AUTH):
    return dict(name=name, table=table, cmd=cmd, using=using, check=check, roles=roles)
BASE = [
    P("customers_see_own_select", "booking", "select", lambda o, s: o),
    P("customers_see_own_insert", "booking", "insert", None, lambda o, s: o),
    P("staff_manage_local_select", "booking", "select", lambda o, s: s),
    P("staff_manage_local_update", "booking", "update", lambda o, s: s, lambda o, s: s),
    P("admins_manage_all_select", "booking", "select", lambda o, s: admin),
    P("admins_manage_all_update", "booking", "update", lambda o, s: admin, lambda o, s: admin),
    P("admins_manage_all_delete", "booking", "delete", lambda o, s: admin),
    P("staff_see_location", "location", "select", lambda o, s: s),
    P("admins_see_locations_select", "location", "select", lambda o, s: admin),
    P("admins_see_locations_insert", "location", "insert", None, lambda o, s: admin),
    P("admins_see_locations_update", "location", "update", lambda o, s: admin, lambda o, s: admin),
]

def mutate(policies, op, name):
    out = []
    for p in policies:
        p = dict(p)
        if p["name"] == name:
            if op == "policy-drop":
                continue
            if op == "policy-using-true" and p["using"]:
                p["using"] = lambda o, s: T
            elif op == "policy-using-negate" and p["using"]:
                u = p["using"]; p["using"] = lambda o, s, u=u: Not(u(o, s))
            elif op == "policy-check-true" and p["check"]:
                p["check"] = lambda o, s: T
            elif op == "policy-to-public":
                p["roles"] = frozenset({"anon", "authenticated"})
            elif op == "policy-role-swap":
                p["roles"] = frozenset({"anon"})
            elif op == "policy-all-actions":
                q = dict(p, name="svetra_mutant", cmd="all"); q["using"] = p["using"] or p["check"]; out.append(p); p = q   # USING defaults to WITH CHECK
        out.append(p)
    return out

def access(policies, table, cmd, actor):
    """The condition under which `actor` (auth or anon) may do `cmd` on `table`."""
    def sees(p, which):
        f = p[which] if p[which] else p["using"]
        if f is None:
            return None
        if which == "check" and cmd == "update":
            return f(own2, staff2)               # the row after the update
        return f(own, staff)
    role = "authenticated" if actor == "auth" else "anon"
    def sees_with(p, o, s):
        return p["using"](o, s)
    app = [p for p in policies if p["table"] == table and p["cmd"] in (cmd, "all") and role in p["roles"]]
    if cmd in ("select", "delete"):
        f = Or([sees(p, "using") for p in app if p["using"]] or [F])
    elif cmd == "insert":
        f = Or([sees(p, "check") for p in app if p["check"] or p["using"]] or [F])
    else:
        u = Or([sees(p, "using") for p in app if p["using"]] or [F])
        c = Or([sees(p, "check") for p in app if p["check"] or p["using"]] or [F])
        # An update also needs the select policies to show the row, before and after: observed
        # on PostgreSQL 16 (a staff member cannot move a booking to a location they do not staff,
        # even when the update policy's WITH CHECK is true). This is the "masked" layer.
        def visible(o, s):
            return Or([sees_with(p, o, s) for p in policies if p["table"] == table and p["cmd"] in ("select", "all")
                       and role in p["roles"] and p["using"]] or [F])
        f = And(u, c, visible(own, staff), visible(own2, staff2))
    if actor == "anon":                          # auth.uid() is null and anon holds no role
        f = substitute(f, *[(a, F) for a in (own, staff, admin, own2, staff2)])
    return f

def sat_(f):
    s = Solver(); s.add(f); return s.check() == sat

def verdict(base, mut):
    widen = narrow = False
    for t, cmds in (("booking", ("select", "insert", "update", "delete")), ("location", ("select", "insert", "update", "delete"))):
        for c in cmds:
            for a in ("auth", "anon"):
                x, y = access(base, t, c, a), access(mut, t, c, a)
                widen |= sat_(And(y, Not(x)))
                narrow |= sat_(And(x, Not(y)))
    return "change" if widen and narrow else "widen" if widen else "narrow" if narrow else "equal"

def main(paths):
    for path in paths:
        rs = [r for r in json.load(open(path)) if r["app"] == "bookings" and r["operator"].startswith("policy-")]
        tally, odd = {}, []
        for r in rs:
            name = r["target"].split(".", 1)[1]
            v = verdict(BASE, mutate(BASE, r["operator"], name))
            key = (v, r["outcome"])
            tally[key] = tally.get(key, 0) + 1
            if (v == "equal") != (r["outcome"] == "survived"):
                odd.append((r["operator"], r["target"], v, r["outcome"]))
        print(f"\n{path}: {len(rs)} policy-level mutants of bookings")
        for k in sorted(tally):
            print(f"  Z3 says {k[0]:7} | tests {k[1]:9} | {tally[k]}")
        for o in odd:
            print("  disagreement:", *o)

if __name__ == "__main__":
    main(sys.argv[1:])


def counterexample(op, target):
    """One person and row the mutant lets through that the original does not, if any."""
    name = target.split(".", 1)[1]
    mut = mutate(BASE, op, name)
    for t, cmds in (("booking", ("select", "insert", "update", "delete")), ("location", ("select", "insert", "update", "delete"))):
        for c in cmds:
            for a in ("auth", "anon"):
                s = Solver(); s.add(And(access(mut, t, c, a), Not(access(BASE, t, c, a))))
                if s.check() == sat:
                    m = s.model()
                    return t, c, a, {str(d): m[d] for d in m.decls()}
