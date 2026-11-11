// The focus timer's card on its own page (pages/Focus.tsx; phase C,
// second source). The backend runs it, lights the keys and asks for screen
// cards; this shows it and sets it. One card: what the timer shows (the
// dial and the keyboard as its keys light up) on top, and every setting of
// the timer under it, the key lights' own settings nested in their row. The
// "Klavye ekranı" row shows the cards the screen will get, drawn by the same
// code as the real ones, and what comes back when a series ends.

import {
  ArrowDownToLine,
  ArrowLeftToLine,
  ArrowRight,
  ArrowRightToLine,
  Clock3,
  FoldHorizontal,
  Lightbulb,
  Loader2,
  Minus,
  MonitorUp,
  Pause,
  Play,
  Plus,
  Radio,
  SkipForward,
  Square,
  SunDim,
  Volume2,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { KeyboardInfo } from "../../bindings/KeyboardInfo";
import type { LiveRestore } from "../../bindings/LiveRestore";
import type { PomodoroEndScreen } from "../../bindings/PomodoroEndScreen";
import type { PomodoroMotion } from "../../bindings/PomodoroMotion";
import type { PomodoroPhase } from "../../bindings/PomodoroPhase";
import type { PomodoroSettings } from "../../bindings/PomodoroSettings";
import type { PomodoroState } from "../../bindings/PomodoroState";
import type { PomodoroTransition } from "../../bindings/PomodoroTransition";
import { ColorPicker, Segmented, Slider, Toggle } from "../../components/controls";
import { useT, type MessageKey } from "../../lib/i18n";
import { fmtWait, useLive } from "../../lib/live";
import { pomodoroCard, pomodoroWords } from "../../lib/liveCards";
import { useApp } from "../../lib/store";
import { LIMITS, POMODORO_SOURCE, fmtClock, phaseColor, remainingNow, usePomodoro } from "../../lib/pomodoro";
import { TRY_MS, fromHex, keyPositions, lightColors, tryColors, viewOf } from "../../lib/pomodoroLights";
import { uploadSeconds } from "../../lib/screenImport";
import { FramePlayer, Hint, fmtSeconds, loadFrames, pickThumb } from "./common";
import { SendNow } from "./LivePanel";

const MOTIONS: { id: PomodoroMotion; label: MessageKey; icon: LucideIcon }[] = [
  { id: "rightToLeft", label: "pomo.motion.rightToLeft", icon: ArrowLeftToLine },
  { id: "leftToRight", label: "pomo.motion.leftToRight", icon: ArrowRightToLine },
  { id: "topToBottom", label: "pomo.motion.topToBottom", icon: ArrowDownToLine },
  { id: "toCenter", label: "pomo.motion.toCenter", icon: FoldHorizontal },
  { id: "fade", label: "pomo.motion.fade", icon: SunDim },
];

const COLOR_KEYS: { phase: PomodoroPhase; field: "focusColor" | "shortColor" | "longColor"; label: MessageKey }[] = [
  { phase: "focus", field: "focusColor", label: "pomo.color.focus" },
  { phase: "shortBreak", field: "shortColor", label: "pomo.color.shortBreak" },
  { phase: "longBreak", field: "longColor", label: "pomo.color.longBreak" },
];

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

export function PomodoroPanel({ kb, usb }: { kb: KeyboardInfo; usb: boolean }) {
  const t = useT();
  const s = usePomodoro((x) => x.state);
  const drawError = usePomodoro((x) => x.drawError);
  const control = usePomodoro((x) => x.control);
  const configure = usePomodoro((x) => x.configure);
  useTicking(s?.status === "running");
  // The idle preview loops the run "Klavyede dene" plays; trying restarts
  // it so the drawing and the keys move together.
  const loopStart = useRef(Date.now());
  // Colour drags fire many changes: show them at once, send the latest
  // shortly after.
  const [draft, setDraft] = useState<Partial<PomodoroSettings>>({});
  const configureRef = useRef(configure);
  configureRef.current = configure;
  useEffect(() => {
    if (!Object.keys(draft).length) return;
    const id = window.setTimeout(() => {
      configureRef.current(draft).catch(console.error);
      setDraft({});
    }, 150);
    return () => window.clearTimeout(id);
  }, [draft]);
  if (!s) return null;
  const set = (patch: Partial<PomodoroSettings>) => configure(patch).catch(console.error);
  const act = (a: Parameters<typeof control>[0]) => control(a).catch(console.error);
  const idle = s.status === "idle";
  const running = s.status === "running";
  const shown = { ...s.settings, ...draft };
  const caption = !shown.lights ? t("pomo.preview.off") : idle ? t("pomo.preview.idle") : t("pomo.preview.live");

  return (
    <section className="card flex flex-col gap-6 p-5" data-pomodoro>
      {/* What the timer shows: the time left, and the keys. */}
      <div className="flex flex-col gap-5 rounded-lg border border-line bg-bg-raise/50 p-5">
        <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-6">
          <Dial s={s} />
          <div className="flex min-w-0 flex-col gap-2">
            <span className="text-[12px] text-ink-3">{caption}</span>
            {kb.geometry && <LightPreview kb={kb} s={s} settings={shown} loopStart={loopStart} />}
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => act(running ? "pause" : "start")}
              className="flex h-10 min-w-[132px] items-center justify-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink"
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
                  className="flex h-10 items-center gap-2 rounded-lg border border-line px-3.5 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
                  data-pomo-skip
                >
                  <SkipForward size={15} /> {t("pomo.skip")}
                </button>
                <button
                  type="button"
                  onClick={() => act("stop")}
                  title={t("pomo.stop.hint")}
                  className="flex h-10 items-center gap-2 rounded-lg border border-line px-3.5 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
                  data-pomo-stop
                >
                  <Square size={13} /> {t("pomo.stop")}
                </button>
              </>
            )}
          </div>
          {idle && shown.lights && usb && (
            <button
              type="button"
              title={t("pomo.try.body")}
              onClick={() => {
                if (s.trying) return;
                loopStart.current = Date.now();
                act("try");
              }}
              className="flex h-9 items-center gap-2 rounded-lg border border-signal/50 px-3 text-[13px] font-semibold text-signal hover:bg-signal/10"
              data-pomo-try
            >
              {s.trying ? <Loader2 size={14} className="animate-spin" /> : <Play size={14} />}
              {s.trying ? t("pomo.trying") : t("pomo.try")}
            </button>
          )}
        </div>
        {idle && s.last && <LastLine s={s} />}
        <CardWaiting usb={usb} />
      </div>

      {/* Settings of the timer; the key lights' own settings sit in their row. */}
      <div className="flex flex-col gap-2">
        <h3 className="text-[13px] font-semibold">{t("pomo.settings")}</h3>
        <div className="flex flex-col divide-y divide-line rounded-lg border border-line" data-pomo-settings>
          <SettingRow icon={Clock3} title={t("pomo.lengths")} body={t("pomo.lengths.note")}>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(196px,1fr))] gap-2">
              <LengthField label={t("pomo.len.focus")} unit={t("pomo.min")} value={s.settings.focusMin} limits={LIMITS.focusMin} onChange={(focusMin) => set({ focusMin })} />
              <LengthField label={t("pomo.len.short")} unit={t("pomo.min")} value={s.settings.shortMin} limits={LIMITS.shortMin} onChange={(shortMin) => set({ shortMin })} />
              <LengthField label={t("pomo.len.long")} unit={t("pomo.min")} value={s.settings.longMin} limits={LIMITS.longMin} onChange={(longMin) => set({ longMin })} />
              <LengthField label={t("pomo.len.rounds")} value={s.settings.rounds} limits={LIMITS.rounds} onChange={(rounds) => set({ rounds })} />
            </div>
          </SettingRow>
          <SettingRow
            icon={Lightbulb}
            title={t("pomo.lights")}
            body={t("pomo.lights.body")}
            checked={shown.lights}
            onChange={(lights) => set({ lights })}
            data="lights"
          >
            {shown.lights && <LightSettings shown={shown} set={set} setDraft={setDraft} />}
            {!usb && <p className="text-[12px] text-ink-3">{t("pomo.preview.noUsb")}</p>}
          </SettingRow>
          <SettingRow
            icon={MonitorUp}
            title={t("pomo.screen")}
            body={t("pomo.screen.body")}
            checked={s.settings.screen}
            onChange={(screen) => set({ screen })}
            data="screen"
          >
            {s.settings.screen && <ScreenSettings s={s} set={set} />}
          </SettingRow>
          <SettingRow
            icon={Volume2}
            title={t("pomo.sound")}
            body={t("pomo.sound.body")}
            checked={s.settings.sound}
            onChange={(sound) => set({ sound })}
          />
        </div>
      </div>

      {drawError && <Hint tone="error">{t("err.draw")}</Hint>}
    </section>
  );
}

