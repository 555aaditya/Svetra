#!/usr/bin/env python3
"""Check on a real PostgreSQL 16 that the four bookings update-policy survivors are not equivalent
or masked, as bookings_spike.py (Z3) suggested.

  python3 verify_bookings.py --svetra PATH/TO/svetra --repo PATH/TO/Svetra-checkout ../round1/results.json

For each of the four mutants it runs two updates as a signed-in person and prints whether the
original policies and the mutant allow them. Needs Docker. Uses a throwaway container.
"""
import argparse, json, os, subprocess, tempfile

ap = argparse.ArgumentParser()
ap.add_argument("results"); ap.add_argument("--svetra", required=True); ap.add_argument("--repo", required=True)
a = ap.parse_args()
NAME = "svetra-z3-verify"
def sh(*c, **k): return subprocess.run(c, capture_output=True, text=True, **k)
def psql(sql, db="spike", must=True):
    r = sh("docker", "exec", "-i", NAME, "psql", "-U", "postgres", "-d", db, "-qAt", "-v", "ON_ERROR_STOP=1", input=sql)
    if must and r.returncode and "violates row-level" not in r.stderr:
        raise SystemExit("psql failed: " + r.stderr.strip()[:300])
    return r

work = tempfile.mkdtemp()
open(work + "/svetra.yaml", "w").write(open(os.path.join(a.repo, "tests/fixtures/bookings.yaml")).read())
sh(a.svetra, "compile", work + "/svetra.yaml")
schema = open(work + "/supabase/schemas/svetra.sql").read()
sh("docker", "rm", "-f", NAME)
sh("docker", "run", "-d", "--name", NAME, "-e", "POSTGRES_PASSWORD=x", "postgres:16")
import time
ok = 0
while ok < 3:   # the image restarts the server once after its first start; wait for it to stay up
    ok = ok + 1 if sh("docker", "exec", NAME, "psql", "-U", "postgres", "-Atc", "select 1").returncode == 0 else 0
    time.sleep(1)
psql("create database spike", "postgres")
psql(open(os.path.join(a.repo, "src/standin/base.sql")).read()); psql(schema)
A, B, C = ("10000000-0000-0000-0000-00000000000a", "10000000-0000-0000-0000-00000000000b", "20000000-0000-0000-0000-000000000001")
U1, U2 = "00000000-0000-0000-0000-00000000000b", "00000000-0000-0000-0000-00000000000c"   # staff at A; customer and staff at B
psql(f"""insert into auth.users(id) values ('{U1}'),('{U2}'),('00000000-0000-0000-0000-00000000000a');
insert into public.location(id,name) values ('{A}','A'),('{B}','B');
insert into svetra.role_grants(user_id,role,scope_id) values ('{U1}','staff','{A}'),('{U2}','staff','{B}');
insert into public.booking(id,customer,location,starts) values ('{C}','{U2}','{A}',now());""")
ORIG = """alter policy "admins_manage_all_update" on public.booking using ((select "svetra"."has_role"('admin'))) with check ((select "svetra"."has_role"('admin')));
alter policy "staff_manage_local_update" on public.booking using ("svetra"."has_role"('staff', "location")) with check ("svetra"."has_role"('staff', "location"));"""
def update(who, extra):
    r = psql(f"""begin; set local role authenticated; select set_config('request.jwt.claim.sub','{who}',true) \\gset
update public.booking set location='{B}'{extra} where id='{C}' returning 'ALLOWED'; rollback;""")
    return "ALLOWED" if "ALLOWED" in r.stdout else "denied"
SCEN = {"staff at A moves a booking at A to B and makes themselves the customer": (U1, f", customer='{U1}'"),
        "customer who is staff at B moves their own booking (at A) to B": (U2, "")}
rs = json.load(open(a.results))
want = [("policy-using-true", "booking.admins_manage_all_update"), ("policy-check-true", "booking.admins_manage_all_update"),
        ("policy-using-true", "booking.staff_manage_local_update"), ("policy-check-true", "booking.staff_manage_local_update")]
for name, (who, extra) in SCEN.items():
    psql(ORIG); print(f"\n{name}\n  original: {update(who, extra)}")
    for op, t in want:
        sql = next(r["sql"] for r in rs if r["app"] == "bookings" and r["operator"] == op and r["target"] == t)
        psql(ORIG); psql(sql); print(f"  {op} {t}: {update(who, extra)}"); psql(ORIG)
sh("docker", "rm", "-f", NAME)
