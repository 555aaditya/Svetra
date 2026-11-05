import { useState } from "react";

/** A command with a copy button. */
export function Copy({ cmd }: { cmd: string }) {
  const [done, setDone] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(cmd);
      setDone(true);
      window.setTimeout(() => setDone(false), 1500);
    } catch {
      // Clipboard access can be refused; the command stays selectable.
    }
  };
  return (
    <div className="flex items-center gap-3 rounded-xl border border-separator-border bg-background-secondary-default py-2 pr-2 pl-4 font-mono text-[14px]">
      <span className="select-none text-accent-400">$</span>
      <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap text-text-primary">{cmd}</code>
      <button
        type="button"
        onClick={copy}
        className="shrink-0 rounded-lg border border-separator-border px-3 py-1.5 font-sans text-caption-1-medium text-text-secondary hover:text-text-primary focus-visible:outline-2 focus-visible:outline-accent-400"
      >
        {done ? "Copied" : "Copy"}
      </button>
    </div>
  );
}