/** Ring with the time left in the phase. */
function Dial({ s }: { s: PomodoroState }) {
  const t = useT();
  const idle = s.status === "idle";
  const left = idle ? s.settings.focusMin * 60_000 : remainingNow(s);
  // The ring is the time left, like the keys: full at the start.
  const share = idle || s.phaseMs <= 0 ? 1 : left / s.phaseMs;
  const color = phaseColor(s.settings, s.phase);
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
          strokeDasharray={`${len * share} ${len}`}
          style={{ filter: `drop-shadow(0 0 6px ${color}88)`, transition: "stroke-dasharray 250ms linear" }}
          opacity={idle ? 0.3 : share > 0 ? 1 : 0}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-0.5">
        <span className="text-[13px] font-semibold" style={{ color: idle ? "var(--ink-3)" : color }}>
          {idle ? t("pomo.idle") : t(PHASE_KEY[s.phase])}
        </span>
        <span className="num font-display text-[44px] leading-none font-semibold" data-time>
          {fmtClock(left)}
        </span>
        <span className="text-[12px] text-ink-3">
          {s.status === "paused"
            ? t("pomo.paused")
            : `${t("pomo.round")} ${idle ? 1 : s.round}/${s.settings.rounds}`}
        </span>
      </div>
    </div>
  );
}

