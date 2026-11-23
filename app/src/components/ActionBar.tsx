// The bar at the bottom of every editor (lighting, key mapping, macros), a
// Windows info bar: "Not saved to the keyboard yet", what is pending in one
// line, "Revert" and "Save to keyboard". It stays in view while the page
// scrolls, and it is the only place on the page that counts the changes
// (the navigation pane's dot is the other, app-wide).

import { Info, Loader2, Undo2 } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { friendlyError } from "../lib/errors";
import { useT } from "../lib/i18n";
import { InfoBar } from "./controls";

type Status = { kind: "idle" } | { kind: "busy" } | { kind: "ok" } | { kind: "error"; message: string };

export function ActionBar({
  left,
  changes,
  blocked,
  canSave,
  onSave,
  onRevert,
}: {
  /** Optional control on the left, such as "Klavyede önizle". */
  left?: ReactNode;
  /** What is not on the keyboard yet, as a sentence; `null` when nothing. */
  changes: string | null;
  /** Why saving is not possible right now (shown instead of `changes`). */
  blocked?: string | null;
  /** The keyboard can be written (connected over USB, records loaded). */
  canSave: boolean;
  onSave: () => Promise<void>;
  /** Drop the pending changes; omitted when there is nothing to go back to. */
  onRevert?: () => void;
}) {
  const t = useT();
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const busy = status.kind === "busy";
  const pending = changes !== null;
  const notSaved = t("bar.notSaved");

  // A new edit replaces the last result ("saved" or the error).
  useEffect(() => {
    if (changes !== null) setStatus((s) => (s.kind === "busy" ? s : { kind: "idle" }));
  }, [changes]);

  const save = async () => {
    setStatus({ kind: "busy" });
    try {
      await onSave();
      setStatus({ kind: "ok" });
    } catch (e) {
      setStatus({ kind: "error", message: friendlyError(e, t) });
    }
  };

  // The line under the title: what changed, unless it only repeats it.
  const detail = changes && changes !== notSaved ? changes : null;
  let tone: "info" | "warn" | "error" | "ok" = "info";
  let title: ReactNode = null;
  let line: ReactNode = null;
  let icon: ReactNode | undefined;
  if (busy) {
    title = t("bar.saving");
    icon = <Loader2 size={20} className="animate-spin" />;
  } else if (status.kind === "error") {
    tone = "error";
    title = notSaved;
    line = <span className="text-danger">{status.message}</span>;
  } else if (blocked && pending) {
    tone = "warn";
    title = notSaved;
    line = blocked;
  } else if (pending) {
    title = notSaved;
    line = detail && <span className="num">{detail}</span>;
  } else if (status.kind === "ok") {
    tone = "ok";
    title = t("bar.saved");
  } else {
    icon = <Info size={20} strokeWidth={1.6} className="text-ink-3" />;
  }

  return (
    <InfoBar
      floating
      live
      tone={tone}
      icon={icon}
      title={title}
      data="action"
      actions={
        <>
          {left && <div className="mr-2 flex min-w-0 items-center gap-3">{left}</div>}
          <button
            type="button"
            className="btn-secondary"
            disabled={!pending || !onRevert || busy}
            onClick={() => {
              onRevert?.();
              setStatus({ kind: "idle" });
            }}
          >
            <Undo2 size={15} strokeWidth={1.6} aria-hidden />
            {t("bar.revert")}
          </button>
          <button type="button" className="btn-primary" disabled={!canSave || !pending || !!blocked || busy} onClick={save}>
            {busy && <Loader2 size={15} className="animate-spin" aria-hidden />}
            {t("bar.save")}
          </button>
        </>
      }
    >
      {line}
    </InfoBar>
  );
}
