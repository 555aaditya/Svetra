#!/usr/bin/env python3
"""Mutation study of the generated access tests.

For each example app: compile it, apply the schema to a fresh PostgreSQL with the Supabase
stand-in, and check that the generated access tests pass. Then break the deployed database one
way at a time (each mutant on its own copy, made from the deployed one as a template), run the
same tests, and record whether they catch it ("killed") and which checks fail.

Mutants change the database the way a hand-written migration or a careless fix would: a policy
dropped, widened or moved to another role, row-level security turned off, an extra permissive
policy, the role check or a hole's guard weakened, extra grants. Each is labelled `widen` (more
access than declared: a leak), `narrow` (less: a broken feature) or `change` (both).

Needs Docker and a built `svetra` binary. Standard library only.

  scripts/mutate.py [--out DIR] [--only bookings,clinic] [--svetra BIN]

Writes DIR/results.json (every mutant) and DIR/summary.md (kill rates), default target/mutate.
"""
import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ["finledger", "posts", "bookings", "clinic"]
CONTAINER = "svetra-mutate"


def sh(*args, stdin=None, check=True):
    r = subprocess.run(args, input=stdin, capture_output=True, text=True)
    if check and r.returncode != 0:
        sys.exit(f"failed: {' '.join(args)}\n{r.stdout}{r.stderr}")
    return r


def psql(sql, db="postgres", check=True):
    """Run SQL; stop at the first error. Returns the completed process."""
    return sh("docker", "exec", "-i", CONTAINER, "psql", "-U", "postgres", "-d", db,
              "-v", "ON_ERROR_STOP=1", "-q", "-At", stdin=sql, check=check)


def query(sql, db="postgres"):
    """Rows of a query, as lists of strings. Values may span lines (function definitions), so
    rows and columns are split on ASCII separators."""
    out = sh("docker", "exec", "-i", CONTAINER, "psql", "-U", "postgres", "-d", db, "-At",
             "-F", "\x1f", "-R", "\x1e", stdin=sql).stdout
    return [row.split("\x1f") for row in out.rstrip("\n").split("\x1e") if row]


def run_tests(tests, db):
    """Run a generated test file. Returns (failed check names, error text or None, checks run)."""
    out = sh("docker", "exec", "-i", CONTAINER, "psql", "-U", "postgres", "-d", db, "-At",
             stdin=tests, check=False)
    text = out.stdout + out.stderr
    failed = re.findall(r"^not ok \d+ - (.*)$", text, re.M)
    error = next((line for line in text.splitlines() if line.startswith("ERROR")), None)
    return failed, error, len(re.findall(r"^(?:not )?ok \d+", text, re.M))


def start():
    sh("docker", "rm", "-f", CONTAINER, check=False)
    sh("docker", "run", "-d", "--name", CONTAINER, "-e", "POSTGRES_PASSWORD=pw", "postgres:16")
    # The image runs a temporary server while it initialises, then restarts: wait for the real one.
    for _ in range(90):
        if "init process complete" in sh("docker", "logs", CONTAINER, check=False).stderr:
            break
        time.sleep(1)
    for _ in range(60):
        if sh("docker", "exec", CONTAINER, "pg_isready", "-U", "postgres", check=False).returncode == 0:
            break
        time.sleep(1)
    sh("docker", "exec", CONTAINER, "sh", "-c",
       'apt-get update -qq && apt-get install -y -qq "postgresql-$PG_MAJOR-pgtap" >/dev/null')


def q(name):
    return '"' + name.replace('"', '""') + '"'


