// "Canlı ekran": the live mode switch, its upload budget and the sources
// (phase C). The backend runs the live mode; this only shows and sets it.

import { AlertTriangle, ChevronRight, Clock3, Loader2, Music2, Radio, RotateCcw, Timer, Upload, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import type { LivePlan } from "../../bindings/LivePlan";
import type { LiveRestore } from "../../bindings/LiveRestore";
import type { LiveState } from "../../bindings/LiveState";
import { Segmented, Toggle } from "../../components/controls";
import { useT, type MessageKey } from "../../lib/i18n";
import { fmtWait, useLive } from "../../lib/live";
import { useApp } from "../../lib/store";
import { fmtClock, phaseColor, remainingNow, usePomodoro } from "../../lib/pomodoro";
import { uploadSeconds } from "../../lib/screenImport";
import { FramePlayer, Hint, fmtSeconds, loadFrames, pickThumb, useNow } from "./common";

const PLANS: { id: LivePlan; label: MessageKey; years: MessageKey }[] = [
  { id: "saver", label: "sc.live.plan.saver", years: "sc.live.years.saver" },
  { id: "balanced", label: "sc.live.plan.balanced", years: "sc.live.years.balanced" },
  { id: "heavy", label: "sc.live.plan.heavy", years: "sc.live.years.heavy" },
];

export function LivePanel({ usb }: { usb: boolean }) {
  const t = useT();
  const s = useLive((x) => x.state);
  const drawError = useLive((x) => x.drawError);
  const configure = useLive((x) => x.configure);
  const [closing, setClosing] = useState(false);
  useNow(); // refresh the waiting times
  if (!s) return null;
  const plan = PLANS.find((p) => p.id === s.plan) ?? PLANS[1]!;
  const set = (patch: Parameters<typeof configure>[0]) => configure(patch).catch(console.error);

  return (
    <section className="card flex flex-col gap-5 p-5" data-live-panel>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="max-w-[72ch]">
          <div className="mb-1.5 flex items-center gap-2">
            <Radio size={14} className={s.enabled ? "text-ok" : "text-ink-3"} />
            <span className="eyebrow">{t("sc.live.eyebrow")}</span>
          </div>
          <h2 className="font-display text-[22px] font-semibold tracking-[-0.02em]">{t("sc.live.title")}</h2>
          <p className="mt-1 text-[12.5px] text-ink-2">{t("sc.live.body")}</p>
        </div>
        <label className="flex items-center gap-3 rounded-xl border border-line px-3 py-2">
          <span className="text-[13px] font-semibold">{t("sc.live.mode")}</span>
          <Toggle
            checked={s.enabled}
            onChange={(enabled) => {
              // Turning off with a live card on the screen: ask what to show.
              if (!enabled && s.restore) setClosing(true);
              else set({ enabled });
            }}
            label={t("sc.live.mode")}
          />
        </label>
      </div>

      <div className="grid grid-cols-[minmax(300px,1fr)_minmax(300px,1fr)] gap-6">
        <div className="flex flex-col gap-3">
          <span className="text-[12.5px] font-medium">{t("sc.live.plan")}</span>
          <Segmented<LivePlan>
            label={t("sc.live.plan")}
            value={s.plan}
            onChange={(p) => set({ plan: p })}
            options={PLANS.map((p) => ({ value: p.id, label: t(p.label) }))}
          />
          <BudgetLine s={s} />
          <p className="text-[11.5px] text-ink-3">{t(plan.years)}</p>
          {s.plan === "heavy" && <Hint tone="warn">{t("sc.live.heavyWarn")}</Hint>}
        </div>

        <div className="flex flex-col gap-3">
          <span className="text-[12.5px] font-medium">{t("sc.live.sources")}</span>
          <div className="flex flex-col gap-2 rounded-xl border border-line p-3">
            <div className="flex items-start justify-between gap-3">
              <span className="flex items-start gap-2.5">
                <Music2 size={16} className="mt-0.5 shrink-0 text-signal" />
                <span className="flex flex-col">
                  <span className="text-[13px] font-semibold">{t("sc.live.np")}</span>
                  <span className="text-[11.5px] text-ink-3">{t("sc.live.np.body")}</span>
                </span>
              </span>
              <Toggle checked={s.nowPlaying} onChange={(nowPlaying) => set({ nowPlaying })} label={t("sc.live.np")} />
            </div>
            {s.enabled && s.nowPlaying && <TrackLine s={s} />}
          </div>
          <PomodoroSource />
        </div>
      </div>

      <StatusLine s={s} usb={usb} />
      {!s.enabled && s.restore && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-line px-4 py-3" data-live-leftover>
          <span className="text-[12.5px] text-ink-2">
            {t("sc.live.leftover")} <span className="font-medium text-ink">{s.restore.name}</span>
          </span>
          <button
            type="button"
            onClick={() => setClosing(true)}
            className="flex h-9 items-center gap-2 rounded-lg border border-signal/50 px-3 text-[12.5px] font-semibold text-signal hover:bg-signal/10"
          >
            <RotateCcw size={14} /> {t("sc.live.offRestore")}
          </button>
        </div>
      )}
      {drawError && <Hint tone="error">{drawError}</Hint>}
      {s.enabled && (
        <div className="flex flex-col gap-1">
          <Hint tone="note">{t("sc.live.seeGif")}</Hint>
          <Hint tone="note">{t("sc.live.appOpen")}</Hint>
        </div>
      )}
      <AnimatePresence>
        {closing && s.restore && (
          <TurnOff
            restore={s.restore}
            alreadyOff={!s.enabled}
            usb={usb}
            onCancel={() => setClosing(false)}
            onChoose={(restore) => {
              setClosing(false);
              set({ enabled: false, restore });
            }}
          />
        )}
      </AnimatePresence>
    </section>
  );
}

