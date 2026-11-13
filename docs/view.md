# `svetra view`, version 0.1

This document specifies `svetra view`: one page that shows a person who doesn't read code who can see and change what in their app, as a table, with a diagram beside it. The words follow [`understand.md`](understand.md), and the answers come from the same evaluator as the access tests in [`compiler.md`](compiler.md). When the code and this document disagree, that's a bug in one of them.

**Done when:** someone who has never written code answers "who can see orders?" correctly from the page alone.

## Command

```sh
svetra view [FILE] [--out PATH] [--no-open] [--json]
```

- **`FILE`** defaults to `./svetra.yaml`.
- **It validates first** and refuses a file with errors, printing the diagnostics with exit `1`, like `explain`: people read the page as the truth about the app.
- **It writes one self-contained HTML file,** by default `.svetra/view.html` next to `FILE`, and opens it in the default browser. `--out` writes it elsewhere; `--no-open` doesn't open it. It prints the path either way.
- **`--json`** prints the view model (below) instead, and writes nothing. It's what the page embeds, and what tests and agents read.
- **Deterministic:** the same file gives byte-identical output. No timestamps, no random ids.

`.svetra/` is Svetra's scratch folder. `svetra view` adds `.svetra/` to `.gitignore` if the file exists and doesn't already ignore it.

## The page

Three parts, in this order.

### 1. Who can see and change what

The main part: a table.

- **Rows are people,** the same people the access tests use, in the same order:
  - **Someone not signed in**
  - **A signed-in person** (alice in the tests) and **another signed-in person** (bob), so "their own" and "someone else's" both show
  - **one row per role:** "admin", or "staff at a location" for a scoped role
  - **for each role with `mfa: true`,** that role again "without a second factor"
- **Columns are things and actions:** one group per entity, headed with its plain plural ("Bookings"), and four columns in each: **See, Create, Change, Delete**. Private entities are listed with every cell "No" and the note "not reachable from the app".
- **Each cell says, in words:**

  | Cell | Means |
  |---|---|
  | **All** | Every row the tests try, for example every booking |
  | **Some** | Some rows and not others, for example only their own bookings |
  | **No** | None |

  Colour backs up the word, never replaces it, so the table reads in greyscale and to screen readers.
- **Selecting a cell** opens a panel with:
  - the rule sentences from `explain` that allow it, such as "Signed-in people can see and create a booking when its customer is them", or "Nobody can see bookings any other way" for **No**;
  - for **Some**, which of the test rows were allowed, in words ("their own booking: yes; another customer's booking: no");
  - the access test numbers that check this cell, so a curious reader can find them in `supabase/tests/svetra.test.sql`.
- **A question box** above the table answers the done-when question directly: pick a thing ("Bookings") and an action ("see"), and the page lists, per person, All, Some or No, with the sentence. It's a filter of the same table, not new logic.

### 2. Custom code

One card per hole, from the "Custom code" part of `explain`: what it takes and returns, **who can call it** (with the same All, Some, No per person, from the hole's `as` and `when`), what it can read and what it can change.

### 3. The diagram

The entities as boxes with their links, the people and roles, and the custom code, drawn by the page itself from the view model. The page makes no network requests, so the diagram can't come from a hosted library. Below it, "Signing in" and "Before going live" from `explain`, when the file has them.

## The view model

`svetra view --json` prints this, and the page embeds it as JSON in a `<script type="application/json">`. Keys are stable from 0.1; new keys may be added. The values below are illustrative, shortened from the bookings example.

```json
{
  "svetra": "0.1",
  "system": "Bookings",
  "revision": "…the compile fingerprint…",
  "people": [
    { "id": "anon", "label": "Someone not signed in" },
    { "id": "alice", "label": "A signed-in person" },
    { "id": "bob", "label": "Another signed-in person" },
    { "id": "admin", "label": "admin", "role": "admin" },
    { "id": "staff at Location a", "label": "staff at a location", "role": "staff", "scope": "Location" }
  ],
  "entities": [
    { "name": "Booking", "label": "bookings", "private": false,
      "fields": [{ "name": "customer", "label": "customer: a signed-in person (required)" }],
      "links": [{ "field": "location", "to": "Location" }] }
  ],
  "cells": [
    { "person": "alice", "entity": "Booking", "action": "select", "access": "some",
      "rows": [{ "row": "a", "label": "their own booking", "allowed": true },
               { "row": "b", "label": "another customer's booking", "allowed": false }],
      "because": ["Signed-in people can see and create a booking when its customer is them."],
      "tests": [56, 57] }
  ],
  "holes": [
    { "name": "transfer", "takes": "…", "returns": "…", "reads": ["Account"], "writes": ["Account", "Transaction"],
      "callers": [{ "person": "alice", "access": "some", "because": ["…when the owner of the source is them"] }] }
  ],
  "signing_in": ["People sign in with email (a password or an emailed link)."],
  "before_going_live": ["Set up your own email sender (SMTP)…"]
}
```

- **`access`** is `all`, `some` or `none`, computed by the access tests' evaluator over the same rows for the same people. A cell the evaluator can't decide (the tests skip such a check) is `unknown`, shown as "Not checked", with the reason.
- **`because`** holds sentences `explain` prints, never new wording.
- **`tests`** are the numbers of the checks in `supabase/tests/svetra.test.sql` for this cell, so the page and the tests can be cross-checked.

## Implementation

- **The page is one HTML template compiled into the binary,** with inline CSS and plain JavaScript, using the website's colours, type and spacing. `cargo install svetra` must keep working without Node, so the page can't need a build step; a React and BoardUI bundle would have to be committed as a generated file. Revisit if the page grows real interaction, such as editing.
- **No network:** a `Content-Security-Policy` meta tag allows only inline styles and scripts and `data:` images.
- **Accessibility:** the table is a real `<table>` with header cells for people and actions, every cell has text, everything works from the keyboard, and the colour contrast meets WCAG AA in light and dark modes.

## Tests

- **Golden files:** the view model of each example (`tests/golden/<name>.view.json`).
- **The page and the tests agree:** for every example, each cell's `rows` equal the expected results in the generated access tests, and its `tests` numbers point at checks for the same person, entity and action.
- **No jargon:** the page's text passes the same word list as `explain`.
- **Determinism:** two runs give identical bytes.

## Not in version 0.1

- `--watch`, which redraws the page when `svetra.yaml` changes.
- Editing from the page. The design stays in `svetra.yaml`, edited by the agent.
- A `view` tool in `svetra mcp`. Agents already have `explain`, `context` and `graph`.
- Showing a change before and after, which comes with `svetra diff` (M6).
