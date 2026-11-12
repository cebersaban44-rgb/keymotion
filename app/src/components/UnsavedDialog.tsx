import { AlertTriangle, Loader2, Save, TriangleAlert } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useT } from "../lib/i18n";
import { closeWindow } from "../lib/ipc";
import { useActiveKeyboard } from "../lib/store";
import type { UnsavedItem } from "../lib/unsaved";
import { friendlyError } from "../lib/errors";

/**
 * Shown when the window is closed with changes not stored on the keyboard:
 * save everything and quit, quit and undo the edits, or stay. When the only
 * thing pending is a write already running (an upload, a save), it offers to
 * close once that ends, to quit now, or to stay.
 */
export function UnsavedDialog({ items, onCancel }: { items: UnsavedItem[]; onCancel: () => void }) {
  const t = useT();
  const kb = useActiveKeyboard();
  const usb = kb?.connection === "usb" && kb.hasCommandChannel;
  const canSave = usb || !items.some((i) => i.needsUsb);
  // Only writes already running: nothing to save, only to wait for.
  const writing = items.every((i) => i.running);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const primary = useRef<HTMLButtonElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  // Focus once, when the dialog opens: the target is decided by the first
  // render's state and kept, so later changes do not move the focus.
  const focusPrimary = useRef(canSave && !writing);

  useEffect(() => {
    (focusPrimary.current ? primary.current : cancel.current)?.focus();
  }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onCancel]);

  const saveAndQuit = async () => {
    setBusy(true);
    setError(null);
    try {
      for (const item of items) await item.save();
      await closeWindow();
    } catch (e) {
      setError(friendlyError(e, t));
      setBusy(false);
    }
  };
  const discardAndQuit = async () => {
    for (const item of items) item.discard();
    await closeWindow();
  };

  return (
    <motion.div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-6 backdrop-blur-[2px]"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
    >
      <motion.div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="unsaved-title"
        aria-describedby="unsaved-body"
        className="card flex w-full max-w-[480px] flex-col gap-5 p-6"
        initial={{ opacity: 0, y: 8, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 4, scale: 0.98 }}
        transition={{ duration: 0.18, ease: "easeOut" }}
      >
        <div className="flex items-start gap-4">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-warn/15 text-warn">
            <TriangleAlert size={19} />
          </span>
          <div className="flex flex-col gap-1.5">
            <h2 id="unsaved-title" className="font-display text-[20px] leading-tight font-semibold">
              {t(writing ? "unsaved.writing.title" : "unsaved.title")}
            </h2>
            <p id="unsaved-body" className="text-[13px] leading-relaxed text-ink-2">
              {t(writing ? "unsaved.writing.body" : "unsaved.body")}
            </p>
          </div>
        </div>

        <ul className="flex flex-col gap-2 rounded-lg border border-line bg-bg-raise/60 p-3.5">
          {items.map((item) => (
            <li key={item.id} className="flex items-baseline gap-2.5 text-[13px]">
              <span className="h-2 w-2 shrink-0 translate-y-[-1px] rounded-full bg-warn" />
              <span className="font-medium">{t(item.area)}</span>
              <span className="text-ink-2">{t(item.detail)}</span>
            </li>
          ))}
        </ul>

        {writing ? (
          <p role="status" className="flex items-center gap-2 text-[13px] text-ink-2">
            <Loader2 size={14} className="animate-spin" aria-hidden />
            {t("unsaved.writing.waiting")}
          </p>
        ) : (
          !canSave && <p className="text-[13px] text-ink-2">{t("unsaved.needsUsb")}</p>
        )}
        {error && (
          <p role="alert" className="flex items-start gap-2 text-[13px] text-danger">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            {error}
          </p>
        )}

        <div className="flex flex-col gap-2.5">
          <div className="flex items-center justify-end gap-2">
            <button
              ref={cancel}
              type="button"
              onClick={onCancel}
              disabled={busy}
              className="h-9 rounded-lg px-3.5 text-[13px] text-ink-2 transition-colors hover:text-ink disabled:opacity-40"
            >
              {t("unsaved.cancel")}
            </button>
            <button
              type="button"
              onClick={discardAndQuit}
              disabled={busy}
              className="h-9 rounded-lg border border-line bg-bg-raise px-3.5 text-[13px] text-ink transition-colors hover:border-line-strong disabled:opacity-40"
            >
              {t(writing ? "unsaved.writing.now" : "unsaved.discardExit")}
            </button>
            {!writing && (
              <button
                ref={primary}
                type="button"
                onClick={saveAndQuit}
                disabled={busy || !canSave}
                className="flex h-9 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink transition-transform active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-40"
              >
                {busy ? <Loader2 size={14} className="animate-spin" /> : <Save size={14} />}
                {busy ? t("unsaved.saving") : t("unsaved.saveExit")}
              </button>
            )}
          </div>
        </div>
      </motion.div>
    </motion.div>
  );
}
