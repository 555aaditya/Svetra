# Column rules: `unchanged` on an update policy (draft)

This document specifies a rule for which **fields** an update may change. When the code and this document disagree, that's a bug in one of them.

**Status: draft for review.** The decisions marked **Open** need the maintainer's answer before building. Nothing here is built.

**Done when:** the approvals app from the agent evaluation (`eval/agent/prompts/approvals.md`) is written entirely in `svetra.yaml`, with no hole and no hand-written SQL, passes every one of its 24 oracle checks, and the generated access tests prove the column rule on the database.

## Why

`check` on update (docs/update-check.md) lets a rule say what an update may turn a row **into**: a draft may become submitted, a submitted document approved or rejected. It cannot say **what the update may touch**. In the approvals prompt a reviewer may approve or reject a document but "cannot change anything else about it", so a reviewer must not be able to approve **and** retitle it in one update. A condition on the new row can't tell: the new row alone doesn't show what changed.

The agent evaluation measured how much this matters. After `check` was built (Svetra PR #69), four runs of the approvals prompt with Svetra all used `check` for the status changes and wrote **no holes**, and **all four** then appended a hand-written trigger to their migration for this one rule. The result was correct (24 of 24 checks), but the trigger lives outside `svetra.yaml`: it isn't in the design, `explain` doesn't mention it, and the generated access tests don't cover it. That is exactly the kind of rule Svetra exists to keep out of hand-written SQL.

Row-level security cannot do this by itself. A policy sees one row at a time, the old one in `USING` and the new one in `WITH CHECK`, never both. Comparing old and new needs a row trigger, which is what the agents wrote and what this feature would generate and test.