/** The key lights' own settings, nested under their row. */
function LightSettings({
  shown,
  set,
  setDraft,
}: {
  shown: PomodoroSettings;
  set: (patch: Partial<PomodoroSettings>) => void;
  setDraft: (f: (d: Partial<PomodoroSettings>) => Partial<PomodoroSettings>) => void;
}) {
  const t = useT();
  const [editing, setEditing] = useState<PomodoroPhase | null>(null);
  const field = COLOR_KEYS.find((c) => c.phase === editing);
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-6" data-pomo-lights>
      <div className="flex min-w-0 flex-col gap-2">
        <span className="text-[13px] font-medium">{t("pomo.motion")}</span>
        <div className="flex flex-col gap-1.5" role="radiogroup" aria-label={t("pomo.motion")}>
          {MOTIONS.map((m) => {
            const on = shown.motion === m.id;
            const Icon = m.icon;
            return (
              <button
                key={m.id}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => set({ motion: m.id })}
                className={`flex items-center gap-2 rounded-lg border px-2.5 py-2 text-left text-[13px] transition-colors ${
                  on ? "border-signal bg-signal/10 text-ink" : "border-line text-ink-2 hover:bg-surface-2 hover:text-ink"
                }`}
              >
                <Icon size={15} className={on ? "text-signal" : "text-ink-3"} />
                {t(m.label)}
              </button>
            );
          })}
        </div>
      </div>

      <div className="flex min-w-0 flex-col gap-4">
        <div className="flex flex-col gap-2">
          <span className="text-[13px] font-medium">{t("pomo.colors")}</span>
          <div className="flex flex-wrap gap-1.5">
            {COLOR_KEYS.map((c) => {
              const on = editing === c.phase;
              return (
                <button
                  key={c.phase}
                  type="button"
                  aria-expanded={on}
                  onClick={() => setEditing(on ? null : c.phase)}
                  className={`flex items-center gap-2 rounded-lg border px-2.5 py-1.5 text-[13px] ${
                    on ? "border-signal bg-signal/10 text-ink" : "border-line text-ink-2 hover:bg-surface-2"
                  }`}
                  data-pomo-color={c.phase}
                >
                  <span className="h-4 w-4 rounded-[5px] border border-black/20" style={{ background: phaseColor(shown, c.phase) }} />
                  {t(c.label)}
                </button>
              );
            })}
          </div>
          {field && (
            <ColorPicker
              label={t(field.label)}
              value={phaseColor(shown, field.phase)}
              onChange={(hex) => setDraft((d) => ({ ...d, [field.field]: fromHex(hex) }))}
            />
          )}
        </div>

        <Slider
          label={t("pomo.brightness")}
          value={shown.brightness}
          min={1}
          max={5}
          step={1}
          format={(v) => `${v}/5`}
          onChange={(brightness) => set({ brightness })}
        />

        <div className="flex flex-col gap-2">
          <span className="text-[13px] font-medium">{t("pomo.transition")}</span>
          <Segmented<PomodoroTransition>
            label={t("pomo.transition")}
            value={shown.transition}
            onChange={(transition) => set({ transition })}
            options={[
              { value: "blink", label: t("pomo.transition.blink") },
              { value: "wave", label: t("pomo.transition.wave") },
            ]}
          />
        </div>

        <div className="flex items-start justify-between gap-3">
          <span className="flex flex-col">
            <span className="text-[13px] font-medium">{t("pomo.pulse")}</span>
            <span className="text-[12px] text-ink-3">{t("pomo.pulse.body")}</span>
          </span>
          <Toggle checked={shown.pulse} onChange={(pulse) => set({ pulse })} label={t("pomo.pulse")} />
        </div>
      </div>
    </div>
  );
}

