// "Canlı ekran": the live screen switch, what shows on the screen in which
// order (calendar, reminders, focus timer, the song, the user's own picture), and the upload
// budget (phase C). The backend runs the live mode; this only shows and
// sets it.

import { AlertTriangle, BellRing, CalendarDays, Clock3, Hourglass, ImageIcon, Loader2, Music2, Radio, RotateCcw, Upload, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState, type ReactNode } from "react";
import type { LivePlan } from "../../bindings/LivePlan";
import type { LiveRestore } from "../../bindings/LiveRestore";
import type { LiveState } from "../../bindings/LiveState";
import type { SongStop } from "../../bindings/SongStop";
import { Segmented, Toggle } from "../../components/controls";
import { CALENDAR_SOURCE, hhmm, useCalendar } from "../../lib/calendar";
import { useT, type MessageKey } from "../../lib/i18n";
import { NOW_PLAYING_SOURCE, fmtWait, useLive } from "../../lib/live";
import { POMODORO_SOURCE, usePomodoro } from "../../lib/pomodoro";
import { REMINDER_SOURCE, useReminders } from "../../lib/reminders";
import { useApp } from "../../lib/store";
import { uploadSeconds } from "../../lib/screenImport";
import { calendarStatus } from "./CalendarPanel";
import { FramePlayer, Hint, fmtSeconds, loadFrames, pickThumb, useItemName, useNow } from "./common";
import { friendlyError } from "../../lib/errors";

const PLANS: { id: LivePlan; label: MessageKey; years: MessageKey }[] = [
  { id: "saver", label: "sc.live.plan.saver", years: "sc.live.years.saver" },
  { id: "balanced", label: "sc.live.plan.balanced", years: "sc.live.years.balanced" },
  { id: "heavy", label: "sc.live.plan.heavy", years: "sc.live.years.heavy" },
];

export function LivePanel({ usb }: { usb: boolean }) {
  const t = useT();
  const name = useItemName();
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
          <h2 className="flex items-center gap-2 text-[18px] font-semibold">
            <Radio size={16} className={s.enabled ? "text-ok" : "text-ink-3"} aria-hidden />
            {t("sc.live.title")}
          </h2>
          <p className="mt-1 text-[13px] text-ink-2">{t("sc.live.body")}</p>
        </div>
        <label className="flex items-center gap-3 rounded-lg border border-line px-3 py-2">
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

      <ScreenOrder s={s} set={set} />

      <div className="flex flex-col gap-3 border-t border-line pt-4" data-live-budget>
        <span className="text-[13px] font-medium">{t("sc.live.plan")}</span>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(300px,1fr))] items-start gap-x-6 gap-y-3">
          <Segmented<LivePlan>
            label={t("sc.live.plan")}
            value={s.plan}
            onChange={(p) => set({ plan: p })}
            options={PLANS.map((p) => ({ value: p.id, label: t(p.label) }))}
          />
          <div className="flex flex-col gap-1.5">
            <BudgetLine s={s} />
            <p className="text-[12px] text-ink-3">{t(plan.years)}</p>
          </div>
        </div>
        {s.plan === "heavy" && <Hint tone="warn">{t("sc.live.heavyWarn")}</Hint>}
      </div>

      <StatusLine s={s} usb={usb} />
      {!s.enabled && s.restore && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-line px-4 py-3" data-live-leftover>
          <span className="text-[13px] text-ink-2">
            {t("sc.live.leftover")} <span className="font-medium text-ink">{name(s.restore)}</span>
          </span>
          <button
            type="button"
            onClick={() => setClosing(true)}
            className="flex h-9 items-center gap-2 rounded-lg border border-signal/50 px-3 text-[13px] font-semibold text-signal hover:bg-signal/10"
          >
            <RotateCcw size={14} /> {t("sc.live.offRestore")}
          </button>
        </div>
      )}
      {drawError && <Hint tone="error">{t("err.draw")}</Hint>}
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

type Patch = Parameters<ReturnType<typeof useLive.getState>["configure"]>[0];

type Layered = "calendar" | "reminder" | "pomodoro" | "song" | "own";

/** Which layer is on the GIF page now. */
function onScreen(s: LiveState): Layered {
  if (s.uploading?.source === "restore" || !s.restore) return "own";
  if (s.last?.source === CALENDAR_SOURCE) return "calendar";
  if (s.last?.source === REMINDER_SOURCE) return "reminder";
  if (s.last?.source === POMODORO_SOURCE) return "pomodoro";
  if (s.last?.source === NOW_PLAYING_SOURCE) return "song";
  return "own";
}

/** "12 sn", "4 dk". */
function fmtSoon(ms: number, units: { s: string; h: string; m: string }) {
  return ms < 60_000 ? `${Math.max(1, Math.ceil(ms / 1000))} ${units.s}` : fmtWait(ms, units);
}