def mutants():
    """Every mutant of the database deployed as `postgres`: (operator, direction, target, sql)."""
    out = []
    policies = query("""select tablename, policyname, cmd, array_to_string(roles, ','),
                               coalesce(qual, ''), coalesce(with_check, '')
                        from pg_policies where schemaname = 'public' order by 1, 2""")
    for table, name, cmd, roles, qual, check in policies:
        on = f"{q(name)} on public.{q(table)}"
        target = f"{table}.{name}"
        out.append(("policy-drop", "narrow", target, f"drop policy {on};"))
        if qual and qual != "true":
            out.append(("policy-using-true", "widen", target, f"alter policy {on} using (true);"))
            out.append(("policy-using-negate", "change", target, f"alter policy {on} using (not ({qual}));"))
        if check and check != "true":
            out.append(("policy-check-true", "widen", target, f"alter policy {on} with check (true);"))
        if roles != "public":
            out.append(("policy-to-public", "widen", target, f"alter policy {on} to public;"))
        swap = {"authenticated": "anon", "anon": "authenticated"}.get(roles)
        if swap:
            out.append(("policy-role-swap", "change", target, f"alter policy {on} to {swap};"))
        if cmd != "ALL":
            # The same rule, mistakenly written for every action.
            using = qual or check or "true"
            out.append(("policy-all-actions", "widen", target,
                        f"create policy svetra_mutant on public.{q(table)} for all to {roles} "
                        f"using ({using}) with check ({check or using});"))
    tables = query("""select c.relname from pg_class c join pg_namespace n on n.oid = c.relnamespace
                      where n.nspname = 'public' and c.relkind = 'r' and c.relrowsecurity order by 1""")
    for (table,) in tables:
        t = f"public.{q(table)}"
        out.append(("table-rls-off", "widen", table, f"alter table {t} disable row level security;"))
        out.append(("table-leak-read", "widen", table,
                    f"create policy svetra_mutant on {t} for select to authenticated using (true);"))
        out.append(("table-leak-read-anon", "widen", table,
                    f"create policy svetra_mutant on {t} for select to anon using (true);"))
        out.append(("table-leak-write", "widen", table,
                    f"create policy svetra_mutant on {t} for all to authenticated using (true) with check (true);"))
        out.append(("table-revoke-read", "narrow", table, f"revoke select on {t} from authenticated, anon;"))
    if query("select 1 from pg_proc p join pg_namespace n on n.oid = p.pronamespace "
             "where n.nspname = 'svetra' and p.proname = 'has_role'"):
        (definition,) = query("select pg_get_functiondef('svetra.has_role(text, uuid)'::regprocedure)")[0]
        for op, find, replace in [
            ("role-check-true", r"AS \$function\$.*\$function\$", "AS $function$ select true $function$"),
            ("role-ignore-person", r'g\."user_id" = \(select auth\.uid\(\)\) and ', ""),
            ("role-ignore-role", r' and g\."role" = p_role', ""),
            ("role-ignore-scope", r' and g\."scope_id" is not distinct from p_scope', ""),
            ("role-ignore-second-factor", r"\n\s*and \(g\.\"role\" not in .*?'aal2'\)", ""),
        ]:
            mutated = re.sub(find, replace, definition, count=1, flags=re.S)
            if mutated != definition:
                out.append((op, "widen", "svetra.has_role", mutated + ";"))
        out.append(("role-grants-writable", "widen", "svetra.role_grants",
                    "grant insert, update, delete on svetra.role_grants to authenticated;"))
        # Two changes: row-level security on the grants table masks the grant above alone.
        out.append(("role-grants-open", "widen", "svetra.role_grants",
                    "grant insert, update, delete on svetra.role_grants to authenticated; "
                    "alter table svetra.role_grants disable row level security;"))
    holes = query("""select p.oid::regprocedure::text, pg_get_functiondef(p.oid), pg_get_userbyid(p.proowner)
                     from pg_proc p join pg_namespace n on n.oid = p.pronamespace
                     where n.nspname = 'svetra_holes' order by 1""")
    for signature, definition, owner in holes:
        hole = signature.split("(")[0].split(".")[-1]
        unguarded = re.sub(r"\n\s*if not coalesce\(.*?\n\s*end if;", "", definition, count=1, flags=re.S)
        if unguarded != definition:
            out.append(("hole-no-guard", "widen", hole, unguarded + ";"))
        out.append(("hole-extra-grant", "widen", hole,
                    f"grant select, insert, update, delete on all tables in schema public to {q(owner)};"))
        out.append(("hole-body-callable", "widen", hole,
                    f"grant usage on schema svetra_holes to authenticated, anon; "
                    f"grant execute on function {signature} to authenticated, anon;"))
        wrapper = query(f"""select p.oid::regprocedure::text from pg_proc p join pg_namespace n
                            on n.oid = p.pronamespace where n.nspname = 'public' and p.proname = '{hole}'""")
        for (w,) in wrapper:
            out.append(("hole-wrapper-anon", "widen", hole, f"grant execute on function {w} to anon;"))
    return out