/** The keyboard as the timer lights it: the running phase, or (idle) the
 *  short run "Klavyede dene" plays, on a loop. */
function LightPreview({
  kb,
  s,
  settings,
  loopStart,
}: {
  kb: KeyboardInfo;
  s: PomodoroState;
  settings: PomodoroSettings;
  loopStart: { current: number };
}) {
  const t = useT();
  const keys = useMemo(() => (kb.geometry ? keyPositions(kb.geometry) : []), [kb.geometry]);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 50);
    return () => window.clearInterval(id);
  }, []);
  const view = viewOf(s, now);
  let colors: Record<number, string>;
  if (!settings.lights) {
    colors = Object.fromEntries(keys.map((k) => [k.led, "#24262b"]));
  } else if (view) {
    colors = lightColors(keys, settings, view, now);
  } else {
    // The run, then a short dark pause before it starts again.
    const t = (now - loopStart.current) % (TRY_MS + 1_200);
    colors = t < TRY_MS ? tryColors(keys, settings, t, now) : Object.fromEntries(keys.map((k) => [k.led, "rgb(0,0,0)"]));
  }
  const g = kb.geometry!;
  const pad = 0.25;
  return (
    <svg
      viewBox={`${-pad} ${-pad} ${g.width + 2 * pad} ${g.height + 2 * pad}`}
      className="w-full max-w-[720px] rounded-lg"
      role="img"
      aria-label={t("pomo.preview.idle")}
      data-pomo-preview
    >
      <rect x={-pad} y={-pad} width={g.width + 2 * pad} height={g.height + 2 * pad} rx={0.35} fill="#111214" />
      {g.screen && (
        <rect x={g.screen.x + 0.06} y={g.screen.y + 0.06} width={g.screen.w - 0.12} height={g.screen.h - 0.12} rx={0.08} fill="#26282c" />
      )}
      {g.keys.map((k) => (
        <rect
          key={k.id}
          x={k.x + 0.06}
          y={k.y + 0.06}
          width={k.w - 0.12}
          height={k.h - 0.12}
          rx={0.12}
          fill={colors[k.led] ?? "#000"}
          stroke="rgba(255,255,255,0.06)"
          strokeWidth={0.03}
        />
      ))}
    </svg>
  );
}

