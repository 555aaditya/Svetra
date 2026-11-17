#!/usr/bin/env python3
"""Agent evaluation: does an agent build a backend that enforces who can see what?

For each (prompt, condition, seed) it starts a fresh directory, runs headless Claude Code on the
prompt, then checks what the agent produced on a real PostgreSQL, deterministically:

  * the schema applies, and the contract in the prompt holds (the oracle's test data inserts);
  * every (person, operation, row) in oracle/<prompt>.json is allowed or denied as expected,
    run as real database roles, never the superuser;
  * `svetra check` finds no critical leak in what the agent built.

Conditions:  alone    an empty folder
             careful  an empty folder, and the prompt adds a plain security reminder
             svetra   `svetra init` was run (skill + MCP server) and the prompt doesn't mention it

A prompt with a `<name>.followup.md` is a second conversation turn on the same folder (a
change request), checked against the database as it was after the first turn, with data in it.

See eval/agent/README.md for the design, what this does and doesn't show, and how to run it.
"""
import argparse, json, os, re, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
CONTAINER = "svetra-eval-pg"
TEMPLATE = "evaltpl"
UUID = re.compile(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
CAREFUL = ("\n\nSecurity matters here: make sure each person can only see and change what they should, "
           "and that nobody else can.")

# Runs a statement as one person and undoes it. A refusal is an error from the rule (insufficient
# privilege, or a check or trigger the app wrote); an error from a missing table or column is not a
# refusal, and is reported as such so a wrong name can't look like a denial.
HELPERS = """
create schema eval;
create function eval.try(p_role text, p_user uuid, p_sql text, p_call boolean default false) returns text
language plpgsql as $$
declare n bigint;
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role, 'aal', 'aal1')::text, true);
    execute p_sql;
    get diagnostics n = row_count;
    raise exception 'eval_undo';
  exception when others then
    if sqlerrm = 'eval_undo' then
      return case when p_call or n > 0 then 'allowed' else 'denied' end;
    elsif sqlstate = '42501' or sqlstate like 'P0%' or sqlstate like '23%' or sqlstate like '22%' or sqlstate = '44000' then
      return 'denied';
    end if;
    return 'error ' || sqlstate || ': ' || sqlerrm;
  end;
end $$;
"""


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def psql(db, sql, check=False):
    return subprocess.run(
        ["docker", "exec", "-i", CONTAINER, "psql", "-U", "postgres", "-d", db, "-X", "-A", "-t", "-q"]
        + (["-v", "ON_ERROR_STOP=1"] if check else []),
        input=sql, capture_output=True, text=True,
    )


def ensure_postgres():
    if not sh(["docker", "ps", "-q", "-f", f"name={CONTAINER}"]).stdout.strip():
        sh(["docker", "rm", "-f", CONTAINER])
        r = sh(["docker", "run", "-d", "--name", CONTAINER, "-e", "POSTGRES_PASSWORD=pw", "postgres:16"])
        if r.returncode:
            sys.exit(f"cannot start postgres: {r.stderr}")
    for _ in range(90):
        if sh(["docker", "exec", CONTAINER, "pg_isready", "-U", "postgres"]).returncode == 0 and \
           psql("postgres", "select 1").returncode == 0:
            break
        time.sleep(1)
    # The template is rebuilt whenever the stub changes, so old containers don't go stale.
    import hashlib
    stub = open(os.path.join(REPO, "tests", "pg", "supabase_stub.sql")).read()
    tag = hashlib.md5(stub.encode()).hexdigest()[:10]
    have = psql("postgres", f"select shobj_description(oid, 'pg_database') from pg_database where datname = '{TEMPLATE}'").stdout.strip()
    if have != tag:
        psql("postgres", f"drop database if exists {TEMPLATE}")
        psql("postgres", f"create database {TEMPLATE}", check=True)
        # Roles belong to the whole server and may already exist from an earlier run.
        if psql("postgres", "select 1 from pg_roles where rolname = 'anon'").stdout.strip():
            stub = "\n".join(l for l in stub.splitlines() if not l.startswith("create role"))
        r = psql(TEMPLATE, stub, check=True)
        if r.returncode:
            sys.exit(f"stub failed: {r.stderr}")
        psql("postgres", f"comment on database {TEMPLATE} is '{tag}'")


def find_schema(work):
    """The SQL the agent produced: migrations first, then Svetra's generated schema."""
    found = []
    for sub in ("supabase/migrations", "supabase/schemas"):
        d = os.path.join(work, sub)
        if os.path.isdir(d):
            found += [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith(".sql")]
        if found:
            break
    return found


def tools_for(condition, mcp=None):
    base = ["Read", "Write", "Edit", "Glob", "Grep"]
    if condition == "svetra":
        return ["mcp__svetra", "Bash(svetra *)", "Bash(npx -y svetra*)"] + base
    return base


def run_agent(work, prompt, condition, model, budget, svetra_bin, timeout, init=True):
    env = dict(os.environ)
    cmd = ["claude", "-p", prompt + (CAREFUL if condition == "careful" else ""), "--output-format", "stream-json",
           "--verbose", "--model", model, "--permission-mode", "acceptEdits", "--setting-sources", "project",
           "--strict-mcp-config", "--no-session-persistence", "--max-budget-usd", str(budget)]
    if condition == "svetra":
        if init:
            r = sh([svetra_bin, "init", work])
            if r.returncode:
                return {"error": f"svetra init failed: {r.stderr}"}, ""
            mcp = os.path.join(work, ".mcp.json")
            cfg = json.load(open(mcp))
            cfg["mcpServers"]["svetra"] = {"command": os.path.abspath(svetra_bin), "args": ["mcp"]}
            json.dump(cfg, open(mcp, "w"), indent=2)
        # `svetra` on the PATH is the binary under test, so the skill's CLI calls use it too.
        shim = os.path.join(work, "..", "bin")
        os.makedirs(shim, exist_ok=True)
        link = os.path.join(shim, "svetra")
        if not os.path.lexists(link):
            os.symlink(os.path.abspath(svetra_bin), link)
        env["PATH"] = os.path.abspath(shim) + os.pathsep + env["PATH"]
        cmd += ["--mcp-config", os.path.join(work, ".mcp.json")]
    cmd += ["--allowedTools"] + tools_for(condition)
    started = time.time()
    try:
        r = subprocess.run(cmd, cwd=work, capture_output=True, text=True, timeout=timeout, env=env)
        out, err, timed_out = r.stdout, r.stderr, False
    except subprocess.TimeoutExpired as e:
        out = e.stdout.decode() if isinstance(e.stdout, bytes) else (e.stdout or "")
        err, timed_out = "timeout", True
    wall = time.time() - started
    tools, final = {}, {}
    for line in out.splitlines():
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        if ev.get("type") == "assistant":
            for c in ev.get("message", {}).get("content", []):
                if c.get("type") == "tool_use":
                    tools[c["name"]] = tools.get(c["name"], 0) + 1
        if ev.get("type") == "result":
            final = ev
    usage = final.get("usage", {})
    answer = final.get("result") or ""
    # A run that failed for an outside reason says nothing about the agent: it is not scored.
    invalid = None
    if timed_out:
        invalid = "timed out"
    elif not final:
        invalid = "no result: " + (err or out)[-200:]
    elif final.get("is_error") and re.search(r"limit|rate|overload|credit|quota|529|429", answer + err, re.I):
        invalid = "the service refused the run: " + answer[:160]
    return {
        "invalid": invalid, "wall_s": round(wall, 1), "is_error": final.get("is_error"),
        "num_turns": final.get("num_turns"), "cost_usd": final.get("total_cost_usd"),
        "input_tokens": usage.get("input_tokens"), "output_tokens": usage.get("output_tokens"),
        "cache_read_tokens": usage.get("cache_read_input_tokens"),
        "cache_creation_tokens": usage.get("cache_creation_input_tokens"),
        "tools": tools, "stderr": err[-400:] if err else "", "answer": answer[:500],
    }, out


def seed_sql(statements):
    """Test rows go in with triggers off: seeding must not pass through the app's own rules (an app
    that says "a new document is always a draft" would otherwise refuse the rows that set up an
    approved one). Row-level security doesn't apply to the superuser either."""
    return "set session_replication_role = replica;\n" + ";\n".join(statements) + ";\nset session_replication_role = origin;"


def subst(text, ids):
    return re.sub(r"\{(\w+)\}", lambda m: ids.get(m.group(1), m.group(0)), text)


def artifacts(work):
    """What only a Svetra run leaves behind: a design, generated access tests, an explanation."""
    tests = 0
    d = os.path.join(work, "supabase", "tests")
    if os.path.isdir(d):
        for f in os.listdir(d):
            if f.endswith(".sql"):
                tests += open(os.path.join(d, f)).read().count("select is(")
    design = os.path.join(work, "svetra.yaml")
    text = open(design).read() if os.path.exists(design) else ""
    return {
        "svetra_yaml": bool(text),
        "generated_access_tests": tests,
        # What the design needed beyond policies: hand-written functions, and policies that say what
        # an update turns a row into.
        "holes": len(re.findall(r"^\s+kind:\s*function", text, re.M)),
        "checks": len(re.findall(r"^\s+check:", text, re.M)),
        "restrictive": len(re.findall(r"^\s+restrictive:\s*true", text, re.M)),
        "only": len(re.findall(r"^\s+only:", text, re.M)),
        # A hand-written trigger in the migration: the rule is outside the design.
        "hand_written_triggers": sum(
            open(os.path.join(work, "supabase", "migrations", f)).read().count("create trigger")
            for f in (os.listdir(os.path.join(work, "supabase", "migrations")) if os.path.isdir(os.path.join(work, "supabase", "migrations")) else [])
            if f.endswith(".sql")
        ) - (1 if "only:" in text else 0),
    }


def check(work, oracle, svetra_bin, name, applied_before=None):
    """Apply the agent's schema, run the oracle as real roles, then svetra check.

    With `applied_before` (the files from the first turn of a two-turn prompt), those are applied
    first and the oracle's `seed` rows are inserted, then only the files that are new."""
    n_checks = len(oracle["checks"]) + len(oracle.get("data_checks", []))
    res = {"schema_files": [], "applied": False, "setup_ok": False, "checks": [], "leaks": 0, "too_strict": 0,
           "passed": 0, "total": n_checks, "svetra_check": None, "artifacts": artifacts(work)}
    files = find_schema(work)
    res["schema_files"] = [os.path.relpath(f, work) for f in files]
    if not files:
        res["error"] = "no schema produced"
        return res
    first = [f for f in files if applied_before is not None and os.path.relpath(f, work) in applied_before]
    later = [f for f in files if f not in first] if applied_before is not None else files
    db = "run_" + re.sub(r"\W", "_", name)
    psql("postgres", f"drop database if exists {db}")
    psql("postgres", f"create database {db} template {TEMPLATE}", check=True)

    def apply(group):
        for f in group:
            r = psql(db, open(f).read(), check=True)
            if r.returncode:
                res["error"] = "schema did not apply: " + r.stderr.strip()[-300:]
                return False
        return True

    ids = {}
    users = ", ".join(f"('{u}', '{n}@eval.test')" for n, u in oracle["users"].items())
    if applied_before is not None:
        if not apply(first):
            return res
        psql(db, f"insert into auth.users (id, email) values {users};\n", check=True)
        r = psql(db, seed_sql(oracle["seed"]), check=True)
        if r.returncode:
            res["error"] = "the data contract in the prompt doesn't hold: " + r.stderr.strip()[-300:]
            return res
        if not apply(later):
            return res
        res["applied"] = True
    else:
        if not apply(files):
            return res
        res["applied"] = True
        psql(db, f"insert into auth.users (id, email) values {users};\n", check=True)
    steps = [subst(s, ids) for s in oracle.get("setup", [])]
    if steps:
        r = psql(db, seed_sql(steps), check=True)
        if r.returncode:
            res["error"] = "the data contract in the prompt doesn't hold: " + r.stderr.strip()[-300:]
            return res
    # Things done through the app's own operations, as real people; their results are kept.
    for step in oracle.get("as_users", []):
        who = step["as"]
        role = "anon" if who == "anon" else "authenticated"
        uid = "" if who == "anon" else oracle["users"][who]
        script = (f"set role {role}; select set_config('request.jwt.claim.sub', '{uid}', false), "
                  f"set_config('request.jwt.claims', json_build_object('sub', nullif('{uid}', ''), 'role', '{role}', "
                  f"'aal', 'aal1')::text, false);\n{subst(step['sql'], ids)} as cap \\gset\nreset role;\n\\echo CAP :cap\n")
        r = psql(db, script, check=True)
        m = UUID.search(r.stdout.split("CAP", 1)[-1]) if "CAP" in r.stdout else None
        if r.returncode or (step.get("capture") and not m):
            res["error"] = "the data contract in the prompt doesn't hold: " + (r.stderr.strip() or r.stdout.strip())[-300:]
            return res
        if step.get("capture"):
            ids[step["capture"]] = m.group(0)
    after = [subst(s, ids) for s in oracle.get("after_setup", [])]
    if after:
        r = psql(db, seed_sql(after), check=True)
        if r.returncode:
            res["error"] = "the data contract in the prompt doesn't hold: " + r.stderr.strip()[-300:]
            return res
    res["setup_ok"] = True
    psql(db, HELPERS, check=True)
    lines = []
    for i, c in enumerate(oracle["checks"]):
        what, who, sql, want = c[:4]
        call = len(c) > 4 and c[4] == "call"
        uid = "null" if who == "anon" else f"'{oracle['users'][who]}'"
        role = "anon" if who == "anon" else "authenticated"
        lines.append(f"select {i}, eval.try('{role}', {uid}, $e${subst(sql, ids)}$e$, {str(call).lower()});")
    got = {}
    for line in psql(db, "\n".join(lines)).stdout.splitlines():
        if "|" in line:
            i, v = line.split("|", 1)
            got[int(i)] = v.strip()
    for i, c in enumerate(oracle["checks"]):
        what, want = c[0], c[3]
        g = got.get(i, "no result")
        ok = g == want
        res["checks"].append({"check": what, "want": want, "got": g, "ok": ok})
        res["passed"] += ok
        if not ok and want == "denied" and g == "allowed":
            res["leaks"] += 1
        if not ok and want == "allowed" and g == "denied":
            res["too_strict"] += 1
    for what, sql, want in oracle.get("data_checks", []):
        out = psql(db, subst(sql, ids)).stdout.strip()
        ok = out == want
        res["checks"].append({"check": what, "want": want, "got": out, "ok": ok})
        res["passed"] += ok
    q = sh([svetra_bin, "check", "--print-query"]).stdout
    snap_file = os.path.join(tempfile.mkdtemp(prefix="svetra-eval-"), "snapshot.json")
    open(snap_file, "w").write(psql(db, q).stdout)
    c = sh([svetra_bin, "check", "--snapshot", snap_file, "--json"])
    try:
        res["svetra_check"] = json.loads(c.stdout)["counts"]
    except ValueError:
        res["svetra_check"] = {"error": c.stderr[-200:]}
    psql("postgres", f"drop database if exists {db}")
    return res


def selftest(svetra_bin):
    """The checker must score correct schemas 100% and catch each broken one (no agents run)."""
    full = lambda r: r["passed"] == r["total"] and r["leaks"] == 0 and r["svetra_check"]["critical"] == 0
    # (name, files in order, oracle, first-turn files or None, expectation)
    cases = [
        ("notes_good", ["notes_good"], "notes", None, full),
        ("notes_rls_off", ["notes_rls_off"], "notes", None, lambda r: r["leaks"] > 0 and r["svetra_check"]["critical"] >= 1),
        ("notes_open_read", ["notes_open_read"], "notes", None, lambda r: r["leaks"] >= 3),
        # An update policy with `with check (true)` is not a leak here: PostgreSQL still applies the select
        # policy to the updated row, so ownership still holds (a "masked" mutant in the P7 study).
        ("notes_open_insert", ["notes_open_insert"], "notes", None, lambda r: r["leaks"] == 1 and r["total"] - r["passed"] == 1),
        ("posts_good", ["posts_good"], "posts", None, full),
        ("posts_too_strict", ["posts_too_strict"], "posts", None, lambda r: r["too_strict"] >= 2 and r["leaks"] == 0),
        ("teams_good", ["teams_good"], "teams", None, full),
        ("teams_no_rls", ["teams_no_rls"], "teams", None, lambda r: r["leaks"] >= 3),
        ("teams_open_add_member", ["teams_open_add_member"], "teams", None, lambda r: r["leaks"] >= 1),
        ("approvals_good", ["approvals_good"], "approvals", None, full),
        ("approvals_no_trigger", ["approvals_no_trigger"], "approvals", None, lambda r: r["leaks"] == 1),
        ("softdelete_good", ["softdelete_good"], "softdelete", None, full),
        ("softdelete_no_function", ["softdelete_no_function"], "softdelete", None, lambda r: r["leaks"] == 0 and any(c["got"].startswith("error") for c in r["checks"])),
        ("rename_good", ["notes_good", "rename_good"], "rename", ["notes_good"], full),
        ("rename_destructive", ["notes_good", "rename_destructive"], "rename", ["notes_good"],
         lambda r: r["passed"] < r["total"] and any(c["check"].startswith("the old note") and not c["ok"] for c in r["checks"])),
    ]
    bad = 0
    for name, order, prompt, first, ok in cases:
        work = os.path.join(tempfile.mkdtemp(prefix="svetra-eval-self-"), "work")
        os.makedirs(os.path.join(work, "supabase", "migrations"))
        names = []
        for i, f in enumerate(order, 1):
            dest = f"supabase/migrations/{i:03d}_{f}.sql"
            shutil.copy(os.path.join(HERE, "selftest", f + ".sql"), os.path.join(work, dest))
            names.append(dest)
        before = names[:len(first)] if first else None
        r = check(work, json.load(open(os.path.join(HERE, "oracle", prompt + ".json"))), svetra_bin, "self_" + name, before)
        good = bool(r.get("applied")) and ok(r)
        bad += not good
        print(f"{'ok  ' if good else 'FAIL'} {name}: {r['passed']}/{r['total']} passed, {r['leaks']} leaks, "
              f"{r['too_strict']} too strict, svetra check {r['svetra_check']}")
        if not good:
            print("     ", r.get("error"), [c for c in r["checks"] if not c["ok"]][:5])
    sys.exit(1 if bad else 0)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--prompts", default="notes,posts")
    ap.add_argument("--conditions", default="alone,svetra")
    ap.add_argument("--seeds", type=int, default=1)
    ap.add_argument("--model", default="claude-sonnet-5-5")
    ap.add_argument("--budget", type=float, default=3.0, help="max USD per run (per turn)")
    ap.add_argument("--timeout", type=int, default=1200)
    ap.add_argument("--svetra", default=os.path.join(REPO, "target", "debug", "svetra"))
    ap.add_argument("--out", default=os.path.join(HERE, "results", time.strftime("%Y%m%d-%H%M%S")))
    ap.add_argument("--check-only", help="re-check an existing results folder without running agents")
    ap.add_argument("--selftest", action="store_true", help="validate the checker on hand-written schemas; runs no agents")
    a = ap.parse_args()
    if not os.path.exists(a.svetra):
        sys.exit(f"build svetra first: cargo build ({a.svetra} missing)")
    ensure_postgres()
    if a.selftest:
        return selftest(a.svetra)
    out = os.path.abspath(a.check_only or a.out)
    os.makedirs(out, exist_ok=True)
    rows = []
    for prompt in a.prompts.split(","):
        text = open(os.path.join(HERE, "prompts", prompt + ".md")).read().strip()
        fup = os.path.join(HERE, "prompts", prompt + ".followup.md")
        followup = open(fup).read().strip() if os.path.exists(fup) else None
        oracle = json.load(open(os.path.join(HERE, "oracle", prompt + ".json")))
        for cond in a.conditions.split(","):
            for seed in range(1, a.seeds + 1):
                name = f"{prompt}-{cond}-{seed}"
                base = os.path.join(out, name)
                work = os.path.join(base, "work")
                before = None
                if a.check_only:
                    saved = json.load(open(os.path.join(base, "run.json")))
                    meta, before = saved.get("agent", {}), saved.get("applied_before")
                else:
                    shutil.rmtree(base, ignore_errors=True)
                    os.makedirs(work)
                    print(f"running {name} ...", flush=True)
                    meta, transcript = run_agent(work, text, cond, a.model, a.budget, a.svetra, a.timeout)
                    open(os.path.join(base, "transcript.jsonl"), "w").write(transcript)
                    if followup and not meta.get("invalid"):
                        before = [os.path.relpath(f, work) for f in find_schema(work)]
                        m2, t2 = run_agent(work, followup, cond, a.model, a.budget, a.svetra, a.timeout, init=False)
                        open(os.path.join(base, "transcript-turn2.jsonl"), "w").write(t2)
                        meta = {**meta, "turn2": m2, "invalid": m2.get("invalid"),
                                "cost_usd": (meta.get("cost_usd") or 0) + (m2.get("cost_usd") or 0),
                                "num_turns": (meta.get("num_turns") or 0) + (m2.get("num_turns") or 0),
                                "output_tokens": (meta.get("output_tokens") or 0) + (m2.get("output_tokens") or 0),
                                "tools": {k: meta["tools"].get(k, 0) + m2["tools"].get(k, 0) for k in {*meta["tools"], *m2["tools"]}}}
                if meta.get("invalid"):
                    res = {"invalid": meta["invalid"]}
                    print(f"  {name}: INVALID, not scored ({meta['invalid']})", flush=True)
                else:
                    res = check(work, oracle, a.svetra, name, before)
                    print(f"  {name}: {res['passed']}/{res['total']} checks, {res['leaks']} leaks, "
                          f"{res['too_strict']} too strict, svetra check {res['svetra_check']}, "
                          f"${meta.get('cost_usd')}, {meta.get('num_turns')} turns", flush=True)
                json.dump({"prompt": prompt, "condition": cond, "seed": seed, "model": a.model,
                           "svetra": sh([a.svetra, "--version"]).stdout.strip(), "agent": meta,
                           "applied_before": before, "check": res}, open(os.path.join(base, "run.json"), "w"), indent=1)
                rows.append((name, meta, res))
    print("\n| run | applied | passed | leaks | too strict | critical | cost | turns | output tokens | svetra tools | access tests |")
    print("|---|---|---|---|---|---|---|---|---|---|---|")
    for name, m, r in rows:
        if "invalid" in r:
            print(f"| {name} | invalid | | | | | | | | | |")
            continue
        crit = (r["svetra_check"] or {}).get("critical", "?")
        st = sum(v for k, v in (m.get("tools") or {}).items() if k.startswith("mcp__svetra"))
        print(f"| {name} | {r['applied']} | {r['passed']}/{r['total']} | {r['leaks']} | {r['too_strict']} | {crit} | "
              f"${(m.get('cost_usd') or 0):.2f} | {m.get('num_turns')} | {m.get('output_tokens')} | {st} | "
              f"{r['artifacts']['generated_access_tests']} |")
    json.dump([{"run": n, "agent": m, "check": r} for n, m, r in rows], open(os.path.join(out, "summary.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