/** Live mode off while a live card is on the screen: put the previous
 *  picture back (an upload), or leave the card. */
function TurnOff({
  restore,
  alreadyOff,
  usb,
  onCancel,
  onChoose,
}: {
  restore: LiveRestore;
  /** Opened from the leftover-card line: only putting back is offered. */
  alreadyOff: boolean;
  usb: boolean;
  onCancel: () => void;
  onChoose: (restore: boolean) => void;
}) {
  const t = useT();
  const [thumb, setThumb] = useState<Uint16Array | null>(null);
  useEffect(() => {
    let live = true;
    loadFrames(restore.id, restore.frames === 1).then((d) => live && setThumb(pickThumb(d.frames)), console.error);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      live = false;
      window.removeEventListener("keydown", onKey);
    };
  }, [restore.id, restore.frames, onCancel]);
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      role="dialog"
      aria-modal
      aria-labelledby="live-off-title"
    >
      <motion.div initial={{ scale: 0.97, y: 8 }} animate={{ scale: 1, y: 0 }} className="card flex w-[500px] flex-col gap-4 p-6">
        <div className="flex items-start justify-between gap-3">
          <h2 id="live-off-title" className="font-display text-[22px] font-semibold tracking-[-0.02em]">
            {alreadyOff ? t("sc.live.restoreTitle") : t("sc.live.offTitle")}
          </h2>
          <button
            type="button"
            onClick={onCancel}
            aria-label={t("sc.close")}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-ink-3 hover:bg-surface-2 hover:text-ink"
          >
            <X size={16} />
          </button>
        </div>
        <p className="text-[12.5px] text-ink-2">{t("sc.live.offBody")}</p>
        <div className="flex items-center gap-4 rounded-xl border border-line p-3">
          {thumb ? (
            <FramePlayer frames={[thumb]} delaysMs={[100]} className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />
          ) : (
            <div className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />
          )}
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="truncate text-[13px] font-semibold">{restore.name}</span>
            <span className="num text-[11.5px] text-ink-3">
              {restore.frames} {t("sc.frames")} · {t("sc.uploadTime")} ~{fmtSeconds(uploadSeconds(restore.frames))}
            </span>
          </div>
        </div>
        {!usb && <Hint tone="usb">{t("sc.live.offNoUsb")}</Hint>}
        <div className="flex flex-wrap justify-end gap-2 pt-1">
          <button
            type="button"
            onClick={() => (alreadyOff ? onCancel() : onChoose(false))}
            className="h-10 rounded-xl border border-line px-4 text-[13px] text-ink-2 hover:bg-surface-2"
          >
            {alreadyOff ? t("sc.cancel") : t("sc.live.offKeep")}
          </button>
          <button
            type="button"
            autoFocus
            onClick={() => onChoose(true)}
            className="flex h-10 items-center gap-2 rounded-xl bg-signal px-4 text-[13px] font-semibold text-signal-ink"
          >
            <RotateCcw size={15} /> {t("sc.live.offRestore")}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}

function BudgetLine({ s }: { s: LiveState }) {
  const t = useT();
  const ready = Math.max(0, Math.floor(s.tokens + 1e-6));
  const pct = Math.max(0, Math.min(100, (s.tokens / s.burst) * 100));
  const units = { h: t("sc.live.h"), m: t("sc.live.m") };
  return (
    <div className="flex flex-col gap-1.5" data-budget>
      <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
        <div className="h-full rounded-full bg-ok transition-[width]" style={{ width: `${pct}%` }} />
      </div>
      <div className="num flex justify-between text-[11.5px] text-ink-2">
        <span>
          {ready} / {s.burst} {t("sc.live.ready")}
        </span>
        <span>
          {s.nextTokenMs != null
            ? `${t("sc.live.next")} ${fmtWait(s.nextTokenMs, units)}`
            : ready >= s.burst
              ? t("sc.live.full")
              : `${t("sc.live.refill")} ${fmtWait(3_600_000 / (s.perDay / 24), units)}`}
        </span>
      </div>
    </div>
  );
}