function LastLine({ s }: { s: PomodoroState }) {
  const t = useT();
  const l = s.last!;
  return (
    <p className="num text-[12px] text-ink-2" data-pomo-last>
      {t("pomo.last")}: {t("pomo.last.rounds", { count: l.focusDone })}, {t("pomo.last.focus", { min: l.focusMin })},{" "}
      {l.complete ? t("pomo.last.complete") : t("pomo.last.stopped")} (
      {new Date(l.endedMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })})
    </p>
  );
}

/** This phase's card could not go yet: why, when, and a way to send it now
 *  (owner's test, 2026-11-10: with the budget empty the screen looked stuck). */
function CardWaiting({ usb }: { usb: boolean }) {
  const t = useT();
  const live = useLive((x) => x.state);
  if (!live?.enabled || live.pending?.source !== POMODORO_SOURCE) return null;
  const units = { h: t("sc.live.h"), m: t("sc.live.m") };
  const budget = usb && live.nextTokenMs != null;
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-lg border border-warn/40 bg-warn/10 px-3.5 py-2.5" data-pomo-waiting>
      <p className="flex min-w-0 flex-1 items-start gap-2 text-[13px] text-ink">
        <Clock3 size={14} className="mt-0.5 shrink-0 text-warn" />
        <span>
          {!usb
            ? t("pomo.wait.cable")
            : budget
              ? t("pomo.wait.budget", { wait: fmtWait(live.nextTokenMs!, units) })
              : t("pomo.wait.turn")}
        </span>
      </p>
      {budget && <SendNow />}
    </div>
  );
}

/** What the keyboard screen gets: the cards in turn, the live mode they need,
 *  and what comes back at the end. */
function ScreenSettings({ s, set }: { s: PomodoroState; set: (patch: Partial<PomodoroSettings>) => void }) {
  const t = useT();
  const live = useLive((x) => x.state);
  const configureLive = useLive((x) => x.configure);
  const openScreen = useApp((x) => x.openScreen);
  if (!live) return null;
  return (
    <>
      <CardStrip settings={s.settings} own={live.own} />
      {live.enabled ? (
        <CostLine s={s} />
      ) : (
        <div className="flex flex-col gap-2 rounded-lg border border-warn/40 bg-warn/10 px-3.5 py-3" data-pomo-liveoff>
          <p className="flex items-start gap-2 text-[13px] text-ink">
            <Radio size={14} className="mt-0.5 shrink-0 text-warn" />
            {t("pomo.screen.liveOff")}
          </p>
          {live.nowPlaying && <p className="text-[12px] text-ink-3">{t("pomo.screen.npToo")}</p>}
          <div className="flex flex-wrap items-center gap-3">
            <button
              type="button"
              onClick={() => configureLive({ enabled: true }).catch(console.error)}
              className="flex h-9 items-center gap-2 rounded-lg bg-signal px-3 text-[13px] font-semibold text-signal-ink"
              data-pomo-liveon
            >
              <Radio size={14} /> {t("pomo.screen.liveOn")}
            </button>
            <button type="button" onClick={() => openScreen("live")} className="text-[12px] font-medium text-signal hover:underline">
              {t("pomo.screen.openLive")}
            </button>
          </div>
        </div>
      )}
      <EndChoice value={s.settings.endScreen} own={live.own} onChange={(endScreen) => set({ endScreen })} />
    </>
  );
}

/** The cards one series puts on the screen, drawn by the real card code
 *  with the user's lengths and colours, and what comes at the end. */
