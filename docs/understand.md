# Understanding commands: `explain`, `graph` and `context`, version 0.1

Three read-only views of a valid `svetra.yaml`, each for a different reader:

| Command | For | Output |
|---|---|---|
| `svetra explain` | **People**, especially those who don't read code | Plain English: what the app stores, its rules, who can see and change what, and what custom code can touch |
| `svetra graph` | People who think in pictures | A Mermaid diagram of the same things |
| `svetra context` | **Agents** | The whole system in as few tokens as possible |

All three:
- validate first, and print diagnostics instead of output when there are errors (exit `1`);
- are **deterministic**, giving the same text for the same file;
- describe **only what the graph declares**. They never guess at intent, so they can't be wrong about what the compiled app does.

```sh
svetra explain [FILE]
svetra graph [FILE]
svetra context [FILE]
```

## `svetra explain`

### Wording rules

The reader may never have written code, so:
- **No jargon.** Never "RLS", "policy", "foreign key", "schema", "migration", "uuid", "constraint" or "hole". Say "who can see", "linked to", "rule" and "custom code".
- **Entities are plain nouns.** Split PascalCase and pluralise: `LedgerEntry` becomes "ledger entries", `Account` becomes "accounts".
- **The output is short and scannable:** headed sections with one fact per line.
- **Security facts come first and are stated absolutely,** including what nobody can do.

### Sections, in order

1. **What your app stores.** One block per entity: its fields in plain words, and which are required.
2. **Rules that always hold.** The declared constraints and uniqueness, in English.
3. **Who can see and change what.** Per entity: who can see, create, change and delete, then what **nobody** can do directly.
4. **Custom code.** Per hole: what it takes, what it returns, and what it can read and change.
5. **Worth knowing.** Warnings and info from `validate`, in plain words.

### Field descriptions

| Field | Described as |
|---|---|
| `uuid` primary | Not shown: every record has an id |
| `user` | "the signed-in person who owns it" if the field is named `owner`, `author`, `user` or `created_by`; otherwise "`<field>`: a signed-in person" |
| `ref` to `E` | "the `<e>` it belongs to" if the field name matches the entity (`account` to `Account`); otherwise "`<field>`: a linked `<e>`" |
| `text` | "text" |
| `int`, `float` | "a whole number", "a number" |
| `money` | "an amount of money (up to N digits before the point and S after)" |
| `decimal` | "an exact number (up to N digits before the point and S after)" |
| `bool` | "yes or no" |
| `timestamp`, `date` | "a date and time", "a date"; with `auto: now`, "set automatically when created" |
| `enum` | "one of: a, b or c" |
| `json` | "free-form data" |
| `required: true` | "(required)" |
| `default: x` | "(starts as x)" |

### Expressions in English

Constraints and policy conditions use the same translation:

| Expression | English |
|---|---|
| `a >= b`, `a > b` | "a is at least b", "a is more than b" |
| `a <= b`, `a < b` | "a is at most b", "a is less than b" |
| `a == b`, `a != b` | "a is b", "a is not b"; for two fields, "a and b are different" |
| `x == null`, `x != null` | "x is empty", "x is filled in" |
| `x in ['a', 'b']` | "x is a or b" |
| `p and q`, `p or q`, `not p` | "p and q", "p, or q", "not (p)" |
| `auth.user` | "them" (the signed-in person) |
| `ref.field` | "the field of its ref", for example `source.owner` becomes "the owner of its source" |
| `true` | "always" |

Anything that doesn't fit a pattern falls back to the expression itself in quotes. That's still accurate, just less friendly.

### Access, stated completely

For each entity, `explain` lists every allowed action. Then it says plainly what isn't allowed, which is where AI-built apps usually go wrong:
- **"Nobody can see accounts any other way."** This is always stated, because access is denied by default.
- **"Nobody can create, change or delete accounts directly."** This appears when no policy allows those actions. If holes write the entity, it adds "Only custom code can change them: `transfer`."
- **"Audit logs are private: the app can't reach them at all; only custom code can."** This is for private entities. If holes write them, it ends instead with "Only custom code can change them: `write_audit`."
- **"Anyone, even without signing in, can see posts."** Anonymous access is always called out, as its own line.

### Example (the ledger)

```
What your app stores
  Accounts
    - the signed-in person who owns it (required)
    - currency: one of USD, EUR, GBP or INR (required)
    - balance: an amount of money, up to 12 digits before the point and 6 after (required)
    - status: one of active, frozen or closed (starts as active)
    - created at: a date and time, set automatically when created
  Transactions
    ...

Rules that always hold
  - An account's balance is at least 0.
  - No two accounts have the same owner and currency.
  - A transaction's amount is more than 0.
  - A transaction's source and destination are different.

Who can see and change what
  Accounts
    - Signed-in people can see an account when its owner is them.
    - Nobody can see accounts any other way.
    - Nobody can create, change or delete accounts directly. Only custom code can change them: transfer.
  Transactions
    - Signed-in people can see a transaction when the owner of its source is them, or the owner of its destination is them.
    ...

Custom code
  transfer
    - takes: source (an account), destination (an account), amount (an amount of money)
    - returns: a transaction
    - can read: accounts. Can change: accounts, transactions.
```

## `svetra graph`

This prints one Mermaid `flowchart LR`:
- **Entity nodes** list their fields, with `name: type`, and `!` marking required fields.
- **Reference edges** go from the entity holding the field to its target, labelled with the field name. Edges to signed-in people go to a single `👤 Signed-in person` node.
- **Access edges** are dotted, and run from `👤 Signed-in person` and from `🌐 Anyone` (anonymous) to each entity. The label lists the allowed actions, such as `sees`, `creates` and `changes`, joined with commas.
- **Hole nodes** are rounded. Dashed edges labelled `reads` or `changes` run to entities.
- **Private entities** are drawn with a lock: `🔒 AuditLog`.

The output can be pasted into GitHub, which renders Mermaid in Markdown, or into any Mermaid viewer.

## `svetra context`

The most compact description that still lets an agent work correctly:
- One line per entity, then one per policy, then one per hole.
- No prose and no repetition.
- Types use the format's own names, so the agent can write graph patches straight from it.

```
FinLedger (svetra 0.1)
Account: id uuid pk; owner user!; currency enum(USD|EUR|GBP|INR)!; balance money(18,6)!; status enum(active|frozen|closed)=active; created_at timestamp=now | check balance_non_negative: balance >= 0 | unique (owner, currency)
Transaction: id uuid pk; source ref(Account)!; destination ref(Account)!; amount money(18,6)!; status enum(pending|committed|failed|reversed); created_at timestamp=now | check amount_positive: amount > 0; different_accounts: source != destination | index (source, created_at); (destination, created_at)
policy owner_reads_own_accounts: Account select authenticated when owner == auth.user
policy owner_reads_own_transactions: Transaction select authenticated when source.owner == auth.user or destination.owner == auth.user
hole transfer: (source Account.id, destination Account.id, amount Transaction.amount) -> Transaction; reads Account; writes Account, Transaction
```

**The size target:** `context` should be no larger than `svetra.yaml` itself, and much smaller than the generated code. Sizes for the fixtures are measured in the implementing PR and recorded there.

## Not in version 0.1

- `--json` for `explain` and `graph`. `context` is already meant for machines.
- Languages other than English.
- Diagrams of flows and connectors, which come with integrations in M12.
