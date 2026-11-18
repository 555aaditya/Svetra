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
        for page in range(1, PAGES_PER_QUERY + 1):
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


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("find")
    args = p.parse_args()
    {"find": find}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
