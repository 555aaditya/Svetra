// Everything shown as output on the page is real. Files from tests/ are imported as raw
// text, so the site always shows what the compiler prints today. Transcripts that can't be
// imported (terminal runs) are copied from real runs, noted beside each one.
import bookingsYaml from "../../tests/fixtures/bookings.yaml?raw";
import clinicYaml from "../../tests/fixtures/clinic.yaml?raw";
import bookingsExplain from "../../tests/golden/bookings.explain.txt?raw";
import clinicExplain from "../../tests/golden/clinic.explain.txt?raw";
import bookingsSql from "../../tests/golden/bookings.sql?raw";
import bookingsContext from "../../tests/golden/bookings.context.txt?raw";

export type Line = { text: string; tone?: "cmd" | "dim" | "ok" | "bad" | "info" };

/** The hero run: the bookings example, validated, compiled, then tested on a local Supabase
 * (`supabase test db`, 2026-11-04). The validate hint is cut short and the repeated note in
 * `compile` is shown as "…"; the test file path is shortened from the absolute one. */
export const heroRun: Line[] = [
  { text: "npx svetra validate svetra.yaml", tone: "cmd" },
  { text: "svetra.yaml:2:9: info SVT009: No `auth` section, so Supabase's defaults apply: passwords of 6 characters, and no email confirmation.", tone: "info" },
  { text: "  hint: Add `auth: { sign_in: [email] }` for safe defaults…", tone: "dim" },
  { text: "svetra.yaml is valid (0 warnings).", tone: "ok" },
  { text: "npx svetra compile svetra.yaml", tone: "cmd" },
  { text: "…", tone: "dim" },
  { text: "wrote supabase/schemas/svetra.sql" },
  { text: "wrote src/svetra.gen.ts" },
  { text: "wrote supabase/tests/svetra.test.sql" },
  { text: "supabase test db", tone: "cmd" },
  { text: "supabase/tests/svetra.test.sql .. ok" },
  { text: "All tests successful." },
  { text: "Files=1, Tests=87" },
  { text: "Result: PASS", tone: "ok" },
];

/** `supabase test db` after adding a policy that lets everyone read bookings (2026-11-04). */
export const leakRun: Line[] = [
  { text: `create policy leak on public.booking for select to authenticated using (true);`, tone: "cmd" },
  { text: "supabase test db", tone: "cmd" },
  { text: `# Failed test 57: "alice can't see Booking b"`, tone: "bad" },
  { text: "#         have: allowed", tone: "dim" },
  { text: "#         want: denied", tone: "dim" },
  { text: `# Failed test 64: "bob can't see Booking a"`, tone: "bad" },
  { text: `# Failed test 81: "staff at Location a can't see Booking b"`, tone: "bad" },
  { text: "# Looks like you failed 3 tests of 87" },
  { text: "Result: FAIL", tone: "bad" },
];

/** The policies section of the compiled schema. */
const policies = bookingsSql.slice(bookingsSql.indexOf("-- Policies"));

/** The sign-in part of `svetra explain` for the clinic example. */
const signingIn = clinicExplain.slice(clinicExplain.indexOf("Signing in"), clinicExplain.indexOf("\nWho can see"));

export const outputs = [
  { id: "yaml", label: "svetra.yaml", text: bookingsYaml.trimEnd() },
  { id: "explain", label: "explain", text: bookingsExplain.slice(bookingsExplain.indexOf("Who can see")).trimEnd() },
  { id: "sql", label: "SQL", text: policies.trimEnd() },
  { id: "context", label: "context", text: bookingsContext.trimEnd() },
  { id: "auth", label: "sign-in", text: `${clinicYaml.slice(clinicYaml.indexOf("auth:"), clinicYaml.indexOf("entities:")).trimEnd()}\n\n${signingIn.trimEnd()}` },
];

export const install = [
  { id: "npx", label: "npx", cmd: "npx svetra --version", note: "Node 18+, on macOS, Linux and Windows. The first run downloads the release binary and checks its SHA-256." },
  { id: "cargo", label: "cargo", cmd: "cargo install svetra", note: "Rust 1.88+, from crates.io." },
  { id: "curl", label: "curl", cmd: "curl -fsSL https://raw.githubusercontent.com/555aaditya/Svetra/main/install.sh | sh", note: "macOS and Linux. Checks the download's SHA-256 before installing." },
];

export const features = [
  { title: "21 checks before anything is written", body: "A table the API can reach with no rule, money stored as a float, weak sign-in settings. Each with a line, a column and a fix, or as JSON for your agent.", code: "SVT001  Order is exposed through the API but has no policy." },
  { title: "Roles and teams", body: "Global roles like admin, or scoped ones like staff at one location, checked by a locked-down function the API can't reach.", code: "when: \"has_role('staff', location)\"" },
  { title: "Access tests", body: "Every person × action × row, as real users, never the service key. Expected results come from svetra.yaml, not from the SQL Svetra wrote.", code: "ok 57 - alice can't see Booking b" },
  { title: "Sign-in and second factors", body: "Safe defaults in supabase/config.toml, only Svetra's own keys touched. Roles marked mfa: true only count after an authenticator app.", code: "admin: { mfa: true }" },
  { title: "Plain English", body: "svetra explain says who can see and change what, including what nobody can do. A test fails if it ever says \"RLS\".", code: "Nobody can see bookings any other way." },
  { title: "Context for agents", body: "The whole app in a few lines, so your agent reads the design instead of thousands of generated lines.", code: "auth email, google; signup invite_only; mfa admin" },
];

export const roadmap = [
  { when: "Nov 3", what: "Format, validator, compiler, explain, graph, context", done: true },
  { when: "Nov 4", what: "Roles and teams · v0.1.0 · access tests in v0.2.0", done: true },
  { when: "Nov 5", what: "Sign-in settings and second factors in v0.3.0 · npm", done: true },
  { when: "Next", what: "The Svetra skill and MCP server for Claude Code, Cursor and Codex", done: false },
  { when: "Later", what: "svetra view: an interactive map of your app · safe changes with svetra plan", done: false },
];
