// "Pomodoro": the focus timer (phase C, second source). The backend runs it,
// lights the keys and asks for screen cards; this shows it and sets it.

import { Lightbulb, Minus, MonitorUp, Pause, Play, Plus, SkipForward, Square, Timer } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import type { PomodoroPhase } from "../../bindings/PomodoroPhase";
import type { PomodoroSettings } from "../../bindings/PomodoroSettings";
import type { PomodoroState } from "../../bindings/PomodoroState";
import { Toggle } from "../../components/controls";
import { useT, type MessageKey } from "../../lib/i18n";
import { useLive } from "../../lib/live";
import { LIMITS, PHASE_COLOR, fmtClock, remainingNow, usePomodoro } from "../../lib/pomodoro";
import { Hint } from "./common";

const PHASE_KEY: Record<PomodoroPhase, MessageKey> = {
  focus: "pomo.phase.focus",
  shortBreak: "pomo.phase.shortBreak",
  longBreak: "pomo.phase.longBreak",
};

/** Re-render this often while the timer runs. */
function useTicking(on: boolean, ms = 250) {
  const [, set] = useState(0);
  useEffect(() => {
    if (!on) return;
    const id = window.setInterval(() => set((n) => n + 1), ms);
    return () => window.clearInterval(id);
  }, [on, ms]);
}

export function PomodoroPanel({ usb }: { usb: boolean }) {
  const t = useT();
  const s = usePomodoro((x) => x.state);
  const drawError = usePomodoro((x) => x.drawError);
  const control = usePomodoro((x) => x.control);
  const configure = usePomodoro((x) => x.configure);
  const live = useLive((x) => x.state);
  useTicking(s?.status === "running");
  if (!s) return null;
  const set = (patch: Partial<PomodoroSettings>) => configure(patch).catch(console.error);
  const act = (a: Parameters<typeof control>[0]) => control(a).catch(console.error);
  const idle = s.status === "idle";
  const running = s.status === "running";

  return (
    <section className="card flex flex-col gap-5 p-5" data-pomodoro>
      <div className="max-w-[72ch]">
        <div className="mb-1.5 flex items-center gap-2">
          <Timer size={14} className={idle ? "text-ink-3" : "text-signal"} />
          <span className="eyebrow">{t("pomo.eyebrow")}</span>
        </div>
        <h2 className="font-display text-[22px] font-semibold tracking-[-0.02em]">{t("pomo.title")}</h2>
        <p className="mt-1 text-[12.5px] text-ink-2">{t("pomo.body")}</p>
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-6">
        <div className="flex min-w-0 flex-col items-center gap-4 rounded-2xl border border-line bg-bg-raise/50 px-4 py-6">
          <Dial s={s} />
          <KeyRow s={s} />
          <div className="flex flex-wrap items-center justify-center gap-2">
            <button
              type="button"
              onClick={() => act(running ? "pause" : "start")}
              className="flex h-10 min-w-[132px] items-center justify-center gap-2 rounded-xl bg-signal px-4 text-[13px] font-semibold text-signal-ink"
              data-pomo-main
            >
              {running ? <Pause size={15} /> : <Play size={15} />}
              {running ? t("pomo.pause") : idle ? t("pomo.start") : t("pomo.resume")}
            </button>
            {!idle && (
              <>
                <button
                  type="button"
                  onClick={() => act("skip")}
                  title={t("pomo.skip.hint")}
                  className="flex h-10 items-center gap-2 rounded-xl border border-line px-3.5 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
                  data-pomo-skip
                >
                  <SkipForward size={15} /> {t("pomo.skip")}
                </button>
                <button
                  type="button"
                  onClick={() => act("stop")}
                  title={t("pomo.stop.hint")}
                  className="flex h-10 items-center gap-2 rounded-xl border border-line px-3.5 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
                  data-pomo-stop
                >
                  <Square size={13} /> {t("pomo.stop")}
                </button>
              </>
            )}
          </div>
          {idle && s.last && <LastLine s={s} />}
        </div>

        <div className="flex flex-col gap-4">
          <div className="flex flex-col gap-2">
            <span className="text-[12.5px] font-medium">{t("pomo.lengths")}</span>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(196px,1fr))] gap-2">
              <LengthField label={t("pomo.len.focus")} unit={t("pomo.min")} value={s.settings.focusMin} limits={LIMITS.focusMin} onChange={(focusMin) => set({ focusMin })} />
              <LengthField label={t("pomo.len.short")} unit={t("pomo.min")} value={s.settings.shortMin} limits={LIMITS.shortMin} onChange={(shortMin) => set({ shortMin })} />
              <LengthField label={t("pomo.len.long")} unit={t("pomo.min")} value={s.settings.longMin} limits={LIMITS.longMin} onChange={(longMin) => set({ longMin })} />
              <LengthField label={t("pomo.len.rounds")} value={s.settings.rounds} limits={LIMITS.rounds} onChange={(rounds) => set({ rounds })} />
            </div>
            {!idle && <p className="text-[11.5px] text-ink-3">{t("pomo.lengths.note")}</p>}
          </div>

          <SwitchRow
            icon={<Lightbulb size={16} className="mt-0.5 shrink-0 text-signal" />}
            title={t("pomo.lights")}
            body={t("pomo.lights.body")}
            checked={s.settings.lights}
            onChange={(lights) => set({ lights })}
          />
          <SwitchRow
            icon={<MonitorUp size={16} className="mt-0.5 shrink-0 text-signal" />}
            title={t("pomo.screen")}
            body={t("pomo.screen.body")}
            checked={s.settings.screen}
            onChange={(screen) => set({ screen })}
          >
            {s.settings.screen && (live?.enabled ? <CostLine s={s} /> : <p className="text-[11.5px] text-ink-3">{t("pomo.screen.liveOff")}</p>)}
          </SwitchRow>
        </div>
      </div>

      {!usb && <Hint tone="usb">{t("pomo.noUsb")}</Hint>}
      {drawError && <Hint tone="error">{drawError}</Hint>}
    </section>
  );
}

