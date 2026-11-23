// The Connection page's list of an "AA" keyboard's backups (the AK980 PRO on
// firmware 3.01): Keymotion saves one before it first writes to the
// keyboard and one before every restore (lib/aa.ts). A backup of the
// keyboard on the cable can be put back, in visible steps with a Cancel:
//
//   1. What comes back (and what doesn't) → Restore
//   2. The lighting's colour, only when the backup doesn't know it and the
//      keyboard refused without one → Restore
//   3. Done: what was put back, the keys left as they are → OK, and the
//      list again with "the backup is back on the keyboard".
//
// After a restore the app's own records (Lighting, Key mapping, Macros)
// still hold what Keymotion last saved; the done step says so rather than
// pretend the pages show the keyboard.

import { ArchiveRestore, Check, CircleAlert, History, Loader2, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import type { KeyboardBackupInfo } from "../bindings/KeyboardBackupInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { KeyboardRestoreResult } from "../bindings/KeyboardRestoreResult";
import { getKeyboardBackups, restoreKeyboardBackup, writeBlock } from "../lib/aa";
import { keptWhy, restoredParts, type KeptWhy, type RestoredPart } from "../lib/aaText";
import { friendlyError } from "../lib/errors";
import { intlTag, useT, type MessageKey } from "../lib/i18n";
import { useLighting } from "../lib/lighting";
import { useApp } from "../lib/store";
import { ColorPicker, InfoBar, Toggle } from "./controls";

const PART: Record<RestoredPart, MessageKey> = {
  base: "aa.rs.part.base",
  fn: "aa.rs.part.fn",
  effect: "aa.rs.part.effect",
  perKey: "aa.rs.part.perKey",
  macros: "aa.rs.part.macros",
};
const ALL_PARTS: RestoredPart[] = ["base", "fn", "effect", "perKey", "macros"];

const KEPT: Record<KeptWhy, MessageKey> = {
  fn: "aa.rs.kept.fn",
  knob: "aa.rs.kept.knob",
  fixed: "aa.rs.kept.fixed",
  function: "aa.rs.kept.function",
  other: "aa.rs.kept.other",
};

type Step = "check" | "color" | "done";

type Flow =
  | { step: "check"; backup: KeyboardBackupInfo; busy: boolean; error: string | null }
  | { step: "color"; backup: KeyboardBackupInfo; busy: boolean; error: string | null; color: string; rainbow: boolean }
  | { step: "done"; backup: KeyboardBackupInfo; result: KeyboardRestoreResult; askedColor: boolean };

/** The keyboard refused a restore for want of the effect's colour. */
const COLOR_NEEDED = /does not report its effect colou?r/i;

export function KeyboardBackups({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const [list, setList] = useState<KeyboardBackupInfo[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [flow, setFlow] = useState<Flow | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const blocked = writeBlock(kb, t);

  const reload = useCallback(() => {
    getKeyboardBackups().then(
      (l) => {
        setList(l);
        setLoadError(null);
      },
      (e) => setLoadError(friendlyError(e, t)),
    );
    // The text of a failure is fixed when it happens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // Again when the keyboard's backup changes (the first one is saved
  // when the keyboard is first read).
  useEffect(reload, [reload, kb.aaStatus?.backup]);

  const when = (ms: number) => new Date(ms).toLocaleString(intlTag(locale), { dateStyle: "medium", timeStyle: "short" });

  const run = async (f: Extract<Flow, { step: "check" | "color" }>) => {
    setFlow({ ...f, busy: true, error: null });
    const color = f.step === "color" ? { color: f.color, rainbow: f.rainbow } : undefined;
    try {
      const result = await restoreKeyboardBackup(f.backup.file, color);
      setFlow({ step: "done", backup: f.backup, result, askedColor: f.step === "color" });
      reload();
    } catch (e) {
      const raw = e instanceof Error ? e.message : String(e);
      if (f.step === "check" && COLOR_NEEDED.test(raw)) {
        // Nothing was written: the same backup goes again with a colour.
        const known = useLighting.getState().appliedEffect;
        setFlow({
          step: "color",
          backup: f.backup,
          busy: false,
          error: null,
          color: known?.color ?? "#ffffff",
          rainbow: false,
        });
        return;
      }
      setFlow({ ...f, busy: false, error: friendlyError(e, t) });
      reload();
    }
  };

  const close = (done?: KeyboardBackupInfo) => {
    setFlow(null);
    if (done) setNotice(t("aa.rs.doneNotice", { date: when(done.savedAtMs) }));
    window.setTimeout(() => titleRef.current?.focus(), 0);
  };

  // Newest first: the one most likely wanted on top.
  const shown = list ? [...list].sort((a, b) => b.savedAtMs - a.savedAtMs) : [];

  return (
    <section className="card flex flex-col gap-3 p-5" data-keyboard-backups>
      <h2 ref={titleRef} tabIndex={-1} className="eyebrow outline-none">
        {t("aa.bk.title")}
      </h2>
      <p className="max-w-[80ch] text-[13px] text-ink-2">{t("aa.bk.desc")}</p>

      {notice && !flow && (
        <InfoBar
          tone="ok"
          live
          title={notice}
          data="aa-restored"
          actions={
            <button type="button" onClick={() => setNotice(null)} aria-label={t("report.close")} className="btn-subtle btn-sm !px-2">
              <X size={14} />
            </button>
          }
        />
      )}
      {loadError && <InfoBar tone="error" title={loadError} />}

      {flow ? (
        <RestoreFlow
          kb={kb}
          flow={flow}
          date={when(flow.backup.savedAtMs)}
          blocked={blocked}
          onChange={setFlow}
          onRun={run}
          onClose={close}
        />
      ) : list && shown.length === 0 ? (
        <p className="text-[13px] text-ink-3">{t("aa.bk.none")}</p>
      ) : (
        <ul className="flex flex-col" data-backup-list>
          {shown.map((b) => (
            <li key={b.file} className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-line py-2.5 first:border-t-0" data-backup={b.file}>
              <History size={16} className="shrink-0 text-ink-3" aria-hidden />
              <span className="flex min-w-[220px] flex-1 flex-col">
                <span className="num text-[13px] font-medium">{when(b.savedAtMs)}</span>
                <span className="text-[12px] text-ink-2">
                  {t(b.reason === "first-connect" ? "aa.bk.reason.first" : "aa.bk.reason.restore")} ·{" "}
                  <span className="num">{t("overview.firmware.short", { version: b.firmware })}</span>
                  {!b.matches && <> · {t("aa.bk.other")}</>}
                </span>
              </span>
              {b.matches && (
                <button
                  type="button"
                  className="btn-secondary btn-sm"
                  disabled={!!blocked || kb.connection !== "usb"}
                  title={blocked ?? undefined}
                  onClick={() => {
                    setNotice(null);
                    setFlow({ step: "check", backup: b, busy: false, error: null });
                  }}
                  data-backup-restore
                >
                  <ArchiveRestore size={14} aria-hidden />
                  {t("aa.bk.restore")}
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      {!flow && blocked && list && list.some((b) => b.matches) && <p className="text-[12px] text-ink-3">{blocked}</p>}
    </section>
  );
}

function RestoreFlow({
  kb,
  flow,
  date,
  blocked,
  onChange,
  onRun,
  onClose,
}: {
  kb: KeyboardInfo;
  flow: Flow;
  date: string;
  blocked: string | null;
  onChange: (f: Flow) => void;
  onRun: (f: Extract<Flow, { step: "check" | "color" }>) => Promise<void>;
  onClose: (done?: KeyboardBackupInfo) => void;
}) {
  const t = useT();
  const busy = flow.step !== "done" && flow.busy;
  const askedColor = flow.step === "color" || (flow.step === "done" && flow.askedColor);
  const steps: Step[] = askedColor ? ["check", "color", "done"] : ["check", "done"];
  const at = steps.indexOf(flow.step);
  const stepLabel: Record<Step, MessageKey> = { check: "aa.rs.step.check", color: "aa.rs.step.color", done: "aa.rs.step.done" };
  const title = t("aa.rs.title", { date });
  const primary = useRef<HTMLButtonElement>(null);
  const box = useRef<HTMLElement>(null);
  // Each step in view (the list sits at the bottom of the page), its main
  // button focused.
  useEffect(() => {
    box.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    primary.current?.focus({ preventScroll: true });
  }, [flow.step]);

  // Escape is Cancel, except while the keyboard is being written.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) {
        e.preventDefault();
        onClose(flow.step === "done" ? flow.backup : undefined);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, flow, onClose]);

  let body: ReactNode;
  let actions: ReactNode;
  if (flow.step === "check") {
    body = (
      <>
        <p className="text-ink-2">{t("aa.rs.what")}</p>
        <ul className="flex flex-col gap-1.5">
          {ALL_PARTS.map((p) => (
            <li key={p} className="flex items-start gap-2">
              <Check size={15} className="mt-0.5 shrink-0 text-ok" aria-hidden />
              {t(PART[p])}
            </li>
          ))}
        </ul>
        <p className="text-ink-2">{t("aa.rs.not")}</p>
        <p className="text-ink-2">{t("aa.rs.before")}</p>
      </>
    );
  } else if (flow.step === "color") {
    body = (
      <>
        <p className="text-ink-2">{t("aa.rs.color.body")}</p>
        <ColorPicker
          label={t("aa.rs.step.color")}
          value={flow.color}
          disabled={busy || flow.rainbow}
          onChange={(color) => onChange({ ...flow, color })}
        />
        <label className="flex items-center gap-3 text-[13px]">
          <Toggle label={t("light.rainbow")} checked={flow.rainbow} disabled={busy} onChange={(rainbow) => onChange({ ...flow, rainbow })} />
          <span className="flex flex-col">
            <span>{t("light.rainbow")}</span>
            <span className="text-[12px] text-ink-3">{t("aa.rs.color.rainbow")}</span>
          </span>
        </label>
      </>
    );
  } else {
    const parts = restoredParts(flow.result.written);
    const keys = kb.geometry?.keys ?? [];
    const layer = (l: string) => t(l === "fn" ? "km.layer.fn" : "km.layer.base");
    body = (
      <ul className="flex flex-col gap-2">
        <li className="flex items-start gap-2 font-semibold">
          <Check size={16} className="mt-0.5 shrink-0 text-ok" aria-hidden />
          {t("aa.rs.done.title")}
        </li>
        <li className="flex items-start gap-2" data-restore-written>
          <span className="w-4 shrink-0" aria-hidden />
          {parts.length > 0 ? (
            <span className="flex flex-col gap-1">
              <span>{t("aa.rs.done.written")}</span>
              <ul className="flex flex-col gap-0.5 text-[13px] text-ink-2">
                {parts.map((p) => (
                  <li key={p}>{t(PART[p])}</li>
                ))}
              </ul>
            </span>
          ) : (
            t("aa.rs.done.nothing")
          )}
        </li>
        {flow.result.kept.length > 0 && (
          <li className="flex items-start gap-2" data-restore-kept>
            <CircleAlert size={16} className="mt-0.5 shrink-0 text-ink-3" aria-hidden />
            <span className="flex flex-col gap-1">
              <span>{t("aa.rs.done.kept")}</span>
              <ul className="flex flex-col gap-0.5 text-[13px] text-ink-2">
                {flow.result.kept.map((k) => {
                  const key = k.led === null ? null : keys.find((g) => g.led === k.led);
                  const name = key ? key.label || key.id : t("aa.rs.kept.unnamed");
                  return (
                    <li key={`${k.layer}:${k.slot}`}>
                      <span className="font-medium text-ink">{name}</span> ({layer(k.layer)}): {t(KEPT[keptWhy(k.reason)])}
                    </li>
                  );
                })}
              </ul>
            </span>
          </li>
        )}
        <li className="flex items-start gap-2 text-ink-2">
          <span className="w-4 shrink-0" aria-hidden />
          {t("aa.rs.pages")}
        </li>
      </ul>
    );
  }

  if (flow.step === "done") {
    actions = (
      <button ref={primary} type="button" className="btn-primary" onClick={() => onClose(flow.backup)}>
        {t("ag.flow.ok")}
      </button>
    );
  } else {
    actions = (
      <>
        <button type="button" className="btn-secondary" disabled={busy} onClick={() => onClose()}>
          {t("sc.cancel")}
        </button>
        <button
          ref={primary}
          type="button"
          className="btn-primary"
          disabled={busy || !!blocked}
          title={blocked ?? undefined}
          onClick={() => void onRun(flow)}
          data-restore-go
        >
          {busy ? <Loader2 size={14} className="animate-spin" aria-hidden /> : <ArchiveRestore size={14} aria-hidden />}
          {busy ? t("aa.rs.busy") : t("aa.rs.go")}
        </button>
      </>
    );
  }

  return (
    <section ref={box} className="dlg !shadow-none" aria-label={title} data-restore-flow={flow.step}>
      <div className="flex flex-col gap-3.5 p-5 text-[14px]">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <h3 className="subtitle">{title}</h3>
          <ol className="flex items-center gap-2 text-[13px]" aria-label={title}>
            {steps.map((s, i) => (
              <li key={s} className="flex items-center gap-2" aria-current={i === at ? "step" : undefined}>
                {i > 0 && <span className="text-ink-3">›</span>}
                <span
                  className={`grid h-5 w-5 place-items-center rounded-full text-[11px] font-semibold ${
                    i === at
                      ? "bg-signal text-signal-ink"
                      : i < at
                        ? "bg-ok text-white"
                        : "shadow-[inset_0_0_0_1px_var(--ctl-strong)] text-ink-2"
                  }`}
                >
                  {i < at ? <Check size={12} strokeWidth={3} aria-hidden /> : i + 1}
                </span>
                <span className={i === at ? "font-semibold" : "text-ink-2"}>{t(stepLabel[s])}</span>
              </li>
            ))}
          </ol>
        </div>
        {body}
        {busy && <p className="text-[13px] text-ink-2" role="status">{t("aa.rs.cable")}</p>}
        {flow.step !== "done" && flow.error && <InfoBar tone="error" live title={flow.error} />}
        <div className="flex flex-wrap justify-end gap-2 pt-1">{actions}</div>
      </div>
    </section>
  );
}