/** The order the sources use the screen in, what each does now, and the
 *  user's own picture at the bottom (owner's design, 2026-11-10). */
function ScreenOrder({ s, set }: { s: LiveState; set: (patch: Patch) => void }) {
  const t = useT();
  const name = useItemName();
  const p = usePomodoro((x) => x.state);
  const cal = useCalendar((x) => x.state);
  const configureCal = useCalendar((x) => x.configure);
  const rem = useReminders((x) => x.state);
  const configureRem = useReminders((x) => x.configure);
  const setPage = useApp((x) => x.setPage);
  const on = onScreen(s);
  const calKey = calendarStatus(s, cal, on === "calendar");
  const calStatus = calKey === "calendar.st.next" && cal?.nextCardMs != null ? t(calKey, { time: hhmm(cal.nextCardMs) }) : t(calKey);
  const units = { s: t("sc.live.s"), h: t("sc.live.h"), m: t("sc.live.m") };
  const playing = !!s.track?.playing;

  const pomo: MessageKey = !s.enabled
    ? "sc.live.st.liveOff"
    : p && !p.settings.screen
      ? "sc.live.st.cardsOff"
      : s.pending?.source === POMODORO_SOURCE && s.nextTokenMs != null
        ? "sc.live.st.waitBudget"
        : on === "pomodoro"
        ? "sc.live.st.showing"
        : s.holders.includes(CALENDAR_SOURCE) && p?.status !== "idle"
          ? "sc.live.st.waiting"
          : p?.status === "running"
          ? "sc.live.st.running"
          : p?.status === "paused"
            ? "sc.live.st.paused"
            : "sc.live.st.idle";
  const song: MessageKey = !s.enabled
    ? "sc.live.st.liveOff"
    : !s.nowPlaying
      ? "sc.live.st.sourceOff"
      : s.pending?.source === NOW_PLAYING_SOURCE && s.nextTokenMs != null
        ? "sc.live.st.waitBudget"
        : on === "song"
          ? "sc.live.st.showing"
          : s.held && playing
            ? "sc.live.st.waiting"
            : playing
              ? "sc.live.st.playing"
              : "sc.live.st.silent";
  const goingBack = s.enabled && s.goingBack ? Math.max(0, s.goingBack.atMs - Date.now()) : null;
  const nextCard = rem?.reminders
    .filter((i) => i.reminder.card && i.nextMs != null)
    .sort((a, b) => a.nextMs! - b.nextMs!)[0];
  const remStatus = !s.enabled
    ? t("sc.live.st.liveOff")
    : !rem?.settings.enabled || !rem.settings.screen
      ? t("sc.live.st.cardsOff")
      : on === "reminder"
        ? t("sc.live.st.showing")
        : nextCard
          ? t("reminder.st.next", { time: hhmm(nextCard.nextMs!) })
          : t("reminder.st.none");

  return (
    <div className="flex flex-col gap-3" data-live-order>
      <div className="flex flex-col gap-0.5">
        <h3 className="text-[14px] font-semibold">{t("sc.live.order")}</h3>
        <p className="text-[12px] text-ink-2">{t("sc.live.order.body")}</p>
        {!s.enabled && <p className="text-[12px] text-ink-3">{t("sc.live.order.off")}</p>}
      </div>
      <ol className="flex flex-col divide-y divide-line rounded-lg border border-line">
        <Layer
          n={1}
          icon={<CalendarDays size={16} />}
          title={t("calendar.title")}
          body={t("calendar.body")}
          status={calStatus}
          active={on === "calendar"}
          data="calendar"
          right={
            <Toggle
              checked={cal?.settings.screen ?? true}
              onChange={(screen) => configureCal({ screen }).catch(console.error)}
              label={t("calendar.title")}
            />
          }
        >
          <div>
            <button
              type="button"
              onClick={() => setPage("calendar")}
              className="h-8 rounded-lg border border-line px-3 text-[12px] text-ink-2 hover:bg-surface-2 hover:text-ink"
              data-calendar-open
            >
              {t("calendar.layer.open")}
            </button>
          </div>
        </Layer>
        <Layer
          n={2}
          icon={<BellRing size={16} />}
          title={t("reminder.layer")}
          body={t("reminder.layer.body")}
          status={remStatus}
          active={on === "reminder"}
          data="reminder"
          right={
            <Toggle
              checked={rem?.settings.screen ?? true}
              onChange={(screen) => configureRem({ screen }).catch(console.error)}
              label={t("reminder.layer")}
            />
          }
        >
          <div>
            <button
              type="button"
              onClick={() => setPage("reminders")}
              className="h-8 rounded-lg border border-line px-3 text-[12px] text-ink-2 hover:bg-surface-2 hover:text-ink"
              data-reminder-open
            >
              {t("reminder.layer.open")}
            </button>
          </div>
        </Layer>
        <Layer n={3} icon={<Hourglass size={16} />} title={t("sc.live.pomo")} body={t("sc.live.pomo.body")} status={t(pomo)} active={on === "pomodoro"} data="pomodoro" />
        <Layer
          n={4}
          icon={<Music2 size={16} />}
          title={t("sc.live.np")}
          body={t("sc.live.np.body")}
          status={t(song)}
          active={on === "song"}
          data="now-playing"
          right={<Toggle checked={s.nowPlaying} onChange={(nowPlaying) => set({ nowPlaying })} label={t("sc.live.np")} />}
        >
          {s.nowPlaying && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
              <span className="text-[12px] text-ink-2">{t("sc.live.np.stop")}</span>
              <Segmented<SongStop>
                label={t("sc.live.np.stop")}
                value={s.songStop}
                onChange={(songStop) => set({ songStop })}
                options={[
                  { value: "back", label: t("sc.live.np.stop.back") },
                  { value: "keep", label: t("sc.live.np.stop.keep") },
                ]}
              />
            </div>
          )}
          {s.enabled && s.nowPlaying && <TrackLine s={s} />}
        </Layer>
        <Layer
          n={5}
          icon={<ImageIcon size={16} />}
          title={t("sc.live.own")}
          body={s.own ? t("sc.live.own.body") : t("sc.live.own.unknown")}
          status={on === "own" ? t("sc.live.st.showing") : t("sc.live.st.turn")}
          active={on === "own"}
          data="own"
        >
          {s.own && (
            <div className="flex flex-wrap items-center gap-4">
              <OwnThumb own={s.own} />
              <div className="flex min-w-0 flex-col gap-0.5">
                <span className="truncate text-[13px] font-semibold">{name(s.own)}</span>
                <span className="num text-[12px] text-ink-3">
                  {t("sc.frames", { count: s.own.frames })} · {t("sc.uploadTime")} ~{fmtSeconds(uploadSeconds(s.own.frames))}
                </span>
                {goingBack != null && (
                  <span className="flex items-center gap-1.5 text-[12px] text-signal" data-going-back>
                    <Clock3 size={13} />
                    {t("sc.live.goingBack", { wait: fmtSoon(goingBack, units) })}
                  </span>
                )}
              </div>
              {s.enabled && s.restore && !s.uploading && (
                <button
                  type="button"
                  onClick={() => set({ restore: true })}
                  className="ml-auto flex h-9 items-center gap-2 rounded-lg border border-signal/50 px-3 text-[13px] font-semibold text-signal hover:bg-signal/10"
                  data-own-back
                >
                  <RotateCcw size={14} /> {t("sc.live.own.back")}
                </button>
              )}
            </div>
          )}
        </Layer>
      </ol>
    </div>
  );
}