/** Ring with the time left in the phase. */
function Dial({ s }: { s: PomodoroState }) {
  const t = useT();
  const idle = s.status === "idle";
  const left = idle ? s.settings.focusMin * 60_000 : remainingNow(s);
  const progress = idle || s.phaseMs <= 0 ? 0 : 1 - left / s.phaseMs;
  const color = PHASE_COLOR[s.phase];
  const r = 78;
  const len = 2 * Math.PI * r;
  return (
    <div className="relative h-[184px] w-[184px]" data-dial>
      <svg viewBox="0 0 184 184" className="absolute inset-0 -rotate-90" aria-hidden>
        <circle cx="92" cy="92" r={r} fill="none" stroke="var(--line)" strokeWidth="9" />
        <circle
          cx="92"
          cy="92"
          r={r}
          fill="none"
          stroke={color}
          strokeWidth="9"
          strokeLinecap="round"
          strokeDasharray={`${len * progress} ${len}`}
          style={{ filter: `drop-shadow(0 0 6px ${color}88)`, transition: "stroke-dasharray 250ms linear" }}
          opacity={progress > 0 ? 1 : 0}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-0.5">
        <span className="text-[11px] font-semibold tracking-[0.12em] uppercase" style={{ color: idle ? "var(--ink-3)" : color }}>
          {idle ? t("pomo.idle") : t(PHASE_KEY[s.phase])}
        </span>
        <span className="num font-display text-[44px] leading-none font-semibold tracking-[-0.03em]" data-time>
          {fmtClock(left)}
        </span>
        <span className="text-[11.5px] text-ink-3">
          {s.status === "paused"
            ? t("pomo.paused")
            : `${t("pomo.round")} ${idle ? 1 : s.round}/${s.settings.rounds}`}
        </span>
      </div>
    </div>
  );
}

/** What the keys show: Esc in the phase colour, F1–F12 filling up, the
 *  digit keys counting finished rounds (same rules as the backend). */
function KeyRow({ s }: { s: PomodoroState }) {
  const idle = s.status === "idle";
  const color = PHASE_COLOR[s.phase];
  const progress = idle || s.phaseMs <= 0 ? 0 : 1 - remainingNow(s) / s.phaseMs;
  const filled = progress * 12;
  const cap = (key: string, level: number, c: string, label: string, size = "h-6 min-w-0 flex-1") => (
    <span
      key={key}
      className={`flex ${size} items-center justify-center rounded-[5px] border border-line text-[8px] text-ink-3`}
      style={
        idle
          ? undefined
          : {
              background: `color-mix(in srgb, ${c} ${Math.round(level * 100)}%, transparent)`,
              borderColor: level > 0.3 ? c : undefined,
              color: level > 0.5 ? "#fff" : undefined,
            }
      }
    >
      {label}
    </span>
  );
  return (
    <div className="flex w-full max-w-[400px] flex-col items-center gap-1.5" aria-hidden data-keyrow>
      <div className="flex w-full gap-[3px]">
        {cap("esc", 1, color, "Esc", "h-6 min-w-0 flex-[1.3]")}
        <span className="w-1 shrink-0" />
        {Array.from({ length: 12 }, (_, i) => {
          const part = Math.max(0, Math.min(1, filled - i));
          return cap(`f${i + 1}`, 0.1 + 0.9 * part, color, `F${i + 1}`);
        })}
      </div>
      <div className="flex gap-[3px]">
        {Array.from({ length: s.settings.rounds }, (_, i) =>
          cap(`r${i}`, i < s.focusDone ? 0.55 : 0.1, PHASE_COLOR.focus, String(i + 1), "h-6 w-6"),
        )}
      </div>
    </div>
  );
}

