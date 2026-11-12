// The bar at the bottom of every editor (lighting, key mapping, macros):
// what is pending in one sentence, "Geri al" and "Klavyeye kaydet". It
// stays in view while the page scrolls, and it is the only place on the
// page that counts the changes (the sidebar dot is the other, app-wide).

import { AlertTriangle, Check, Loader2 } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { friendlyError } from "../lib/errors";
import { useT } from "../lib/i18n";

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

  let message: ReactNode = null;
  if (busy) {
    message = (
      <span className="flex items-center gap-2 text-ink-2">
        <Loader2 size={14} className="animate-spin" aria-hidden />
        {t("bar.saving")}
      </span>
    );
  } else if (status.kind === "error") {
    message = (
      <span className="flex items-start gap-2 text-danger">
        <AlertTriangle size={14} className="mt-0.5 shrink-0" aria-hidden />
        {status.message}
      </span>
    );
  } else if (blocked && pending) {
    message = <span className="text-danger">{blocked}</span>;
  } else if (pending) {
    message = <span className="num text-ink">{changes}</span>;
  } else if (status.kind === "ok") {
    message = (
      <span className="flex items-center gap-2 text-ok">
        <Check size={14} aria-hidden />
        {t("bar.saved")}
      </span>
    );
  }

  return (
    <div
      className="sticky bottom-4 z-20 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-lg border border-line-strong bg-surface px-4 py-3 shadow-[0_10px_30px_-14px_rgb(0_0_0/0.55)]"
      data-action-bar
    >
      {left && <div className="flex min-w-0 items-center gap-3">{left}</div>}
      <div role="status" className="ml-auto min-w-0 text-[13px]">
        {message}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <button
          type="button"
          className="btn-secondary"
          disabled={!pending || !onRevert || busy}
          onClick={() => {
            onRevert?.();
            setStatus({ kind: "idle" });
          }}
        >
          {t("bar.revert")}
        </button>
        <button type="button" className="btn-primary" disabled={!canSave || !pending || !!blocked || busy} onClick={save}>
          {busy && <Loader2 size={15} className="animate-spin" aria-hidden />}
          {t("bar.save")}
        </button>
      </div>
    </div>
  );
}
