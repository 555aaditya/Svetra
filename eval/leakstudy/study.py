#!/usr/bin/env python3
"""Leak study harness: run `svetra check` on the migrations of open-source Supabase apps.
See README.md for the rules. Standard library only; needs gh, docker, psql and a built svetra."""
import argparse
import datetime
import hashlib
import json
import os
import random
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "data")

# The frame is whatever these searches return. Written to data/frame.json with the date, so the
# sample can be described exactly.
QUERIES = [
    '"create table" path:supabase/migrations extension:sql',
    '"enable row level security" path:supabase/migrations extension:sql',
    '"create policy" path:supabase/migrations extension:sql',
    '"auth.users" path:supabase/migrations extension:sql',
]
PAGES_PER_QUERY = 10          # the search API stops at 1000 results per query
EXCLUDED_OWNERS = {"supabase", "supabase-community"}
EXCLUDED_NAME_WORDS = ("starter", "template", "example", "tutorial", "boilerplate", "course", "demo")
MAX_AGE_DAYS = 365


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def gh_api(path, params=None):
    """GET from the GitHub API with the gh CLI, waiting out rate limits."""
    cmd = ["gh", "api", "-X", "GET", path]
    for k, v in (params or {}).items():
        cmd += ["-f", f"{k}={v}"]
    for attempt in range(6):
        r = sh(cmd)
        if r.returncode == 0:
            return json.loads(r.stdout)
        if "rate limit" in r.stderr.lower() or "403" in r.stderr or "429" in r.stderr:
            time.sleep(60)
            continue
        return None
    return None


def save(name, obj):
    os.makedirs(DATA, exist_ok=True)
    with open(os.path.join(DATA, name), "w") as f:
        json.dump(obj, f, indent=1, sort_keys=True)


def load(name):
    with open(os.path.join(DATA, name)) as f:
        return json.load(f)


def find(args):
    """Search, then fetch each repo's metadata and apply the exclusions."""
    names = {}
    for q in QUERIES:
        for page in range(1, args.pages + 1):
            res = gh_api("search/code", {"q": q, "per_page": 100, "page": page})
            time.sleep(7)  # code search allows about 10 requests a minute
            items = (res or {}).get("items", [])
            for it in items:
                names[it["repository"]["full_name"]] = True
            print(f"{q[:40]!r} page {page}: {len(items)} files, {len(names)} repos so far", flush=True)
            if len(items) < 100:
                break
    stages = {"found": len(names)}
    keep, now = [], datetime.datetime.now(datetime.timezone.utc)
    for full in sorted(names):
        meta = gh_api(f"repos/{full}")
        if not meta:
            continue
        pushed = datetime.datetime.fromisoformat(meta["pushed_at"].replace("Z", "+00:00"))
        low = meta["name"].lower()
        why = None
        if meta["fork"]:
            why = "fork"
        elif meta["archived"]:
            why = "archived"
        elif meta.get("is_template"):
            why = "template"
        elif meta["owner"]["login"].lower() in EXCLUDED_OWNERS:
            why = "owner"
        elif any(w in low for w in EXCLUDED_NAME_WORDS):
            why = "name"
        elif (now - pushed).days > MAX_AGE_DAYS:
            why = "stale"
        stages[why or "kept"] = stages.get(why or "kept", 0) + 1
        if not why:
            keep.append({
                "full_name": full,
                "licence": (meta.get("license") or {}).get("spdx_id"),
                "size_kb": meta.get("size", 0),
            })
    save("candidates.json", keep)
    save("frame.json", {"date": now.date().isoformat(), "queries": QUERIES, "stages": stages})
    print(json.dumps(stages, indent=1))


# Markers of an AI builder or agent, looked for in a shallow clone. Guesses, so the two groups are
# noisy; the write-up must say so.
AI_MARKERS = [
    ("lovable", [".lovable", "lovable-tagger"]),
    ("bolt", [".bolt", "bolt.new"]),
    ("cursor", [".cursorrules", ".cursor"]),
    ("claude", ["CLAUDE.md", ".claude"]),
    ("agents", ["AGENTS.md"]),
]


