import { useEffect, useState } from "react";
import { cx } from "@/utils/cx";
import type { Line } from "@/content";

const tone: Record<NonNullable<Line["tone"]>, string> = {
  cmd: "text-text-primary",
  dim: "text-text-tertiary",
  ok: "text-accent-400",
  bad: "text-[#f08a6c]",
  info: "text-[#9cc3ff]",
};

const reduced = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/** A terminal that types each command, then prints its output a line at a time. With reduced
 * motion, or after the first run, it shows everything at once. */
export function Terminal({ lines, title }: { lines: Line[]; title: string }) {
  const [shown, setShown] = useState(0); // whole lines shown
  const [typed, setTyped] = useState(0); // characters of the current command
  const [run, setRun] = useState(0);

  useEffect(() => {
    if (reduced()) {
      setShown(lines.length);
      return;
    }
    setShown(0);
    setTyped(0);
    let i = 0;
    let c = 0;
    let timer: number;
    const step = () => {
      if (i >= lines.length) return;
      const line = lines[i];
      if (line.tone === "cmd" && c < line.text.length) {
        c += 1;
        setTyped(c);
        timer = window.setTimeout(step, 28);
      } else {
        i += 1;
        c = 0;
        setShown(i);
        setTyped(0);
        timer = window.setTimeout(step, line.tone === "cmd" ? 350 : 110);
      }
    };
    timer = window.setTimeout(step, 500);
    return () => window.clearTimeout(timer);
  }, [lines, run]);

  const current = lines[shown];
  return (
    <div className="dark overflow-hidden rounded-xl border border-separator-border bg-background-secondary-default text-text-primary shadow-2xl shadow-black/20">
      <div className="flex items-center gap-2 border-b border-separator-border px-4 py-3">
        <span className="size-3 rounded-full bg-[#ff5f57]" />
        <span className="size-3 rounded-full bg-[#febc2e]" />
        <span className="size-3 rounded-full bg-[#28c840]" />
        <span className="ml-2 text-caption-1-medium text-text-tertiary">{title}</span>
        <button
          type="button"
          onClick={() => setRun((r) => r + 1)}
          className="ml-auto rounded-md px-2 py-1 text-caption-1-medium text-text-tertiary hover:text-text-primary focus-visible:outline-2 focus-visible:outline-accent-400"
        >
          Replay
        </button>
      </div>
      <pre aria-label={`${title} output`} className="min-h-[22rem] p-5 font-mono text-[13px] leading-6 break-words whitespace-pre-wrap">
        {lines.slice(0, shown).map((l, k) => (
          <div key={k} className={cx(l.tone ? tone[l.tone] : "text-text-secondary")}>
            {l.tone === "cmd" && <span className="select-none text-accent-400">$ </span>}
            {l.text}
          </div>
        ))}
        {current?.tone === "cmd" && (
          <div className="text-text-primary">
            <span className="select-none text-accent-400">$ </span>
            {current.text.slice(0, typed)}
            <span className="ml-px inline-block h-4 w-2 translate-y-0.5 animate-pulse bg-accent-400" />
          </div>
        )}
      </pre>
    </div>
  );
}
