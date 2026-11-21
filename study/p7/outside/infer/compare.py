#!/usr/bin/env python3
"""Compare the infer draft with the hand-written spec and the database's policies.

    python3 -I study/p7/outside/infer/compare.py

Reads snapshot.json, draft.yaml and tests/fixtures/outside_slack.yaml; needs PyYAML.
Prints the numbers the README quotes. Deterministic. Exits 1 if the draft exposes an
entity with no policy (that would be wider than the database).
"""
import json
import pathlib
import re
import sys

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
snap = json.loads((HERE / "snapshot.json").read_text())
draft_text = (HERE / "draft.yaml").read_text()
draft = yaml.safe_load(draft_text)
hand = yaml.safe_load((ROOT / "tests/fixtures/outside_slack.yaml").read_text())

# The hand spec's entity for each table. The spec's table is the snake_case of the entity
# name, so `users` is Profile there. user_roles and role_permissions are Svetra's own roles.
HAND_FOR = {"users": "Profile", "channels": "Channel", "messages": "Message"}
# Each database policy on those tables is one policy of the hand spec.
HAND_POLICY = {
    ("users", "SELECT", "Allow logged-in read access"): "profiles_read",
    ("users", "INSERT", "Allow individual insert access"): "profiles_insert_own",
    ("users", "UPDATE", "Allow individual update access"): "profiles_update_own",
    ("channels", "SELECT", "Allow logged-in read access"): "channels_read",
    ("channels", "INSERT", "Allow individual insert access"): "channels_insert_own",
    ("channels", "DELETE", "Allow individual delete access"): "channels_delete_own",
    ("channels", "DELETE", "Allow authorized delete access"): "channels_delete_admin",
    ("messages", "SELECT", "Allow logged-in read access"): "messages_read",
    ("messages", "INSERT", "Allow individual insert access"): "messages_insert_own",
    ("messages", "UPDATE", "Allow individual update access"): "messages_update_own",
    ("messages", "DELETE", "Allow individual delete access"): "messages_delete_own",
    ("messages", "DELETE", "Allow authorized delete access"): "messages_delete_staff",
}


def pascal(t):
    return "".join(p.capitalize() for p in t.split("_"))


def facets(f):
    return {k: v for k, v in f.items() if k != "type"}


tables = [t["name"] for t in snap["tables"]]
n_cols = len(snap["columns"])
d_ents = draft["entities"]
n_fields_draft = sum(len(e["fields"]) for e in d_ents.values())
print("== entities and fields ==")
print(f"database: {len(tables)} tables, {n_cols} columns")
print(f"draft: {len(d_ents)} entities, {n_fields_draft} fields")
print(f"hand spec: {len(hand['entities'])} entities, "
      f"{sum(len(e['fields']) for e in hand['entities'].values())} fields")

same = type_only = diff = only_hand = only_draft = 0
for table, hname in HAND_FOR.items():
    d, h = d_ents[pascal(table)]["fields"], hand["entities"][hname]["fields"]
    print(f"-- {table} (draft {pascal(table)}) vs {hname}")
    for name in sorted(set(d) | set(h)):
        if name not in h:
            only_draft += 1
            print(f"   {name:12} only in draft: {d[name]}")
        elif name not in d:
            only_hand += 1
            print(f"   {name:12} only in hand spec: {h[name]}")
        elif d[name] == h[name]:
            same += 1
            print(f"   {name:12} identical")
        elif d[name]["type"] == h[name]["type"]:
            type_only += 1
            print(f"   {name:12} same type, other modifiers: draft {facets(d[name])} "
                  f"hand {facets(h[name])}")
        else:
            diff += 1
            print(f"   {name:12} different: draft {d[name]} hand {h[name]}")
print(f"fields: {same} identical, {type_only} same type with other modifiers, {diff} other type, "
      f"{only_hand} only in hand spec, {only_draft} only in draft")

print("\n== policies ==")
pols = snap["policies"]
n_draft = len(draft.get("policies") or {})
print(f"database {len(pols)}; draft {n_draft}; hand spec {len(hand['policies'])}")
unt = {}
for line in draft_text.splitlines():
    m = re.match(r"\s*# UNTRANSLATED policy (.*?): CREATE POLICY .* ON public\.(\w+) .* -- (.*)$", line)
    if m:
        unt.setdefault((m.group(2), m.group(1)), []).append(m.group(3))
print(f"UNTRANSLATED policy comments: {sum(len(v) for v in unt.values())}")
covered = 0
for p in pols:
    key = (p["table"], p["cmd"], p["name"])
    covered += key in HAND_POLICY
    why = unt.get((p["table"], p["name"]), ["MISSING"]).pop(0)
    print(f"   {p['table']:11} {p['cmd']:6} {p['name']:34} "
          f"hand: {HAND_POLICY.get(key, '(none)'):22} draft: UNTRANSLATED ({why})")
print(f"database policies with a hand-spec policy: {covered} of {len(pols)}")

print("\n== never wider than the database ==")
policies = (draft.get("policies") or {}).values()
bad = [n for n, e in d_ents.items()
       if not e.get("private") and not any(p["on"] == n for p in policies)]
print("entities exposed without a policy:", bad or "none")
print("entities private:", sum(1 for e in d_ents.values() if e.get("private")), "of", len(d_ents))
sys.exit(1 if bad else 0)