def study(name, work, svetra):
    app = work / name
    app.mkdir()
    shutil.copy(ROOT / f"tests/fixtures/{name}.yaml", app / "svetra.yaml")
    holes = ROOT / f"tests/fixtures/holes/{name}"
    if holes.is_dir():
        shutil.copytree(holes, app / "supabase/holes")
    sh(svetra, "compile", str(app / "svetra.yaml"))
    psql((ROOT / "tests/pg/supabase_stub.sql").read_text())
    psql((app / "supabase/schemas/svetra.sql").read_text())
    tests = (app / "supabase/tests/svetra.test.sql").read_text()
    failed, error, total = run_tests(tests, "postgres")
    if failed or error:
        sys.exit(f"{name}: the tests fail before any mutation: {failed or error}")
    results = []
    for i, (op, direction, target, sql) in enumerate(mutants()):
        db = f"mutant_{i}"
        psql(f"create database {db} template postgres;", db="template1")
        applied = psql(sql, db=db, check=False)
        if applied.returncode != 0:
            outcome, failed, error = "stillborn", [], applied.stderr.strip()
        else:
            failed, error, _ = run_tests(tests, db)
            outcome = "killed" if failed else ("error" if error else "survived")
        psql(f"drop database {db};", db="template1")
        results.append({"app": name, "operator": op, "direction": direction, "target": target,
                        "sql": sql, "outcome": outcome, "failed": failed, "error": error})
        print(f"  {outcome:9} {op:26} {target}", flush=True)
    return total, results


def summary(checks, results):
    lines = ["# Mutation study: summary", "",
             "Killed: at least one access check failed. Error: the test file stopped with an error "
             "(also detected). Stillborn: the mutation itself didn't apply, so it's left out.", "",
             "| App | Checks | Mutants | Killed | Error | Survived | Stillborn | Kill rate |",
             "|---|---|---|---|---|---|---|---|"]
    def rate(rs):
        live = [r for r in rs if r["outcome"] != "stillborn"]
        caught = [r for r in live if r["outcome"] in ("killed", "error")]
        return len(live), len(caught), (f"{100 * len(caught) / len(live):.1f}%" if live else "n/a")
    for app in checks:
        rs = [r for r in results if r["app"] == app]
        n = {o: sum(r["outcome"] == o for r in rs) for o in ("killed", "error", "survived", "stillborn")}
        lines.append(f"| {app} | {checks[app]} | {len(rs)} | {n['killed']} | {n['error']} | "
                     f"{n['survived']} | {n['stillborn']} | {rate(rs)[2]} |")
    lines += ["", "| Direction | Live mutants | Caught | Kill rate |", "|---|---|---|---|"]
    for d in ("widen", "change", "narrow"):
        live, caught, r = rate([x for x in results if x["direction"] == d])
        lines.append(f"| {d} | {live} | {caught} | {r} |")
    lines += ["", "| Operator | Direction | Live | Caught | Kill rate |", "|---|---|---|---|---|"]
    for op in sorted({r["operator"] for r in results}):
        rs = [r for r in results if r["operator"] == op]
        live, caught, r = rate(rs)
        lines.append(f"| {op} | {rs[0]['direction']} | {live} | {caught} | {r} |")
    lines += ["", "## Survivors", ""]
    for r in results:
        if r["outcome"] == "survived":
            lines.append(f"- **{r['app']}** `{r['operator']}` on `{r['target']}`")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(ROOT / "target/mutate"))
    ap.add_argument("--only", default=",".join(FIXTURES))
    ap.add_argument("--svetra", default=str(ROOT / "target/debug/svetra"),
                    help="the svetra binary whose generated tests are studied")
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    checks, results = {}, []
    try:
        with tempfile.TemporaryDirectory() as tmp:
            for name in args.only.split(","):
                print(f"{name}:", flush=True)
                start()  # a fresh server per app: roles such as anon are server-wide
                checks[name], rs = study(name, Path(tmp), args.svetra)
                results += rs
    finally:
        sh("docker", "rm", "-f", CONTAINER, check=False)
    (out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    (out / "summary.md").write_text(summary(checks, results))
    print(f"wrote {out / 'results.json'} and {out / 'summary.md'}")


if __name__ == "__main__":
    main()