function CardStrip({ settings, own }: { settings: PomodoroSettings; own: LiveRestore | null }) {
  const t = useT();
  const locale = useApp((x) => x.locale);
  const [cards, setCards] = useState<Uint16Array[] | null>(null);
  const { focusMin, shortMin, longMin, rounds, focusColor, shortColor, longColor, endScreen } = settings;
  useEffect(() => {
    let live = true;
    const w = pomodoroWords(t);
    const s = settings;
    const breakPhase = rounds > 1 ? "shortBreak" : "longBreak";
    const base = { kind: "phase" as const, round: 1, rounds, focusMin: 0, complete: false, endedMs: Date.now() };
    const list = [
      pomodoroCard({ ...base, phase: "focus", minutes: focusMin, focusDone: 0 }, s, w),
      pomodoroCard({ ...base, phase: breakPhase, minutes: rounds > 1 ? shortMin : longMin, focusDone: 1 }, s, w),
    ];
    if (endScreen === "summary") {
      const done = { kind: "done" as const, phase: "focus" as const, round: rounds, rounds, minutes: 0 };
      list.push(pomodoroCard({ ...done, focusDone: rounds, focusMin: rounds * focusMin, complete: true, endedMs: Date.now() }, s, w));
    }
    Promise.all(list).then((p) => live && setCards(p.map((c) => c.frames[0]!)), console.error);
    return () => {
      live = false;
    };
    // The colours are arrays: compare them by value.
  }, [locale, focusMin, shortMin, longMin, rounds, endScreen, focusColor.join(), shortColor.join(), longColor.join()]);
  const [ownFrame, setOwnFrame] = useState<Uint16Array | null>(null);
  useEffect(() => {
    setOwnFrame(null);
    if (!own || endScreen !== "back") return;
    let live = true;
    loadFrames(own.id, own.frames === 1).then((d) => live && setOwnFrame(pickThumb(d.frames) ?? null), console.error);
    return () => {
      live = false;
    };
  }, [own?.id, own?.frames, endScreen]);

  const box = "h-[76px] w-[135px] shrink-0 rounded-md bg-black";
  const labels = [t("pomo.phase.focus"), t(rounds > 1 ? "pomo.phase.shortBreak" : "pomo.phase.longBreak")];
  const tile = (frame: Uint16Array | null | undefined, label: string, key: string) => (
    <figure key={key} className="flex flex-col gap-1">
      {frame ? <FramePlayer frames={[frame]} delaysMs={[100]} className={box} /> : <div className={box} />}
      <figcaption className="truncate text-[11px] text-ink-3">{label}</figcaption>
    </figure>
  );
  const arrow = (key: string) => <ArrowRight key={key} size={14} className="mb-5 shrink-0 text-ink-3" />;
  const end =
    endScreen === "summary"
      ? cards?.[2]
      : ownFrame;
  const endLabel = `${t("pomo.screen.atEnd")}: ${endScreen === "summary" ? t("pomo.end.summary.short") : own?.name ?? t("pomo.end.own")}`;
  return (
    <div className="flex flex-col gap-1.5" data-pomo-strip>
      <span className="text-[12px] text-ink-3">{t("pomo.screen.example")}</span>
      <div className="flex flex-wrap items-center gap-2">
        {tile(cards?.[0], labels[0]!, "a")}
        {arrow("x")}
        {tile(cards?.[1], labels[1]!, "b")}
        {arrow("y")}
        {endScreen === "back" && !own ? (
          <figure className="flex flex-col gap-1">
            <div className={`${box} flex items-center justify-center border border-dashed border-line bg-transparent text-[11px] text-ink-3`}>
              {t("pomo.end.own")}
            </div>
            <figcaption className="truncate text-[11px] text-ink-3">{t("pomo.screen.atEnd")}</figcaption>
          </figure>
        ) : (
          tile(end, endLabel, "c")
        )}
      </div>
    </div>
  );
}

const END_CHOICES: { id: PomodoroEndScreen; label: MessageKey; body: MessageKey }[] = [
  { id: "back", label: "pomo.end.back", body: "pomo.end.back.body" },
  { id: "summary", label: "pomo.end.summary", body: "pomo.end.summary.body" },
];