function LastLine({ s }: { s: PomodoroState }) {
  const t = useT();
  const l = s.last!;
  return (
    <p className="num text-[12px] text-ink-2" data-pomo-last>
      {t("pomo.last")}: {l.focusDone} {t("pomo.round").toLocaleLowerCase()} · {l.focusMin} {t("pomo.last.focus")} ·{" "}
      {l.complete ? t("pomo.last.complete") : t("pomo.last.stopped")} ·{" "}
      {new Date(l.endedMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
    </p>
  );
}

/** Uploads one series needs against the budget ready now. */
function CostLine({ s }: { s: PomodoroState }) {
  const t = useT();
  const live = useLive((x) => x.state);
  if (!live) return null;
  // Every phase is a card: rounds focus phases, rounds − 1 short breaks, one long break.
  const { rounds, focusMin, shortMin, longMin } = s.settings;
  const need = rounds * 2;
  const ready = Math.max(0, Math.floor(live.tokens + 1e-6));
  const seriesMin = rounds * focusMin + (rounds - 1) * shortMin + longMin;
  const refill = (live.perDay / 24) * (seriesMin / 60);
  return (
    <div className="flex flex-col gap-1">
      <p className="text-[11.5px] text-ink-2">
        {t("pomo.cost.a")} {need} {t("pomo.cost.b")} {ready} {t("pomo.cost.c")}
      </p>
      {need > live.tokens + refill && (
        <p className="text-[11.5px] text-warn">{t("pomo.cost.short")}</p>
      )}
    </div>
  );
}

function SwitchRow({
  icon,
  title,
  body,
  checked,
  onChange,
  children,
}: {
  icon: ReactNode;
  title: string;
  body: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-line p-3">
      <div className="flex items-start justify-between gap-3">
        <span className="flex items-start gap-2.5">
          {icon}
          <span className="flex flex-col">
            <span className="text-[13px] font-semibold">{title}</span>
            <span className="text-[11.5px] text-ink-3">{body}</span>
          </span>
        </span>
        <Toggle checked={checked} onChange={onChange} label={title} />
      </div>
      {children}
    </div>
  );
}

function LengthField({
  label,
  unit,
  value,
  limits,
  onChange,
}: {
  label: string;
  unit?: string;
  value: number;
  limits: readonly [number, number];
  onChange: (v: number) => void;
}) {
  const [lo, hi] = limits;
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = (v: number) => {
    const n = Math.max(lo, Math.min(hi, Math.round(v)));
    setText(String(n));
    if (n !== value) onChange(n);
  };
  const btn = "flex h-8 w-8 items-center justify-center rounded-lg text-ink-2 hover:bg-surface-2 hover:text-ink disabled:opacity-30";
  return (
    <div className="flex items-center justify-between gap-2 rounded-xl border border-line py-1 pr-1 pl-3">
      <span className="truncate text-[12.5px] text-ink-2">{label}</span>
      <span className="flex shrink-0 items-center gap-0.5">
        <button type="button" className={btn} onClick={() => commit(value - 1)} disabled={value <= lo} aria-label={`${label} −`}>
          <Minus size={14} />
        </button>
        <input
          className="num h-8 w-9 rounded-md bg-transparent text-center text-[13px] font-semibold outline-none focus:bg-surface-2"
          inputMode="numeric"
          value={text}
          aria-label={label}
          onChange={(e) => setText(e.target.value.replace(/\D/g, "").slice(0, 3))}
          onBlur={() => commit(Number(text) || lo)}
          onKeyDown={(e) => {
            if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          }}
        />
        {unit && <span className="w-5 text-[11px] text-ink-3">{unit}</span>}
        <button type="button" className={btn} onClick={() => commit(value + 1)} disabled={value >= hi} aria-label={`${label} +`}>
          <Plus size={14} />
        </button>
      </span>
    </div>
  );
}
