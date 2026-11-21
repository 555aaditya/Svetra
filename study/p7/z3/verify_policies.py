#!/usr/bin/env python3
"""Check on a real PostgreSQL 16 what policies.py (Z3) says about finledger, posts and clinic.

  python3 verify_policies.py --svetra PATH/TO/svetra --repo PATH/TO/Svetra-checkout ../round1/results.json

policies.py found no mutant where it and classify.py disagree, but three survivors of round one are
mutants it says widen access and the tests missed (posts update using-true and check-true,
finledger transaction using-true), and one survivor changes only a hole's own role. This applies each
mutant's exact SQL from results.json to a seeded database and runs the scenario the model's
counterexample suggests, as the original and as the mutant. It also runs three survivors the model
calls "no change", to see that side hold too. Needs Docker; uses a throwaway container.
"""
import argparse, json, os, subprocess, tempfile, time

ap = argparse.ArgumentParser()
ap.add_argument("results"); ap.add_argument("--svetra", required=True); ap.add_argument("--repo", required=True)
a = ap.parse_args()
NAME = "svetra-z3-verify-policies"
def sh(*c, **k): return subprocess.run(c, capture_output=True, text=True, **k)
def psql(sql, db="spike"):
    return sh("docker", "exec", "-i", NAME, "psql", "-U", "postgres", "-d", db, "-qAt", "-v", "ON_ERROR_STOP=1", input=sql)

def start():
    sh("docker", "rm", "-f", NAME)
    sh("docker", "run", "-d", "--name", NAME, "-e", "POSTGRES_PASSWORD=x", "postgres:16")
    ok = 0
    while ok < 3:   # the image restarts the server once after its first start; wait for it to stay up
        ok = ok + 1 if sh("docker", "exec", NAME, "psql", "-U", "postgres", "-Atc", "select 1").returncode == 0 else 0
        time.sleep(1)

def load(app, db, seed):
    work = tempfile.mkdtemp()
    open(work + "/svetra.yaml", "w").write(open(os.path.join(a.repo, f"tests/fixtures/{app}.yaml")).read())
    sh(a.svetra, "compile", work + "/svetra.yaml")
    psql(f"create database {db}", "postgres")
    base = os.path.join(a.repo, "src/standin/base.sql")
    if not os.path.exists(base):
        base = os.path.join(a.repo, "tests/pg/supabase_stub.sql")
    for sql in (open(base).read(), open(work + "/supabase/schemas/svetra.sql").read(), seed):
        r = psql(sql, db)
        if r.returncode:
            raise SystemExit(f"{app}: load failed: {r.stderr.strip()[:300]}")

A, B, C = (f"00000000-0000-0000-0000-00000000000{x}" for x in "abc")
P1, P2 = "10000000-0000-0000-0000-000000000001", "10000000-0000-0000-0000-000000000002"
USERS = f"insert into auth.users(id) values ('{A}'),('{B}'),('{C}');\n"
SEEDS = {
    "posts": USERS + f"insert into public.post(id,author,body) values ('{P1}','{A}','a');",
    "finledger": USERS + f"""insert into public.account(id,owner,currency,balance) values ('{P1}','{A}','USD',10),('{P2}','{B}','USD',10);
insert into public.transaction(source,destination,amount,status) values ('{P1}','{P2}',1,'pending');""",
    "clinic": USERS + f"""insert into public.visit(id,patient,notes) values ('{P1}','{A}','n');
insert into svetra.role_grants(user_id,role) values ('{C}','admin');""",
}

def count(who, stmt, mut=""):
    """Rows the statement touched or returned as `who` ('anon', a uid, or a role name), after the mutant's SQL."""
    if who == "anon":
        role = "set local role anon;"
    elif who[0] == "0":
        role = f"set local role authenticated; select set_config('request.jwt.claim.sub','{who}',true);"
    else:
        role = f"set local role {who};"
    r = psql(f"begin; {mut}\n{role}\n{stmt}\nrollback;")
    if r.returncode:
        return "denied (" + ("row-level security" if "row-level" in r.stderr else r.stderr.strip()[:80]) + ")"
    return [l for l in r.stdout.split("\n") if l.strip()][-1]

def upd(sql): return f"with u as ({sql} returning 1) select count(*) from u;"

SCEN = [  # (app, operator, target, who, statement, what a widening looks like)
    ("posts", "policy-using-true", "post.posts_by_author_update", B, upd(f"update public.post set author='{B}' where id='{P1}'"), "B takes over A's post"),
    ("posts", "policy-check-true", "post.posts_by_author_update", A, upd(f"update public.post set author='{B}' where id='{P1}'"), "A gives their post to B"),
    ("finledger", "policy-using-true", "transaction.owner_reads_own_transactions", C, "select count(*) from public.transaction;", "C, who owns no account, reads the transaction"),
    ("posts", "policy-drop", "audit_log.svetra_hole_write_audit", "svetra_hole_write_audit", upd("insert into public.audit_log(note) values ('x')"), "the hole's role inserts into its own table"),
    # survivors the model says change nothing:
    ("clinic", "policy-using-true", "visit.admins_manage_update", A, upd(f"update public.visit set notes='x' where id='{P1}'"), "a patient edits their own visit (no update rule for patients)"),
    ("clinic", "policy-check-true", "visit.admins_manage_update", A, upd(f"update public.visit set patient='{B}' where id='{P1}'"), "a patient gives their visit away"),
    ("finledger", "policy-to-public", "account.owner_reads_own_accounts", "anon", "select count(*) from public.account;", "anon reads accounts"),
]

rs = json.load(open(a.results))
try:
    for app in dict.fromkeys(s[0] for s in SCEN):
        start()
        load(app, "spike", SEEDS[app])
        for _, op, target, who, stmt, what in [s for s in SCEN if s[0] == app]:
            sql = next(r["sql"] for r in rs if r["app"] == app and r["operator"] == op and r["target"] == target)
            print(f"\n{app} {op} {target}\n  scenario: {what}\n  original: {count(who, stmt)}\n  mutant:   {count(who, stmt, sql)}")
finally:
    sh("docker", "rm", "-f", NAME)

