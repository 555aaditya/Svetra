import { Badge } from "@/components/base/badges/badge";
import { ButtonLink } from "@/components/base/buttons/button";
import { Tab, TabList, TabPanel, Tabs } from "@/components/base/tabs/tabs";
import { agentRun, features, heroRun, install, leakRun, outputs, roadmap } from "@/content";
import { Copy } from "@/site/Copy";
import { Terminal } from "@/site/Terminal";
import { ThemeToggle } from "@/site/ThemeToggle";
import { cx } from "@/utils/cx";

const GITHUB = "https://github.com/555aaditya/Svetra";

function Section({ id, eyebrow, title, children, className }: { id?: string; eyebrow?: string; title: React.ReactNode; children: React.ReactNode; className?: string }) {
  return (
    <section id={id} className={cx("mx-auto max-w-6xl scroll-mt-20 px-4 py-20 sm:px-6 sm:py-28", className)}>
      {eyebrow && <p className="mb-3 text-caption-1-semibold tracking-wide text-accent-700 uppercase dark:text-accent-400">{eyebrow}</p>}
      <h2 className="max-w-3xl text-[32px] leading-[1.15] font-semibold tracking-tight text-text-primary sm:text-[40px]">{title}</h2>
      <div className="mt-10">{children}</div>
    </section>
  );
}

function Accent({ children }: { children: React.ReactNode }) {
  return <em className="font-serif font-normal text-accent-700 italic dark:text-accent-400">{children}</em>;
}

function Code({ text, className }: { text: string; className?: string }) {
  return (
    <pre className={cx("overflow-x-auto rounded-xl border border-separator-border bg-background-secondary-default p-5 font-mono text-[13px] leading-6 text-text-secondary", className)}>
      {text}
    </pre>
  );
}

function Nav() {
  return (
    <header className="sticky top-0 z-10 border-b border-separator-border bg-background-full/80 backdrop-blur">
      <nav aria-label="Main" className="mx-auto flex h-16 max-w-6xl items-center gap-6 px-4 sm:px-6">
        <a href="#top" className="text-[18px] font-semibold tracking-tight text-text-primary">
          Svetra
        </a>
        <div className="hidden items-center gap-5 text-body-2-medium text-text-tertiary md:flex">
          <a className="hover:text-text-primary" href="#how">How it works</a>
          <a className="hover:text-text-primary" href="#agent">In your agent</a>
          <a className="hover:text-text-primary" href="#tests">Access tests</a>
          <a className="hover:text-text-primary" href="#install">Install</a>
          <a className="hover:text-text-primary" href={`${GITHUB}/tree/main/docs`}>Docs</a>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <ThemeToggle />
          <ButtonLink href={GITHUB} variant="secondary" size="small">GitHub</ButtonLink>
        </div>
      </nav>
    </header>
  );
}

function Hero() {
  return (
    <div id="top" className="mx-auto grid max-w-6xl items-center gap-12 px-4 pt-16 pb-20 sm:px-6 sm:pt-24 lg:grid-cols-[1.05fr_1fr]">
      <div>
        <Badge color="primary" className="mb-6">v0.4.0 · in your agent: MCP server, skill and safe custom code</Badge>
        <h1 className="text-[40px] leading-[1.08] font-semibold tracking-tight text-text-primary sm:text-[52px] lg:text-[50px] xl:text-[54px]">
          You design the data.
          <br />
          <Accent>Svetra makes it safe.</Accent>
          <br />
          The AI builds the rest.
        </h1>
        <p className="mt-6 max-w-xl text-[18px] leading-8 text-text-secondary">
          One small file, <code className="font-mono text-[16px] text-text-primary">svetra.yaml</code>, compiles to a locked-down Supabase backend, and writes the tests that prove it on your real database.
        </p>
        <div className="mt-8 max-w-xl">
          <Copy cmd="npx -y svetra init" />
        </div>
        <div className="mt-6 flex flex-wrap items-center gap-4">
          <ButtonLink href="#install">Get started</ButtonLink>
          <a href={GITHUB} className="text-body-2-medium text-text-secondary hover:text-text-primary">View on GitHub →</a>
        </div>
        <p className="mt-10 text-caption-1-medium text-text-tertiary">Rust · Postgres · Supabase · pgTAP · Zod · MIT licence</p>
      </div>
      <Terminal lines={heroRun} title="bookings app" />
    </div>
  );
}

