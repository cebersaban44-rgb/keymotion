import { AlertTriangle, Check, FolderOpen, Loader2 } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState, type ChangeEvent, type ReactNode } from "react";
import type { BackupExported } from "../bindings/BackupExported";
import type { BackupImported } from "../bindings/BackupImported";
import type { BackupSection } from "../bindings/BackupSection";
import type { BackupSectionInfo } from "../bindings/BackupSectionInfo";
import type { BackupSummary } from "../bindings/BackupSummary";
import { useCalendar } from "../lib/calendar";
import { friendlyError } from "../lib/errors";
import { useGauges } from "../lib/gauges";
import { useT, type MessageKey, type Translate } from "../lib/i18n";
import { cancelImport, exportBackup, importBackup, inspectBackup, logUiError, revealExport } from "../lib/ipc";
import { stageKeymapImport } from "../lib/keymap";
import { initLighting } from "../lib/lighting";
import { stageMacrosImport } from "../lib/macros";
import { usePomodoro } from "../lib/pomodoro";
import { useReminders } from "../lib/reminders";
import { useScreen } from "../lib/screen";
import { useApp } from "../lib/store";
import { Toggle } from "./controls";

/** Larger files are not read (backup.rs MAX_BYTES). */
const MAX_FILE_BYTES = 256 * 1024 * 1024;

const SECTION_LABEL: Record<BackupSection, MessageKey> = {
  lighting: "nav.lighting",
  keymap: "nav.keymap",
  macros: "nav.macros",
  screen: "sc.library",
  calibration: "cal.title",
  clock: "clock.title",
  focus: "nav.focus",
  gauges: "nav.gauges",
  reminders: "nav.reminders",
  calendar: "data.section.calendar",
};

export const secondaryButton =
  "flex h-9 shrink-0 items-center gap-2 rounded-lg border border-line bg-bg-raise px-3.5 text-[13px] text-ink transition-colors hover:border-line-strong disabled:cursor-not-allowed disabled:opacity-40";