function TrackLine({ s }: { s: LiveState }) {
  const t = useT();
  const tr = s.track;
  if (!tr) return <p className="text-[12px] text-ink-3">{s.held ? t("sc.live.np.held") : t("sc.live.np.none")}</p>;
  const state = s.held
    ? t("sc.live.np.held")
    : !tr.playing
      ? t("sc.live.np.paused")
      : tr.settled
        ? t("sc.live.np.settled")
        : t("sc.live.np.settling");
  return (
    <div className="flex flex-col gap-0.5 rounded-lg bg-surface-2/60 px-2.5 py-2" data-track>
      <span className="truncate text-[12.5px] font-medium">
        {tr.artist ? `${tr.artist} — ${tr.title}` : tr.title}
      </span>
      <span className="text-[11px] text-ink-3">
        {tr.app ? `${tr.app} · ` : ""}
        {state}
      </span>
    </div>
  );
}

/** The focus timer as a source: whether it sends cards and where it is.
 *  Its settings (the card switch too) are on its own page, so there is one
 *  place to set them. */
function PomodoroSource() {
  const t = useT();
  const p = usePomodoro((x) => x.state);
  const setPage = useApp((x) => x.setPage);
  if (!p) return null;
  const idle = p.status === "idle";
  const cards = p.settings.screen;
  const phase =
    p.phase === "focus" ? t("pomo.phase.focus") : p.phase === "shortBreak" ? t("pomo.phase.shortBreak") : t("pomo.phase.longBreak");
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-line p-3" data-live-pomodoro>
      <div className="flex items-start justify-between gap-3">
        <span className="flex items-start gap-2.5">
          <Timer size={16} className="mt-0.5 shrink-0 text-signal" />
          <span className="flex flex-col">
            <span className="text-[13px] font-semibold">{t("sc.live.pomo")}</span>
            <span className="text-[11.5px] text-ink-3">{t("sc.live.pomo.body")}</span>
          </span>
        </span>
        <span
          className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium ${cards ? "bg-ok/15 text-ok" : "bg-surface-2 text-ink-3"}`}
          data-live-pomodoro-cards={cards ? "on" : "off"}
        >
          {cards ? t("sc.live.pomo.on") : t("sc.live.pomo.off")}
        </span>
      </div>
      <button
        type="button"
        onClick={() => setPage("focus")}
        className="flex items-center justify-between gap-2 rounded-lg bg-surface-2/60 px-2.5 py-2 text-left text-[12.5px] hover:bg-surface-2"
      >
        <span className="flex items-center gap-2">
          <span
            className="h-2 w-2 rounded-full"
            style={{ background: idle ? "var(--ink-3)" : phaseColor(p.settings, p.phase) }}
            aria-hidden
          />
          {idle ? (
            <span className="text-ink-3">{t("pomo.idle")}</span>
          ) : (
            <span className="num">
              {phase} · {fmtClock(remainingNow(p))}
              {p.status === "paused" && <span className="text-ink-3"> · {t("pomo.paused")}</span>}
            </span>
          )}
        </span>
        <span className="flex items-center gap-1 text-[12px] font-medium text-signal">
          {t("sc.live.pomo.open")} <ChevronRight size={14} />
        </span>
      </button>
    </div>
  );
}

function StatusLine({ s, usb }: { s: LiveState; usb: boolean }) {
  const t = useT();
  const units = { h: t("sc.live.h"), m: t("sc.live.m") };
  const row = "flex items-center gap-2 text-[12.5px]";
  if (!s.enabled && !s.uploading) return <p className="text-[12.5px] text-ink-3">{t("sc.live.off")}</p>;
  return (
    <div className="flex flex-col gap-1.5 border-t border-line pt-4" data-live-status>
      {s.uploading && (
        <p className={`${row} text-ink`}>
          <Loader2 size={14} className="animate-spin text-signal" />
          {s.uploading.source === "restore" ? t("sc.live.restoring") : t("sc.live.uploading")}:{" "}
          <span className="truncate font-medium">{s.uploading.name}</span>
        </p>
      )}
      {s.pending && (
        <p className={`${row} text-ink-2`}>
          <Clock3 size={14} className="text-warn" />
          {t("sc.live.pending")}: <span className="truncate font-medium">{s.pending.name}</span>
          <span className="text-ink-3">
            ·{" "}
            {!usb
              ? t("sc.live.waitCable")
              : s.nextTokenMs != null
                ? `${t("sc.live.waitBudget")} ${fmtWait(s.nextTokenMs, units)}`
                : t("sc.live.waitTurn")}
          </span>
        </p>
      )}
      {s.last ? (
        <p className={`${row} text-ink-2`}>
          <Upload size={14} className="text-ok" />
          {t("sc.live.last")}: <span className="truncate font-medium">{s.last.name}</span>
          <span className="num text-ink-3">
            · {new Date(s.last.atMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </span>
        </p>
      ) : (
        !s.pending && !s.uploading && <p className="text-[12.5px] text-ink-3">{t("sc.live.nothingYet")}</p>
      )}
      {s.lastError && !s.uploading && (
        <p className={`${row} text-warn`}>
          <AlertTriangle size={14} /> {t("sc.live.failed")}: {s.lastError}
        </p>
      )}
    </div>
  );
}