export function App() {
  return (
    <div className="min-h-screen bg-background-full text-text-primary antialiased">
      <a href="#how" className="sr-only focus:not-sr-only focus:absolute focus:p-3">Skip to content</a>
      <Nav />
      <main>
        <Hero />

        <Section eyebrow="The problem" title={<>Most vibe-coded Supabase leaks are <Accent>one missing rule.</Accent></>} className="border-t border-separator-border">
          <p className="max-w-3xl text-[18px] leading-8 text-text-secondary">
            Supabase lets the browser query your tables directly. That's safe only if every table has row-level security and the right rules. Miss one, and anyone with your public key can read the whole table. Agents miss it all the time, and the people building with them often can't read the SQL to notice. Svetra turns security on for every table, closes each one by default, and refuses a table the API can reach without a rule.
          </p>
        </Section>

        <Section id="how" eyebrow="How it works" title="Three steps, all of them checked.">
          <ol className="grid gap-4 md:grid-cols-3">
            {[
              ["Design", "You and your agent write svetra.yaml: the data, the rules, who can see what, and how people sign in."],
              ["Compile", "Svetra checks it with 21 rules, then writes the Postgres schema, row-level security, types, access tests and fenced-in custom code."],
              ["Prove", "supabase test db runs the access tests as real users. Every person, action and row, checked."],
            ].map(([t, b], i) => (
              <li key={t} className="rounded-xl border border-separator-border p-6">
                <span className="font-mono text-caption-1-medium text-accent-700 dark:text-accent-400">0{i + 1}</span>
                <h3 className="mt-3 text-[20px] font-semibold text-text-primary">{t}</h3>
                <p className="mt-2 text-body-2-regular leading-6 text-text-secondary">{b}</p>
              </li>
            ))}
          </ol>
          <div className="mt-12">
            <Tabs aria-label="Real output">
              <TabList aria-label="Outputs">
                {outputs.map((o) => <Tab key={o.id} id={o.id}>{o.label}</Tab>)}
              </TabList>
              {outputs.map((o) => (
                <TabPanel key={o.id} id={o.id}>
                  <Code text={o.text} className="max-h-[28rem]" />
                </TabPanel>
              ))}
            </Tabs>
            <p className="mt-3 text-caption-1-regular text-text-tertiary">Real output from the bookings and clinic examples, taken from the repository's tests.</p>
          </div>
        </Section>

        <Section id="agent" eyebrow="In your agent" title={<>Design it in a conversation. <Accent>Svetra checks every edit.</Accent></>}>
          <div className="grid items-start gap-10 lg:grid-cols-[1fr_1.1fr]">
            <div className="space-y-6 text-[17px] leading-7 text-text-secondary">
              <p>One command sets up any folder for Claude Code. Cursor, Codex and other agents that speak MCP can run the same server.</p>
              <ul className="space-y-4">
                <li><strong className="text-text-primary">The skill</strong> teaches your agent one loop: design svetra.yaml with you, validate after every edit, compile, explain the result in your words, then prove it.</li>
                <li><strong className="text-text-primary">The MCP server,</strong> <code className="font-mono text-[15px] text-text-primary">svetra mcp</code>, gives it five tools: validate, compile, explain, context and graph. None of them edit svetra.yaml: your agent makes the change, and Svetra checks it.</li>
                <li><strong className="text-text-primary">The agent writes only custom code bodies.</strong> Policies, grants, types and tests all come from svetra.yaml, so nothing safety-critical is written by hand.</li>
              </ul>
            </div>
            <Terminal lines={agentRun} title="an empty folder" />
          </div>
        </Section>

        <Section id="tests" eyebrow="Access tests" title={<>Your database proves <Accent>who can see what.</Accent></>}>
          <div className="grid items-start gap-10 lg:grid-cols-[1fr_1.1fr]">
            <div className="space-y-6 text-[17px] leading-7 text-text-secondary">
              <p>Add one policy by hand that lets every signed-in person read every booking. Exactly the checks it breaks fail, each by name.</p>
              <ul className="space-y-4">
                <li><strong className="text-text-primary">Never the service key.</strong> Checks run as logged-out visitors and signed-in people, because the service key bypasses security and proves nothing.</li>
                <li><strong className="text-text-primary">Every failure, not just the first.</strong> Each check is undone afterwards and the file rolls back, so nothing persists.</li>
                <li><strong className="text-text-primary">Expected results come from svetra.yaml,</strong> not from the SQL Svetra wrote, so a hand-edited database or a compiler bug both show up.</li>
              </ul>
            </div>
            <Terminal lines={leakRun} title="after a hand-written policy" />
          </div>
        </Section>

        <Section eyebrow="What's in it" title="Small file, complete backend.">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {features.map((f) => (
              <article key={f.title} className="flex flex-col rounded-xl border border-separator-border p-6">
                <h3 className="text-[18px] font-semibold text-text-primary">{f.title}</h3>
                <p className="mt-2 flex-1 text-body-2-regular leading-6 text-text-secondary">{f.body}</p>
                <code className="mt-5 block rounded-lg bg-background-secondary-default px-3 py-2 font-mono text-[12px] break-words text-text-tertiary">{f.code}</code>
              </article>
            ))}
          </div>
        </Section>

        <Section id="install" eyebrow="Install" title="Try it in one command.">
          <Tabs aria-label="Install" className="max-w-3xl">
            <TabList aria-label="Install methods">
              {install.map((m) => <Tab key={m.id} id={m.id}>{m.label}</Tab>)}
            </TabList>
            {install.map((m) => (
              <TabPanel key={m.id} id={m.id}>
                <Copy cmd={m.cmd} />
                <p className="mt-3 text-caption-1-regular text-text-tertiary">{m.note}</p>
              </TabPanel>
            ))}
          </Tabs>
          <Code className="mt-8 max-w-3xl" text={"svetra validate svetra.yaml   # check it, or --json for agents\nsvetra compile svetra.yaml    # schema, types, access tests, sign-in settings\nsvetra explain svetra.yaml    # the app in plain English\nsvetra init                   # set the folder up for your agent\nsvetra mcp                    # the MCP server agents talk to\nsupabase test db              # prove it on your database"} />
        </Section>

        <Section eyebrow="Roadmap" title="Built in public, a release a day.">
          <ol className="max-w-3xl border-l border-separator-border">
            {roadmap.map((r) => (
              <li key={r.what} className="relative pb-8 pl-8 last:pb-0">
                <span className={cx("absolute top-1.5 -left-[5px] size-[9px] rounded-full", r.done ? "bg-accent-400" : "border border-separator-border-strong bg-background-full")} />
                <p className="font-mono text-caption-1-medium text-text-tertiary">{r.when}</p>
                <p className={cx("mt-1 text-[17px]", r.done ? "text-text-primary" : "text-text-secondary")}>{r.what}</p>
              </li>
            ))}
          </ol>
        </Section>
      </main>

      <footer className="border-t border-separator-border">
        <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-x-6 gap-y-2 px-4 py-10 text-caption-1-medium text-text-tertiary sm:px-6">
          <span className="text-text-secondary">Svetra</span>
          <span>MIT licence</span>
          <a className="hover:text-text-primary" href={`${GITHUB}/tree/main/docs`}>Docs</a>
          <a className="hover:text-text-primary" href="https://www.npmjs.com/package/svetra">npm</a>
          <a className="hover:text-text-primary" href="https://crates.io/crates/svetra">crates.io</a>
          <a className="hover:text-text-primary" href="https://x.com/555aaditya">Built in public by @555aaditya</a>
        </div>
      </footer>
    </div>
  );
}