/** A settings row: name and an optional one-line explanation, the control on the right. */
export function Row({ label, hint, children }: { label: string; hint?: string; children?: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-t border-line py-4 first:border-t-0">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-[13.5px]">{label}</span>
        {hint && <span className="text-[12.5px] text-ink-2">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

function localDate(ms = Date.now()) {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function importErrorText(e: unknown, t: Translate) {
  const raw = e instanceof Error ? e.message : String(e);
  if (raw === "not a backup") return t("data.import.err.notBackup");
  if (raw === "backup from a newer version") return t("data.import.err.newer");
  if (raw === "backup damaged") return t("data.import.err.damaged");
  if (raw === "backup too large") return t("data.import.err.tooLarge");
  return friendlyError(e, t);
}

/** What a section of the file holds and what restoring it does. */
function sectionHint(info: BackupSectionInfo, screenNew: number, t: Translate) {
  const count = info.count;
  switch (info.section) {
    case "lighting":
    case "keymap":
      return t("data.section.draft");
    case "macros":
      return t("data.section.macros", { count });
    case "screen":
      return screenNew > 0 ? t("data.section.screen", { count: screenNew }) : t("data.section.screen.none");
    case "reminders":
      return t("data.section.reminders", { count });
    case "calendar":
      return count > 0 ? t("data.section.calendar.with", { count }) : t("data.section.calendar.settings");
    default:
      return t("data.section.replace");
  }
}

/** Pages whose stores hold what an import replaced read it again. */
async function refreshAfterImport(result: BackupImported) {
  for (const k of result.keymaps) stageKeymapImport(k.modelId, k.state);
  for (const m of result.macros) stageMacrosImport(m.modelId, m.slots);
  await initLighting();
  const modelId = useScreen.getState().modelId;
  const loads = [
    modelId ? useScreen.getState().load(modelId) : Promise.resolve(),
    usePomodoro.getState().load(),
    useGauges.getState().load(),
    useReminders.getState().load(),
    useCalendar.getState().load(),
  ];
  await Promise.all(loads.map((p) => p.catch(console.error)));
}

/**
 * Settings → Backup: export everything to one file, restore from one.
 * Restoring shows what the file holds first; nothing goes to the keyboard.
 */
export function BackupCard({ onImported }: { onImported: () => void }) {
  const t = useT();
  const hasCalendars = useCalendar((s) => (s.state?.sources.length ?? 0) > 0);
  const uploading = useScreen((s) => s.upload !== null);
  const [withAddresses, setWithAddresses] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exported, setExported] = useState<BackupExported | null>(null);
  const [exportError, setExportError] = useState(false);
  const [reading, setReading] = useState(false);
  const [summary, setSummary] = useState<BackupSummary | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [done, setDone] = useState<{ result: BackupImported; drafts: boolean } | null>(null);
  const input = useRef<HTMLInputElement>(null);

  const doExport = async () => {
    setExporting(true);
    setExportError(false);
    setExported(null);
    try {
      const name = t("data.export.fileName", { app: t("app.name"), date: localDate() });
      setExported(await exportBackup(withAddresses && hasCalendars, name));
    } catch (e) {
      console.error(e);
      void logUiError(`export failed: ${e instanceof Error ? e.message : String(e)}`);
      setExportError(true);
    } finally {
      setExporting(false);
    }
  };

  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    // The same file can be chosen again after a cancel.
    e.target.value = "";
    if (!file) return;
    setImportError(null);
    setDone(null);
    if (file.size > MAX_FILE_BYTES) {
      setImportError(t("data.import.err.tooLarge"));
      return;
    }
    setReading(true);
    try {
      setSummary(await inspectBackup(await file.arrayBuffer()));
    } catch (err) {
      setImportError(importErrorText(err, t));
    } finally {
      setReading(false);
    }
  };

  const cancel = () => {
    setSummary(null);
    cancelImport().catch(console.error);
  };

  const restore = async (sections: BackupSection[]) => {
    const result = await importBackup(sections);
    await refreshAfterImport(result);
    const drafts = sections.some((s) => s === "lighting" || s === "keymap" || s === "macros");
    setSummary(null);
    setDone({ result, drafts });
    onImported();
  };

  return (
    <section className="card px-5 py-1">
      <div className="eyebrow pt-4 pb-1">{t("data.backup")}</div>
      <Row label={t("data.export")} hint={t("data.export.hint")}>
        <button type="button" onClick={doExport} disabled={exporting} className={secondaryButton}>
          {exporting && <Loader2 size={14} className="animate-spin" />}
          {exporting ? t("data.export.busy") : t("data.export.button")}
        </button>
      </Row>
      {hasCalendars && (
        <Row label={t("data.export.addresses")} hint={t("data.export.addresses.hint")}>
          <Toggle checked={withAddresses} onChange={setWithAddresses} label={t("data.export.addresses")} />
        </Row>
      )}
      {(exported || exportError) && (
        <div className="flex items-center justify-between gap-4 pb-4 text-[13px]" role="status">
          {exported ? (
            <>
              <span className="flex items-center gap-2 text-ink-2">
                <Check size={14} className="shrink-0 text-signal" />
                {t(exported.inDownloads ? "data.export.done" : "data.export.doneElsewhere", { file: exported.file })}
              </span>
              <button type="button" onClick={() => revealExport().catch(console.error)} className={secondaryButton}>
                <FolderOpen size={14} />
                {t("data.export.show")}
              </button>
            </>
          ) : (
            <span className="flex items-start gap-2 text-danger">
              <AlertTriangle size={14} className="mt-0.5 shrink-0" />
              {t("data.export.failed")}
            </span>
          )}
        </div>
      )}
      <Row label={t("data.import")} hint={uploading ? t("data.import.waitUpload") : t("data.import.hint")}>
        <button
          type="button"
          onClick={() => input.current?.click()}
          disabled={reading || uploading}
          className={secondaryButton}
        >
          {reading && <Loader2 size={14} className="animate-spin" />}
          {reading ? t("data.import.reading") : t("data.import.button")}
        </button>
        <input ref={input} type="file" accept=".json,application/json" className="hidden" onChange={onFile} />
      </Row>
      {importError && (
        <p role="alert" className="flex items-start gap-2 pb-4 text-[13px] text-danger">
          <AlertTriangle size={14} className="mt-0.5 shrink-0" />
          {importError}
        </p>
      )}
      {done && (
        <div className="flex flex-col gap-1 pb-4 text-[13px] text-ink-2" role="status">
          <span className="flex items-center gap-2 text-ink">
            <Check size={14} className="shrink-0 text-signal" />
            {t("data.import.done")}
          </span>
          {done.drafts && <span className="pl-[22px]">{t("data.import.done.drafts")}</span>}
          {done.result.skipped > 0 && (
            <span className="pl-[22px]">{t("data.import.done.skipped", { count: done.result.skipped })}</span>
          )}
          {done.result.failed.length > 0 && (
            <span className="pl-[22px] text-danger">
              {t("data.import.done.failed", { list: done.result.failed.map((s) => t(SECTION_LABEL[s])).join(", ") })}
            </span>
          )}
        </div>
      )}
      <AnimatePresence>
        {summary && <ImportDialog summary={summary} onCancel={cancel} onRestore={restore} />}
      </AnimatePresence>
    </section>
  );
}

function ImportDialog({
  summary,
  onCancel,
  onRestore,
}: {
  summary: BackupSummary;
  onCancel: () => void;
  onRestore: (sections: BackupSection[]) => Promise<void>;
}) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const [chosen, setChosen] = useState<BackupSection[]>(() => summary.sections.map((s) => s.section));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    // Replacing settings is not undone: the safe answer has the focus.
    cancelRef.current?.focus();
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

  const toggle = (s: BackupSection, on: boolean) =>
    setChosen((list) => (on ? [...list.filter((x) => x !== s), s] : list.filter((x) => x !== s)));

  const restore = async () => {
    setBusy(true);
    setError(null);
    try {
      await onRestore(chosen);
    } catch (e) {
      setError(importErrorText(e, t));
      setBusy(false);
    }
  };

  const date = summary.createdMs
    ? new Date(summary.createdMs).toLocaleDateString(locale === "tr" ? "tr-TR" : "en-US", { dateStyle: "long" })
    : null;

  return (
    <motion.div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-6 backdrop-blur-[2px]"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
    >
      <motion.div
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-title"
        aria-describedby="import-body"
        className="card flex max-h-full w-full max-w-[520px] flex-col gap-5 overflow-y-auto p-6"
        initial={{ opacity: 0, y: 8, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 4, scale: 0.98 }}
        transition={{ duration: 0.18, ease: "easeOut" }}
      >
        <div className="flex flex-col gap-1.5">
          <h2 id="import-title" className="font-display text-[20px] leading-tight font-semibold">
            {t("data.import.title")}
          </h2>
          <p id="import-body" className="text-[13px] leading-relaxed text-ink-2">
            {date && `${t("data.import.from", { date })} `}
            {t("data.import.body")}
          </p>
        </div>

        <ul className="flex flex-col rounded-lg border border-line">
          {summary.sections.map((info) => {
            const on = chosen.includes(info.section);
            return (
              <li key={info.section} className="border-t border-line first:border-t-0">
                <label className="flex cursor-pointer items-start gap-3 px-3.5 py-3">
                  <input
                    type="checkbox"
                    checked={on}
                    disabled={busy}
                    onChange={(e) => toggle(info.section, e.target.checked)}
                    className="mt-0.5 h-4 w-4 shrink-0 accent-[var(--signal)]"
                  />
                  <span className="flex flex-col gap-0.5">
                    <span className="text-[13.5px]">{t(SECTION_LABEL[info.section])}</span>
                    <span className="text-[12.5px] text-ink-2">{sectionHint(info, summary.screenNew, t)}</span>
                  </span>
                </label>
              </li>
            );
          })}
        </ul>

        {error && (
          <p role="alert" className="flex items-start gap-2 text-[13px] text-danger">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            {error}
          </p>
        )}

        <div className="flex items-center justify-end gap-2">
          <button
            ref={cancelRef}
            type="button"
            onClick={onCancel}
            disabled={busy}
            className="h-9 rounded-lg px-3.5 text-[13px] text-ink-2 transition-colors hover:text-ink disabled:opacity-40"
          >
            {t("unsaved.cancel")}
          </button>
          <button
            type="button"
            onClick={restore}
            disabled={busy || chosen.length === 0}
            className="flex h-9 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink transition-transform active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-40"
          >
            {busy && <Loader2 size={14} className="animate-spin" />}
            {busy ? t("data.import.restoring") : t("data.import.restore")}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