function Layer({
  n,
  icon,
  title,
  body,
  status,
  active,
  right,
  data,
  children,
}: {
  n: number;
  icon: ReactNode;
  title: string;
  body: string;
  status: string;
  active: boolean;
  right?: ReactNode;
  data: string;
  children?: ReactNode;
}) {
  const nested = Array.isArray(children) ? children.some(Boolean) : !!children;
  return (
    <li
      className={`flex flex-col gap-3 p-4 first:rounded-t-2xl last:rounded-b-2xl ${active ? "bg-ok/[0.07]" : ""}`}
      data-layer={data}
      data-active={active || undefined}
    >
      <div className="flex items-start justify-between gap-3">
        <span className="flex min-w-0 items-start gap-3">
          <span
            className={`num flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[12px] font-semibold ${
              active ? "bg-ok text-bg" : "bg-surface-2 text-ink-2"
            }`}
          >
            {n}
          </span>
          <span className={`mt-0.5 shrink-0 ${active ? "text-ok" : "text-signal"}`}>{icon}</span>
          <span className="flex min-w-0 flex-col">
            <span className="text-[13px] font-semibold">{title}</span>
            <span className="text-[12px] text-ink-3">{body}</span>
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-3">
          <span
            className={`rounded-md px-2 py-0.5 text-[11px] font-medium whitespace-nowrap ${
              active ? "bg-ok/15 text-ok" : "bg-surface-2 text-ink-2"
            }`}
          >
            {status}
          </span>
          {right}
        </span>
      </div>
      {nested && <div className="ml-[60px] flex flex-col gap-3">{children}</div>}
    </li>
  );
}