def key_of(full_name):
    """A salted hash, so results can be kept and shared without naming the repo."""
    os.makedirs(DATA, exist_ok=True)
    salt_file = os.path.join(DATA, "salt")
    if not os.path.exists(salt_file):
        with open(salt_file, "w") as f:
            f.write(os.urandom(16).hex())
    salt = open(salt_file).read()
    return hashlib.sha256((salt + full_name).encode()).hexdigest()[:10]


def draw(args):
    """A seeded random sample of N from the candidates. Groups are decided at run time, from the
    clone, so the draw is over everything and each group is filled as repos are run."""
    cands = load("candidates.json")
    rng = random.Random(args.seed)
    rng.shuffle(cands)
    save(f"draw-{args.seed}.json", cands[: args.n])
    print(f"drew {min(args.n, len(cands))} of {len(cands)} candidates with seed {args.seed}")


CONTAINER = "svetra-leakstudy"
MAX_FILE_BYTES = 2_000_000
MAX_CLONE_KB = 200_000


def docker_psql(db, extra=(), env=None, stdin=None, timeout=90):
    """psql inside the study container as the superuser; never touches the host's databases."""
    cmd = ["docker", "exec", "-i"]
    for k, v in (env or {}).items():
        cmd += ["-e", f"{k}={v}"]
    cmd += [CONTAINER, "psql", "-U", "postgres", "-d", db, "-X", "-q"] + list(extra)
    return sh(cmd, input=stdin, timeout=timeout)


def start_database(image="postgres:16"):
    """One throwaway PostgreSQL with the Supabase stand-in loaded into a template database.
    No network, no extra privileges, and limits on memory and processes: the migrations are
    strangers' SQL and run as a superuser."""
    sh(["docker", "rm", "-f", CONTAINER])
    r = sh(["docker", "run", "-d", "--name", CONTAINER, "--network", "none",
            "--cap-drop", "ALL", "--cap-add", "CHOWN", "--cap-add", "DAC_OVERRIDE",
            "--cap-add", "FOWNER", "--cap-add", "SETGID", "--cap-add", "SETUID",
            "--security-opt", "no-new-privileges", "--memory", "1g", "--pids-limit", "512",
            "-e", "POSTGRES_PASSWORD=pw", image])
    if r.returncode:
        sys.exit("could not start PostgreSQL: " + r.stderr)
    for _ in range(90):
        if "init process complete" in sh(["docker", "logs", CONTAINER]).stderr + sh(["docker", "logs", CONTAINER]).stdout:
            break
        time.sleep(1)
    for _ in range(60):
        if sh(["docker", "exec", CONTAINER, "pg_isready", "-U", "postgres"]).returncode == 0:
            break
        time.sleep(1)
    stub = open(os.path.join(HERE, "..", "..", "tests", "pg", "supabase_stub.sql")).read()
    docker_psql("postgres", ["-c", "create database tpl"])
    r = docker_psql("tpl", ["-v", "ON_ERROR_STOP=1"], stdin=stub)
    if r.returncode:
        sys.exit("could not load the Supabase stand-in: " + r.stderr)
    docker_psql("postgres", ["-c", "alter database tpl is_template true"])


def stop_database():
    sh(["docker", "rm", "-f", CONTAINER])


def clone(full_name, dest):
    """A shallow clone with symlinks, hooks and prompts off. Nothing in it is ever executed."""
    env = dict(os.environ, GIT_TERMINAL_PROMPT="0")
    r = sh(["git", "-c", "core.symlinks=false", "-c", "core.hooksPath=/dev/null",
            "clone", "--depth", "1", "--no-tags", "--single-branch", "--quiet",
            f"https://github.com/{full_name}.git", dest], env=env, timeout=180)
    return r.returncode == 0


PACKAGE_SIGNS = ("lovable-tagger", "bolt.new")


def markers_in(repo):
    """Which AI builders or agents left a trace: a file or folder, or a package.json mention."""
    try:
        pkg = open(os.path.join(repo, "package.json")).read()
    except OSError:
        pkg = ""
    found = []
    for name, signs in AI_MARKERS:
        if any(os.path.exists(os.path.join(repo, x)) or (x in PACKAGE_SIGNS and x in pkg) for x in signs):
            found.append(name)
    return found


