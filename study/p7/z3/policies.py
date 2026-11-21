#!/usr/bin/env python3
"""Z3 model of every example app's policies, as a second reader of the mutation study's
policy-level classification. Generalises bookings_spike.py (which stays as the first spike).

  cargo build
  python3 policies.py --svetra ../../../target/debug/svetra [--fixtures ../../../tests/fixtures] \
      ../round1/results.json ../round2/results.json ../round3/results.json

What it does. For each app it runs `svetra compile` on the fixture into a temporary directory and
reads the policies, the table privileges and the role definition out of the generated SQL (not
from a hand copy). Each policy condition becomes a formula over facts about a person and a row:

  own        the person is the value of a user column of the row      ("author" = auth.uid())
  refown     the person owns the row a reference column points to     (source.owner == auth.user)
  role       the person holds a global role; for a role with mfa: true, a second factor too
  role@col   the person holds a role scoped to the row's column value  (has_role('staff', "location"))

For an update, the row before and the row after are separate sets of facts. Conditions are
evaluated with SQL's three-valued logic (a NULL result does not pass, and NOT NULL is NULL), and
anon, and a hole's own database role, have no uid and no role. Then it applies the seven policy
operators exactly as scripts/mutate.py does and asks Z3, for every table, command and actor
(anon, authenticated, each hole role), whether the mutant lets anyone do something the original
does not (widen), stops someone doing it (narrow), or both (change).

PostgreSQL rules encoded (all observed on PostgreSQL 16, see README.md):
  select, delete   the OR of the applicable policies' USING; an update or delete also needs the
                   select policies to show the old row, an update also the new one
  insert           the OR of the applicable policies' WITH CHECK (USING when an ALL policy has none)
  update           the OR of the USING on the old row, and the OR of the WITH CHECK on the new row
  a policy with no `to` role applies to everyone; `to <hole role>` applies to nobody but the hole
Table privileges come from the generated SQL: `revoke all ... from anon, authenticated` on a
private table; otherwise Supabase's default grants (all) to anon and authenticated, and each hole
role has the grants the compiler gives it.

Anything it cannot read faithfully raises Unmodelled, and the mutant is reported "not modelled"
with the reason, never guessed.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter

from z3 import And, Bool, BoolVal, Not, Or, Solver, sat

T, F = BoolVal(True), BoolVal(False)
CMDS = ("select", "insert", "update", "delete")


class Unmodelled(Exception):
    pass


# ---------------------------------------------------------------- reading the compiled SQL

def balanced(s, start):
    """The text inside the parentheses that open at s[start], and the index after the close."""
    assert s[start] == "("
    depth, i, quote = 0, start, None
    while i < len(s):
        c = s[i]
        if quote:
            quote = None if c == quote else quote
        elif c in "'\"":
            quote = c
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return s[start + 1:i], i + 1
        i += 1
    raise Unmodelled("unbalanced parentheses")


def split_top(s, kw):
    """Split s on kw where it is outside parentheses and quotes."""
    parts, depth, quote, last, i = [], 0, None, 0, 0
    while i < len(s):
        c = s[i]
        if quote:
            quote = None if c == quote else quote
        elif c in "'\"":
            quote = c
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
        elif depth == 0 and s.startswith(kw, i):
            parts.append(s[last:i])
            i += len(kw)
            last = i
            continue
        i += 1
    parts.append(s[last:])
    return parts


OWN = re.compile(r'^"(\w+)" = \(select auth\.uid\(\)\)$')
REFOWN = re.compile(r'^\(select t\."(\w+)" from "(\w+)"\."(\w+)" t where t\."(\w+)" = "(\w+)"\."(\w+)"\) = \(select auth\.uid\(\)\)$')
ROLE = re.compile(r"""^"svetra"\."has_role"\('(\w+)'(?:, "(\w+)")?\)$""")


def parse(s):
    """A condition as a tree: ('true',), ('or'|'and', [..]), ('not', x), ('own', col),
    ('refown', owner column, other table, its key, this row's column), ('role', role, scopecol or None)."""
    s = s.strip()
    for kw in (" or ", " and "):
        parts = split_top(s, kw)
        if len(parts) > 1:
            return (kw.strip(), [parse(p) for p in parts])
    if s == "true":
        return ("true",)
    if s.startswith("not "):
        return ("not", parse(s[4:]))
    if m := OWN.match(s):
        return ("own", m[1])
    if m := REFOWN.match(s):
        # the owner column of the row in another table whose key equals one of this row's columns
        return ("refown", m[1], m[3], m[4], m[6])
    w = re.match(r"^\(select (.*)\)$", s, re.S)
    if w and (m := ROLE.match(w[1])):
        return ("role", m[1], m[2])
    if m := ROLE.match(s):
        return ("role", m[1], m[2])
    if s.startswith("("):
        inner, end = balanced(s, 0)
        if end == len(s):
            return parse(inner)
    raise Unmodelled(f"condition not understood: {s[:80]}")


def P(name, table, cmd, roles, using, check):
    return dict(name=name, table=table, cmd=cmd, roles=frozenset(roles), using=using, check=check)


class App:
    def __init__(self, sql):
        self.policies = []
        for stmt in re.findall(r"create policy .*?;\n", sql, re.S):
            m = re.match(r'create policy "(\w+)" on "public"\."(\w+)"\s+for (\w+) to (.*?)(?=\s+using \(|\s+with check \(|;)', stmt, re.S)
            if not m:
                raise Unmodelled("policy statement not understood: " + stmt[:80])
            name, table, cmd, roles = m.groups()
            roles = [r.strip().strip('"') for r in roles.split(",")]
            rest = stmt[m.end():]
            u = c = None
            if (i := rest.find(" using (")) >= 0:
                u = parse(balanced(rest, i + 7)[0])
            if (i := rest.find("with check (")) >= 0:
                c = parse(balanced(rest, i + 11)[0])
            self.policies.append(P(name, table, cmd, roles, u, c))
        self.tables = sorted({p["table"] for p in self.policies} | set(re.findall(r'create table "public"\."(\w+)"', sql)))
        # a role with mfa: true needs a second factor (aal2) in the same has_role function
        m = re.search(r"""not in \(([^)]*)\) or coalesce\(\(select auth\.jwt\(\) ->> 'aal'\), ''\) = 'aal2'""", sql)
        self.mfa = set(re.findall(r"'(\w+)'", m[1])) if m else set()
        if "aal" in sql and not m:
            raise Unmodelled("the second-factor test in has_role is not the expected one")
        revoked = set(re.findall(r'revoke all on table "public"\."(\w+)" from anon, authenticated', sql))
        self.priv = {}
        for t in self.tables:
            for a in ("anon", "authenticated"):
                self.priv[(t, a)] = set() if t in revoked else set(CMDS)
        self.holes = sorted(set(re.findall(r'create role "(svetra_hole_\w+)"', sql)))
        for h in self.holes:
            for t in self.tables:
                m = re.search(r'grant ([a-z, ]+) on table "public"\."%s" to "%s"' % (t, h), sql)
                self.priv[(t, h)] = set(x.strip() for x in m[1].split(",")) if m else set()
        self.actors = ["anon", "authenticated"] + self.holes


# ---------------------------------------------------------------- formulas

def ev(app, tree, actor, row):
    """SQL three-valued truth of a condition: (true, null) formulas; false is neither."""
    k = tree[0]
    anon = actor != "authenticated"           # anon and hole roles: auth.uid() is null, no role
    if k == "true":
        return T, F
    if k in ("own", "refown"):
        if anon:
            return F, T                        # column = NULL is NULL
        return Bool("%s.%s" % ("/".join(map(str, tree)), row)), F
    if k == "role":
        if anon:
            return F, F                        # has_role() is an exists(): false, never NULL
        _, role, scope = tree
        a = Bool("role.%s@%s.%s" % (role, scope, row)) if scope else Bool("role.%s" % role)
        if role in app.mfa:
            a = And(a, Bool("aal2"))
        return a, F
    if k == "not":
        t, n = ev(app, tree[1], actor, row)
        return And(Not(t), Not(n)), n
    parts = [ev(app, x, actor, row) for x in tree[1]]
    if k == "and":
        t = And([p[0] for p in parts])
        f = Or([And(Not(p[0]), Not(p[1])) for p in parts])
    else:
        t = Or([p[0] for p in parts])
        f = And([And(Not(p[0]), Not(p[1])) for p in parts])
    return t, Not(Or(t, f))


def applies(p, actor, cmds):
    return p["cmd"] in cmds and ("public" in p["roles"] or actor in p["roles"])


def access(app, policies, table, cmd, actor):
    if cmd not in app.priv.get((table, actor), ()):
        return F
    def pol(cmds):
        return [p for p in policies if p["table"] == table and applies(p, actor, cmds)]
    def using(p, row):
        return ev(app, p["using"], actor, row)[0]
    def check(p, row):
        return ev(app, p["check"] or p["using"], actor, row)[0]
    def visible(row):
        return Or([using(p, row) for p in pol(("select", "all")) if p["using"]] or [F])
    if cmd == "select":
        return visible("o")
    if cmd == "insert":
        return Or([check(p, "o") for p in pol(("insert", "all")) if p["check"] or p["using"]] or [F])
    u = Or([using(p, "o") for p in pol((cmd, "all")) if p["using"]] or [F])
    if cmd == "delete":
        # a delete finds its row with a WHERE, so the select policies must show it
        return And(u, visible("o"))
    c = Or([check(p, "n") for p in pol(("update", "all")) if p["check"] or p["using"]] or [F])
    return And(u, c, visible("o"), visible("n"))


def mutate(policies, op, target):
    table, name = target.split(".", 1)
    out = []
    for p in policies:
        if p["table"] != table or p["name"] != name:
            out.append(p)
            continue
        p = dict(p)
        if op == "policy-drop":
            continue
        if op == "policy-using-true" and p["using"]:
            p["using"] = ("true",)
        elif op == "policy-using-negate" and p["using"]:
            p["using"] = ("not", p["using"])
        elif op == "policy-check-true" and p["check"]:
            p["check"] = ("true",)
        elif op == "policy-to-public":
            p["roles"] = frozenset({"public"})
        elif op == "policy-role-swap":
            swap = {"authenticated": "anon", "anon": "authenticated"}
            if len(p["roles"]) != 1 or next(iter(p["roles"])) not in swap:
                raise Unmodelled("role swap on roles " + ",".join(sorted(p["roles"])))
            p["roles"] = frozenset({swap[next(iter(p["roles"]))]})
        elif op == "policy-all-actions":
            u = p["using"] or p["check"]
            out.append(p)
            p = dict(p, name="svetra_mutant", cmd="all", using=u, check=p["check"] or u)
        else:
            raise Unmodelled("operator %s does not apply to this policy" % op)
        out.append(p)
    return out


def sat_(f):
    s = Solver()
    s.add(f)
    return s.check() == sat


def diff(app, base, mut):
    """[(table, cmd, actor, 'widen'|'narrow')] for every place the two rule sets differ."""
    out = []
    for t in app.tables:
        for c in CMDS:
            for a in app.actors:
                x, y = access(app, base, t, c, a), access(app, mut, t, c, a)
                if sat_(And(y, Not(x))):
                    out.append((t, c, a, "widen"))
                if sat_(And(x, Not(y))):
                    out.append((t, c, a, "narrow"))
    return out


def verdict(d):
    """The summary verdict. Changes that touch only a hole's own role are 'hole-only'."""
    people = [x for x in d if not x[2].startswith("svetra_hole_")]
    kinds = {x[3] for x in people}
    if not people:
        return "hole-only" if d else "equal"
    return "change" if len(kinds) == 2 else kinds.pop()


def counterexample(app, base, mut, table, cmd, actor, kind):
    x, y = access(app, base, table, cmd, actor), access(app, mut, table, cmd, actor)
    s = Solver()
    s.add(And(y, Not(x)) if kind == "widen" else And(x, Not(y)))
    if s.check() != sat:
        return None
    m = s.model()
    return {str(d): bool(m[d]) for d in m.decls()}


# ---------------------------------------------------------------- the study

def load_classify(here):
    src = open(os.path.join(here, "..", "classify.py")).read().split("def table")[0]
    ns = {}
    exec(compile(src, "classify.py", "exec"), ns)
    return ns["classify"]


def compile_app(svetra, fixture):
    with tempfile.TemporaryDirectory() as w:
        open(w + "/svetra.yaml", "w").write(open(fixture).read())
        subprocess.run([svetra, "compile", w + "/svetra.yaml"], check=True, capture_output=True)
        return App(open(w + "/supabase/schemas/svetra.sql").read())


def agreement(v, outcome, cls):
    """(model vs the tests, model vs the classifier).
    Tests: 'agree'; 'tests-miss' (the model sees a change to who has access that the tests did not
    catch, a gap); 'DISAGREE' (the tests caught a mutant the model says changes nothing).
    Classifier, for survivors only: 'agree' or 'DISAGREE'."""
    caught = outcome in ("killed", "error")
    if v == "hole-only":
        tests = "agree"          # only a hole's own role changes; either outcome is consistent
    elif v == "equal":
        tests = "DISAGREE" if caught else "agree"
    else:
        tests = "agree" if caught else "tests-miss"
    if outcome != "survived":
        return tests, "-"
    if cls == "out-of-scope":
        ok = v == "hole-only"
    elif cls in ("equivalent", "masked"):
        ok = v == "equal"
    else:
        ok = v not in ("equal", "hole-only")
    return tests, "agree" if ok else "DISAGREE"


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    ap.add_argument("results", nargs="+")
    ap.add_argument("--svetra", default=os.path.join(here, "../../../target/debug/svetra"))
    ap.add_argument("--fixtures", default=os.path.join(here, "../../../tests/fixtures"))
    ap.add_argument("--json", help="write every mutant's verdict here")
    a = ap.parse_args()
    classify = load_classify(here)
    apps, rows = {}, []
    for path in a.results:
        rnd = os.path.basename(os.path.dirname(os.path.abspath(path)))
        for r in json.load(open(path)):
            if not r["operator"].startswith("policy-") or r["outcome"] == "stillborn":
                continue
            app = r["app"]
            if app not in apps:
                try:
                    apps[app] = compile_app(a.svetra, os.path.join(a.fixtures, app + ".yaml"))
                except Unmodelled as e:
                    apps[app] = e
            row = dict(round=rnd, app=app, operator=r["operator"], target=r["target"], outcome=r["outcome"])
            cls = classify(r)[0] if r["outcome"] == "survived" else None
            row["class"] = cls
            if isinstance(apps[app], Unmodelled):
                row.update(verdict="not modelled", why=str(apps[app]))
            else:
                try:
                    base = apps[app].policies
                    mut = mutate(base, r["operator"], r["target"])
                    d = diff(apps[app], base, mut)
                    row.update(verdict=verdict(d), detail=d)
                except Unmodelled as e:
                    row.update(verdict="not modelled", why=str(e))
            if row["verdict"] == "not modelled":
                row["tests_vs_model"] = row["class_vs_model"] = "n/a"
            else:
                row["tests_vs_model"], row["class_vs_model"] = agreement(row["verdict"], r["outcome"], cls)
            rows.append(row)
    report(rows)
    parity(rows)
    if a.json:
        json.dump(rows, open(a.json, "w"), indent=1, default=str)
    return rows


def report(rows):
    print("| Round | App | Mutants | Model agrees with tests | Tests miss a change the model sees | Survivors the classifier reads the same way | Not modelled |")
    print("|---|---|---|---|---|---|---|")
    for rnd in sorted({r["round"] for r in rows}):
        for app in sorted({r["app"] for r in rows}):
            rs = [r for r in rows if r["round"] == rnd and r["app"] == app]
            if not rs:
                continue
            nm = sum(r["verdict"] == "not modelled" for r in rs)
            t = [r for r in rs if r["tests_vs_model"] != "n/a"]
            c = [r for r in rs if r["class_vs_model"] not in ("n/a", "-")]
            ta = sum(r["tests_vs_model"] == "agree" for r in t)
            ca = sum(r["class_vs_model"] == "agree" for r in c)
            miss = sum(r["tests_vs_model"] == "tests-miss" for r in rs)
            print(f"| {rnd} | {app} | {len(rs)} | {ta} of {len(t)} | {miss} | {ca} of {len(c)} | {nm} |")
    print()
    tally = Counter((r["round"], r["app"], r["verdict"], r["outcome"], r["class"]) for r in rows)
    print("model verdict | tests | classifier class | count, per round and app")
    for k in sorted(tally, key=str):
        print("  ", *k, tally[k])
    print()
    for r in rows:
        if r["tests_vs_model"] == "tests-miss":
            print("TESTS MISS", r["round"], r["app"], r["operator"], r["target"], "| model", r["verdict"], "| class", r["class"])
    for r in rows:
        if "DISAGREE" in (r["tests_vs_model"], r["class_vs_model"]):
            print("DISAGREEMENT", r["round"], r["app"], r["operator"], r["target"], "| model", r["verdict"], "| tests", r["outcome"], "| class", r["class"])
    for r in rows:
        if r["verdict"] == "not modelled":
            print("NOT MODELLED", r["round"], r["app"], r["operator"], r["target"], r.get("why"))


def parity(rows):
    """The bookings verdicts must equal the first spike's (bookings_spike.py), mutant by mutant."""
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import bookings_spike as sp
    bs = [r for r in rows if r["app"] == "bookings"]
    same = sum(r["verdict"] == sp.verdict(sp.BASE, sp.mutate(sp.BASE, r["operator"], r["target"].split(".", 1)[1])) for r in bs)
    print(f"bookings_spike.py gives the same verdict for {same} of {len(bs)} bookings mutants")


if __name__ == "__main__":
    main()