function OwnThumb({ own }: { own: LiveRestore }) {
  const [frame, setFrame] = useState<Uint16Array | null>(null);
  useEffect(() => {
    let live = true;
    loadFrames(own.id, own.frames === 1).then((d) => live && setFrame(pickThumb(d.frames) ?? null), console.error);
    return () => {
      live = false;
    };
  }, [own.id, own.frames]);
  const box = "h-[68px] w-[120px] shrink-0 rounded-md bg-black";
  return frame ? <FramePlayer frames={[frame]} delaysMs={[100]} className={box} /> : <div className={box} />;
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
  const name = useItemName();
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
          <h2 id="live-off-title" className="font-display text-[22px] font-semibold">
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
        <p className="text-[13px] text-ink-2">{t("sc.live.offBody")}</p>
        <div className="flex items-center gap-4 rounded-lg border border-line p-3">
          {thumb ? (
            <FramePlayer frames={[thumb]} delaysMs={[100]} className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />
          ) : (
            <div className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />
          )}
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="truncate text-[13px] font-semibold">{name(restore)}</span>
            <span className="num text-[12px] text-ink-3">
              {t("sc.frames", { count: restore.frames })} · {t("sc.uploadTime")} ~{fmtSeconds(uploadSeconds(restore.frames))}
            </span>
          </div>
        </div>
        {!usb && <Hint tone="usb">{t("sc.live.offNoUsb")}</Hint>}
        <div className="flex flex-wrap justify-end gap-2 pt-1">
          <button
            type="button"
            onClick={() => (alreadyOff ? onCancel() : onChoose(false))}
            className="h-10 rounded-lg border border-line px-4 text-[13px] text-ink-2 hover:bg-surface-2"
          >
            {alreadyOff ? t("sc.cancel") : t("sc.live.offKeep")}
          </button>
          <button
            type="button"
            autoFocus
            onClick={() => onChoose(true)}
            className="flex h-10 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink"
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
      <div className="num flex justify-between text-[12px] text-ink-2">
        <span>
          {ready} / {s.burst} {t("sc.live.ready")}
        </span>
        <span>
          {s.nextTokenMs != null
            ? t("sc.live.next", { wait: fmtWait(s.nextTokenMs, units) })
            : ready >= s.burst
              ? t("sc.live.full")
              : t("sc.live.refill", { wait: fmtWait(3_600_000 / (s.perDay / 24), units) })}
        </span>
      </div>
    </div>
  );
}

/** Send the waiting card without waiting for the budget. */
export function SendNow() {
  const t = useT();
  const sendNow = useLive((x) => x.sendNow);
  return (
    <button
      type="button"
      onClick={() => sendNow().catch(console.error)}
      title={t("sc.live.sendNow.hint")}
      className="ml-auto flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-signal/50 px-2.5 text-[12px] font-semibold text-signal hover:bg-signal/10"
      data-send-now
    >
      <Upload size={13} /> {t("sc.live.sendNow")}
    </button>
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
      <span className="truncate text-[13px] font-medium">
        {tr.artist ? `${tr.artist} — ${tr.title}` : tr.title}
      </span>
      <span className="text-[11px] text-ink-3">
        {tr.app ? `${tr.app} · ` : ""}
        {state}
      </span>
    </div>
  );
}

function StatusLine({ s, usb }: { s: LiveState; usb: boolean }) {
  const t = useT();
  const name = useItemName();
  const units = { h: t("sc.live.h"), m: t("sc.live.m") };
  const row = "flex items-center gap-2 text-[13px]";
  if (!s.enabled && !s.uploading) return <p className="text-[13px] text-ink-3">{t("sc.live.off")}</p>;
  return (
    <div className="flex flex-col gap-1.5 border-t border-line pt-4" data-live-status>
      {s.uploading && (
        <p className={`${row} text-ink`}>
          <Loader2 size={14} className="animate-spin text-signal" />
          {s.uploading.source === "restore" ? t("sc.live.restoring") : t("sc.live.uploading")}:{" "}
          <span className="truncate font-medium">
            {/* Going back puts a library item up (maybe the factory backup). */}
            {s.uploading.source === "restore" ? name(s.uploading) : s.uploading.name}
          </span>
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
                ? t("sc.live.waitBudget", { wait: fmtWait(s.nextTokenMs, units) })
                : t("sc.live.waitTurn")}
          </span>
          {usb && s.nextTokenMs != null && <SendNow />}
        </p>
      )}
      {s.last ? (
        <p className={`${row} text-ink-2`}>
          <Upload size={14} className="text-ok" />
          {/* After going back to the own picture the card is not on the screen any more. */}
          {s.restore ? t("sc.live.last") : t("sc.live.lastCard")}: <span className="truncate font-medium">{s.last.name}</span>
          <span className="num text-ink-3">
            · {new Date(s.last.atMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </span>
        </p>
      ) : (
        !s.pending && !s.uploading && <p className="text-[13px] text-ink-3">{t("sc.live.nothingYet")}</p>
      )}
      {s.lastError && !s.uploading && (
        <p className={`${row} text-warn`}>
          <AlertTriangle size={14} /> {friendlyError(s.lastError, t, true)}
        </p>
      )}
    </div>
  );
}