def migration_files(repo):
    folder = os.path.join(repo, "supabase", "migrations")
    out = []
    if os.path.isdir(folder):
        for name in sorted(os.listdir(folder)):
            path = os.path.join(folder, name)
            if name.endswith(".sql") and os.path.isfile(path) and not os.path.islink(path) \
                    and os.path.getsize(path) <= MAX_FILE_BYTES:
                out.append(path)
    return out


def check_database(db, svetra_bin):
    """`svetra check` on the database, through its read-only query, as the eval harness does."""
    q = sh([svetra_bin, "check", "--print-query"]).stdout
    r = docker_psql(db, ["-At"], stdin=q)
    snap = os.path.join(DATA, "snapshot.tmp.json")
    with open(snap, "w") as f:
        f.write(r.stdout)
    c = sh([svetra_bin, "check", "--snapshot", snap, "--json"])
    os.remove(snap)
    try:
        return json.loads(c.stdout)
    except ValueError:
        return None


def study_repo(full_name, files, svetra_bin, markers=None, licence=None):
    """Apply the migrations in order to a fresh database from the template and check it. A file
    that fails is rolled back whole (as `supabase db reset` runs each one) and counted."""
    key = key_of(full_name)
    db = f"r_{key}"
    docker_psql("postgres", ["-c", f"create database {db} template tpl"])
    applied, errors = 0, []
    for path in files:
        try:
            r = docker_psql(db, ["-1", "-v", "ON_ERROR_STOP=1"], env={"PGOPTIONS": "-c statement_timeout=30000"},
                            stdin=open(path, errors="replace").read(), timeout=90)
        except subprocess.TimeoutExpired:
            errors.append("timeout")
            continue
        if r.returncode == 0:
            applied += 1
        else:
            errors.append((r.stderr.strip().splitlines() or ["?"])[-1][:200])
    result = {
        "key": key,
        "group": "ai" if markers else "other",
        "markers": markers or [],
        "licence": licence,
        "files": len(files),
        "applied": applied,
        "status": "none" if applied == 0 else ("clean" if applied == len(files) else "partial"),
    }
    found = check_database(db, svetra_bin) if applied else None
    if found:
        result["counts"] = found["counts"]
        kinds = {}
        for f in found["findings"]:
            if f["severity"] == "critical":
                kinds[f["kind"]] = kinds.get(f["kind"], 0) + 1
        result["critical_kinds"] = kinds
        # The details stay in data/findings, gitignored, for the hand audit.
        os.makedirs(os.path.join(DATA, "findings"), exist_ok=True)
        with open(os.path.join(DATA, "findings", key + ".json"), "w") as out:
            json.dump({"findings": found["findings"], "errors": errors[:5]}, out, indent=1)
    docker_psql("postgres", ["-c", f"drop database if exists {db}"])
    return result


def run(args):
    sample = load(f"draw-{args.seed}.json")
    svetra_bin = os.path.abspath(args.svetra)
    start_database()
    out_path = os.path.join(DATA, "results.jsonl")
    done = set()
    if os.path.exists(out_path):
        done = {json.loads(l)["key"] for l in open(out_path)}
    try:
        for i, cand in enumerate(sample, 1):
            key = key_of(cand["full_name"])
            if key in done:
                continue
            if cand["size_kb"] > MAX_CLONE_KB:
                row = {"key": key, "status": "skipped_large"}
            else:
                import shutil
                import tempfile
                tmp = tempfile.mkdtemp(prefix="leakstudy-")
                try:
                    repo = os.path.join(tmp, "repo")
                    if not clone(cand["full_name"], repo):
                        row = {"key": key, "status": "clone_failed"}
                    else:
                        files = migration_files(repo)
                        row = study_repo(cand["full_name"], files, svetra_bin, markers_in(repo), cand["licence"]) \
                            if files else {"key": key, "status": "no_migrations"}
                finally:
                    shutil.rmtree(tmp, ignore_errors=True)
            with open(out_path, "a") as f:
                f.write(json.dumps(row, sort_keys=True) + "\n")
            print(f"{i}/{len(sample)} {row['key']} {row['status']} {row.get('counts', '')}", flush=True)
    finally:
        stop_database()