/** What the screen shows when a series ends or is stopped. */
function EndChoice({
  value,
  own,
  onChange,
}: {
  value: PomodoroEndScreen;
  own: LiveRestore | null;
  onChange: (v: PomodoroEndScreen) => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-1.5" role="radiogroup" aria-label={t("pomo.end")} data-pomo-end>
      <span className="text-[13px] font-medium">{t("pomo.end")}</span>
      {END_CHOICES.map((c) => {
        const on = c.id === value;
        return (
          <button
            key={c.id}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => onChange(c.id)}
            className={`flex items-start gap-3 rounded-lg border px-3.5 py-2.5 text-left transition-colors ${
              on ? "border-signal/60 bg-signal/10" : "border-line hover:bg-surface-2"
            }`}
          >
            <span
              className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border-2 ${on ? "border-signal" : "border-ink-3"}`}
            >
              {on && <span className="h-1.5 w-1.5 rounded-full bg-signal" />}
            </span>
            <span className="flex min-w-0 flex-col gap-0.5">
              <span className="text-[13px] font-semibold">{t(c.label)}</span>
              <span className="text-[12px] text-ink-3">{t(c.body)}</span>
              {c.id === "back" && own && (
                <span className="text-[12px] text-ink-2">
                  {t("pomo.end.own")}: <span className="font-medium text-ink">{own.name}</span> ({t("sc.uploadTime")}{" "}
                  <span className="num">~{fmtSeconds(uploadSeconds(own.frames))}</span>)
                </span>
              )}
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** Uploads one series needs against the budget ready now. */
function CostLine({ s }: { s: PomodoroState }) {
  const t = useT();
  const live = useLive((x) => x.state);
  if (!live) return null;
  // Every phase is a card: rounds focus phases, rounds − 1 short breaks, one
  // long break; and one more at the end (the own picture or a summary).
  const { rounds, focusMin, shortMin, longMin } = s.settings;
  const need = rounds * 2 + 1;
  const ready = Math.max(0, Math.floor(live.tokens + 1e-6));
  const seriesMin = rounds * focusMin + (rounds - 1) * shortMin + longMin;
  const refill = (live.perDay / 24) * (seriesMin / 60);
  return (
    <div className="flex flex-col gap-1">
      <p className="text-[12px] text-ink-2">
        {t("pomo.cost", { need, ready })}
      </p>
      {need > live.tokens + refill && (
        <p className="text-[12px] text-warn">{t("pomo.cost.short")}</p>
      )}
    </div>
  );
}

/** One setting of the timer: a title and what it does, its switch, and
 *  what belongs to it, indented under it. */
function SettingRow({
  icon: Icon,
  title,
  body,
  checked,
  onChange,
  data,
  children,
}: {
  icon: LucideIcon;
  title: string;
  body: string;
  checked?: boolean;
  onChange?: (v: boolean) => void;
  data?: string;
  children?: ReactNode;
}) {
  const nested = Array.isArray(children) ? children.some(Boolean) : !!children;
  return (
    <div className="flex flex-col gap-3 p-4" data-pomo-row={data}>
      <div className="flex items-start justify-between gap-3">
        <span className="flex items-start gap-2.5">
          <Icon size={16} className="mt-0.5 shrink-0 text-signal" />
          <span className="flex max-w-[80ch] flex-col">
            <span className="text-[13px] font-semibold">{title}</span>
            <span className="text-[12px] text-ink-3">{body}</span>
          </span>
        </span>
        {onChange && checked !== undefined && <Toggle checked={checked} onChange={onChange} label={title} />}
      </div>
      {nested && <div className="ml-[7px] flex flex-col gap-3 border-l-2 border-line pl-5">{children}</div>}
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
  const t = useT();
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
    <div className="flex items-center justify-between gap-2 rounded-lg border border-line py-1 pr-1 pl-3">
      <span className="truncate text-[13px] text-ink-2">{label}</span>
      <span className="flex shrink-0 items-center gap-0.5">
        <button type="button" className={btn} onClick={() => commit(value - 1)} disabled={value <= lo} aria-label={`${label}, ${t("common.decrease")}`}>
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
        <button type="button" className={btn} onClick={() => commit(value + 1)} disabled={value >= hi} aria-label={`${label}, ${t("common.increase")}`}>
          <Plus size={14} />
        </button>
      </span>
    </div>
  );
}
