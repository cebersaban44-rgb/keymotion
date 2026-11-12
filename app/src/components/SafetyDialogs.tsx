import { FileWarning, TriangleAlert, type LucideIcon } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, type ReactNode } from "react";
import { useT } from "../lib/i18n";
import { KEYBOARD_RECORDS, recordKey, startSafety, useSafety } from "../lib/safety";

/**
 * Questions and notices that guard the keyboard's memory (lib/safety.ts):
 * writing on untested firmware, and settings records that were reset.
 */
export function SafetyDialogs() {
  const firmware = useSafety((s) => s.firmware);
  const resets = useSafety((s) => s.resets);
  useEffect(() => startSafety(), []);

  const named = resets.filter((r) => recordKey(r.file));
  return (
    <AnimatePresence>
      {firmware ? (
        <FirmwareDialog key="firmware" version={firmware} />
      ) : named.length > 0 ? (
        <ResetNotice key="reset" files={named.map((r) => r.file)} />
      ) : null}
    </AnimatePresence>
  );
}

/** Whether a safety dialog covers the app (the rest is inert meanwhile). */
export function useSafetyDialogOpen() {
  return useSafety((s) => s.firmware !== null || s.resets.some((r) => recordKey(r.file) !== null));
}

function FirmwareDialog({ version }: { version: string }) {
  const t = useT();
  const answer = useSafety((s) => s.answerFirmware);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => cancel.current?.focus(), []);
  useEscape(() => answer(false));

  return (
    <Dialog id="firmware" icon={TriangleAlert} tone="warn" title={t("safety.fw.title")}>
      <p id="firmware-body" className="text-[13px] leading-relaxed text-ink-2">
        {t("safety.fw.body", { version })}
      </p>
      <div className="flex items-center justify-end gap-2">
        <button
          ref={cancel}
          type="button"
          onClick={() => answer(false)}
          className="h-9 rounded-lg border border-line bg-bg-raise px-3.5 text-[13px] text-ink transition-colors hover:border-line-strong"
        >
          {t("safety.fw.cancel")}
        </button>
        <button
          type="button"
          onClick={() => answer(true)}
          className="h-9 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink transition-transform active:scale-[0.98]"
        >
          {t("safety.fw.continue")}
        </button>
      </div>
    </Dialog>
  );
}

function ResetNotice({ files }: { files: string[] }) {
  const t = useT();
  const dismiss = useSafety((s) => s.dismissResets);
  const ok = useRef<HTMLButtonElement>(null);
  useEffect(() => ok.current?.focus(), []);
  useEscape(dismiss);
  const keyboard = files.some((f) => KEYBOARD_RECORDS.includes(f));

  return (
    <Dialog id="reset" icon={FileWarning} tone="ink" title={t("safety.reset.title")}>
      <ul className="flex flex-col gap-2 rounded-lg border border-line bg-bg-raise/60 p-3.5" data-settings-resets>
        {files.map((f) => {
          const key = recordKey(f);
          return (
            key && (
              <li key={f} className="flex items-baseline gap-2.5 text-[13px]">
                <span className="h-2 w-2 shrink-0 translate-y-[-1px] rounded-full bg-warn" />
                <span className="font-medium">{t(key)}</span>
              </li>
            )
          );
        })}
      </ul>
      <div id="reset-body" className="flex flex-col gap-1 text-[13px] leading-relaxed text-ink-2">
        <p>{t("safety.reset.body")}</p>
        {keyboard && <p>{t("safety.reset.keyboard")}</p>}
      </div>
      <div className="flex justify-end">
        <button
          ref={ok}
          type="button"
          onClick={dismiss}
          className="h-9 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink transition-transform active:scale-[0.98]"
        >
          {t("safety.reset.ok")}
        </button>
      </div>
    </Dialog>
  );
}

function useEscape(onEscape: () => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onEscape();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onEscape]);
}

/** The modal frame, like the unsaved-changes dialog. */
function Dialog({
  id,
  icon: Icon,
  tone,
  title,
  children,
}: {
  id: string;
  icon: LucideIcon;
  tone: "warn" | "ink";
  title: string;
  children: ReactNode;
}) {
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
        aria-labelledby={`${id}-title`}
        aria-describedby={`${id}-body`}
        className="card flex w-full max-w-[460px] flex-col gap-5 p-6"
        initial={{ opacity: 0, y: 8, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 4, scale: 0.98 }}
        transition={{ duration: 0.18, ease: "easeOut" }}
        data-safety-dialog={id}
      >
        <div className="flex items-start gap-4">
          <span
            className={`grid h-10 w-10 shrink-0 place-items-center rounded-full ${
              tone === "warn" ? "bg-warn/15 text-warn" : "bg-surface-2 text-ink-2"
            }`}
          >
            <Icon size={19} />
          </span>
          <h2 id={`${id}-title`} className="self-center font-display text-[20px] leading-tight font-semibold">
            {title}
          </h2>
        </div>
        {children}
      </motion.div>
    </motion.div>
  );
}