def selftest(args):
    """The harness must find the planted leaks in a leaky app, nothing in a clean one, and report a
    broken migration as partial. Local files only: no network, no GitHub."""
    import shutil
    import tempfile
    svetra_bin = os.path.abspath(args.svetra)
    root = tempfile.mkdtemp(prefix="leakstudy-selftest-")
    fixture = os.path.join(HERE, "..", "..", "tests", "pg", "check", "leaky_app.sql")
    clean = (
        "create table public.notes (id uuid primary key, owner uuid not null, body text);\n"
        "alter table public.notes enable row level security;\n"
        "create policy own on public.notes for all to authenticated "
        "using (owner = auth.uid()) with check (owner = auth.uid());\n"
    )
    apps = {
        "leaky": [open(fixture).read()],
        "clean": [clean],
        "broken": [clean, "create table public.oops (", "select 1;\n"],
    }
    start_database()
    try:
        got = {}
        for name, sqls in apps.items():
            folder = os.path.join(root, name, "supabase", "migrations")
            os.makedirs(folder)
            for n, sql in enumerate(sqls):
                with open(os.path.join(folder, f"{n:04d}_m.sql"), "w") as f:
                    f.write(sql)
            got[name] = study_repo("selftest/" + name, migration_files(os.path.join(root, name)), svetra_bin)
            print(name, got[name]["status"], got[name].get("counts"))
    finally:
        stop_database()
        shutil.rmtree(root, ignore_errors=True)
    problems = []
    if got["leaky"]["counts"]["critical"] != 3 or got["leaky"]["status"] != "clean":
        problems.append("the leaky app should apply cleanly and give 3 critical findings")
    if got["clean"]["counts"]["critical"] != 0 or got["clean"]["status"] != "clean":
        problems.append("the clean app should apply cleanly and give no critical finding")
    if got["broken"]["status"] != "partial" or got["broken"]["applied"] != 2:
        problems.append("the broken migration should be partial, with the other two applied")
    if problems:
        sys.exit("selftest failed:\n  " + "\n  ".join(problems))
    print("selftest passed")


def report(args):
    """Aggregate data/results.jsonl: counts only, never a repo name."""
    rows = [json.loads(l) for l in open(os.path.join(DATA, "results.jsonl"))]
    rows = [r for r in rows if not r["key"].startswith("selftest")]
    total = len(rows)
    by_status = {}
    for r in rows:
        by_status[r["status"]] = by_status.get(r["status"], 0) + 1
    checked = [r for r in rows if "counts" in r]
    print(f"repos run: {total}")
    for k in sorted(by_status):
        print(f"  {k}: {by_status[k]}")
    print(f"checked (at least one migration applied): {len(checked)}")
    for label, sel in (("all", checked), ("ai", [r for r in checked if r["group"] == "ai"]),
                       ("other", [r for r in checked if r["group"] == "other"]),
                       ("clean only", [r for r in checked if r["status"] == "clean"])):
        leaky = [r for r in sel if r["counts"]["critical"] > 0]
        share = f"{100 * len(leaky) / len(sel):.0f}%" if sel else "n/a"
        print(f"  {label}: {len(leaky)} of {len(sel)} have a critical finding ({share})")
    kinds = {}
    for r in checked:
        for k in r.get("critical_kinds", {}):
            kinds[k] = kinds.get(k, 0) + 1
    print("repos with a critical finding, by kind:")
    for k, n in sorted(kinds.items(), key=lambda kv: -kv[1]):
        print(f"  {k}: {n}")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("find")
    f.add_argument("--pages", type=int, default=PAGES_PER_QUERY, help="result pages per search query (100 files each)")
    d = sub.add_parser("draw")
    d.add_argument("--n", type=int, required=True)
    d.add_argument("--seed", type=int, required=True)
    r = sub.add_parser("run")
    r.add_argument("--seed", type=int, required=True)
    r.add_argument("--svetra", default="target/debug/svetra")
    t = sub.add_parser("selftest")
    t.add_argument("--svetra", default="target/debug/svetra")
    sub.add_parser("report")
    args = p.parse_args()
    {"find": find, "draw": draw, "run": run, "selftest": selftest, "report": report}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
