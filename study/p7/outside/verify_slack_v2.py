#!/usr/bin/env python3
"""Check on PostgreSQL 16 that the survivors of the outside_slack_v2 study, classified equivalent or
masked, really change nothing.

  python3 study/p7/outside/verify_slack_v2.py --svetra target/debug/svetra study/p7/outside/v2/results.json

For the original database and for each surviving mutant it runs the same battery as six people
(anon, three signed-in people with no role, an admin, a moderator): every action on rows they own
and rows they don't, including taking a row over by changing its owner column. It prints the cells
where a mutant's outcome differs from the original's. No differing cell means no one gets or loses
anything under the tested battery. The battery is finite; it is evidence, not a proof. Needs Docker;
uses a throwaway container.
"""
import argparse, json, subprocess, tempfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
ap = argparse.ArgumentParser()
ap.add_argument("results")
ap.add_argument("--svetra", required=True)
a = ap.parse_args()
NAME = "svetra-outside-v2-verify"


def sh(*c, **k):
    return subprocess.run(c, capture_output=True, text=True, **k)


def psql(sql, must=True):
    r = sh("docker", "exec", "-i", NAME, "psql", "-U", "postgres", "-d", "postgres", "-qAt",
           "-v", "ON_ERROR_STOP=1", input=sql)
    if must and r.returncode:
        raise SystemExit("psql failed: " + r.stderr.strip()[:400])
    return r


work = tempfile.mkdtemp()
Path(work, "svetra.yaml").write_text((ROOT / "tests/fixtures/outside_slack_v2.yaml").read_text())
sh(a.svetra, "compile", work + "/svetra.yaml")
schema = Path(work, "supabase/schemas/svetra.sql").read_text()
sh("docker", "rm", "-f", NAME)
sh("docker", "run", "-d", "--name", NAME, "-e", "POSTGRES_PASSWORD=x", "postgres:16")
ok = 0
while ok < 3:  # the image restarts the server once after its first start; wait for it to stay up
    ok = ok + 1 if sh("docker", "exec", NAME, "psql", "-U", "postgres", "-Atc", "select 1").returncode == 0 else 0
    time.sleep(1)
psql((ROOT / "src/standin/base.sql").read_text())
psql(schema)

P = {n: f"00000000-0000-4000-8000-00000000000{i}" for i, n in enumerate(["alice", "bob", "carol", "admin", "mod"], 1)}
CH = {"alice": "c0000000-0000-4000-8000-00000000000a", "bob": "c0000000-0000-4000-8000-00000000000b"}
MSG = {"alice": "d0000000-0000-4000-8000-00000000000a", "bob": "d0000000-0000-4000-8000-00000000000b"}
PRO = {n: P[n] for n in CH}  # a profile is keyed by the auth user id
psql("insert into auth.users(id) values " + ",".join(f"('{u}')" for u in P.values()) + ";\n"
     f"insert into svetra.role_grants(user_id, role, scope_id) values ('{P['admin']}', 'admin', null), ('{P['mod']}', 'moderator', null);\n"
     + "".join(f"insert into public.channel(id, slug, created_by) values ('{CH[n]}', '{n}', '{P[n]}');\n"
               f"insert into public.message(id, message, user_id, channel_id) values ('{MSG[n]}', 'hi', '{P[n]}', '{CH[n]}');\n"
               f"insert into public.profile(id, username) values ('{P[n]}', '{n}');\n" for n in CH))

HELPER = """create or replace function pg_temp.try(p_role text, p_user uuid, p_sql text) returns text language plpgsql as $$
declare n bigint;
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role, 'aal', 'aal1')::text, true);
    execute p_sql;
    get diagnostics n = row_count;
    raise exception 'undo';
  exception when others then
    if sqlerrm = 'undo' then return case when n > 0 then 'allowed' else 'denied' end; end if;
    if sqlstate = '42501' then return 'denied'; end if;
    return 'error ' || sqlstate;
  end;
end $$;
"""


def battery():
    cells = []
    people = [("anon", "anon", None)] + [(n, "authenticated", P[n]) for n in P]
    for who, role, uid in people:
        me = uid or P["alice"]
        other = P["bob"] if who != "bob" else P["alice"]
        k = 0
        def add(label, sql):
            nonlocal k
            k += 1
            cells.append((f"{who}: {label}", role, uid, sql))
        for t, ids, owner in [("channel", CH, "created_by"), ("message", MSG, "user_id"), ("profile", PRO, "id")]:
            add(f"select {t}", f"select 1 from public.{t}")
            extra = {"channel": "slug", "message": "channel_id", "profile": "username"}[t]
            vals = {"channel": lambda n: f"'s{n}'", "message": lambda n: f"'{CH['alice']}'", "profile": lambda n: f"'u{n}'"}[t]
            for whose, owner_id in [("own", me), ("other's", other)] + ([("a free person's", P["carol"])] if t == "profile" else []):
                if t == "profile":
                    add(f"insert {t} as {whose} owner", f"insert into public.{t}(id, {extra}) values ('{owner_id}', {vals(k)})")
                else:
                    add(f"insert {t} as {whose} owner",
                        f"insert into public.{t}(id, {owner}, {extra}) values (gen_random_uuid(), '{owner_id}', {vals(k)})")
            for row in ("alice", "bob"):
                add(f"update {t} of {row}", f"update public.{t} set {extra} = {extra} where id = '{ids[row]}'")
                add(f"take over {t} of {row}", f"update public.{t} set {owner} = '{me}' where id = '{ids[row]}'")
                add(f"give away {t} of {row}", f"update public.{t} set {owner} = '{other}' where id = '{ids[row]}'")
                if t == "profile":  # carol has no profile: the key a person could take or be given
                    add(f"give {t} of {row} to a free person", f"update public.{t} set {owner} = '{P['carol']}' where id = '{ids[row]}'")
                add(f"delete {t} of {row}", f"delete from public.{t} where id = '{ids[row]}'")
        add("write role_grants", f"insert into svetra.role_grants(user_id, role, scope_id) values ('{me}', 'admin', null)")
        add("read role_grants", "select 1 from svetra.role_grants")
    return cells


CELLS = battery()


def run(mutation):
    rows = []
    for l, r, u, s in CELLS:
        rows.append(f"select 'RES|{l.replace(chr(39), '')}|' || pg_temp.try('{r}', {('%s::uuid' % repr(u)) if u else 'null::uuid'}, $q${s}$q$);")
    sql = "begin;\n" + HELPER + (mutation + "\n" if mutation else "") + "\n".join(rows) + "\nrollback;\n"
    out = psql(sql).stdout
    return dict(line[4:].rsplit("|", 1) for line in out.splitlines() if line.startswith("RES|"))


base = run(None)
print(f"battery: {len(base)} cells; allowed in the original: {sum(v == 'allowed' for v in base.values())}")
rs = [r for r in json.load(open(a.results)) if r["outcome"] == "survived"]
bad = 0
for r in rs:
    got = run(r["sql"])
    diff = [(k, base[k], got[k]) for k in base if base[k] != got.get(k)]
    bad += bool(diff)
    print(f"{r['operator']:22} {r['target']:36} {'DIFFERS in ' + str(len(diff)) + ' cells' if diff else 'same as original'}")
    for k, was, now in diff:
        print(f"    {k}: original {was}, mutant {now}")
sh("docker", "rm", "-f", NAME)
print(f"\n{len(rs) - bad} of {len(rs)} survivors behave exactly like the original on this battery.")
