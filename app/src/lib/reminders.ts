// Hatırlatıcı (phase C, fourth source). The backend keeps the reminders,
// decides when each goes off, sweeps the wave over the keys and asks for a
// card (app/src-tauri/src/reminders.rs); the UI sets them up and draws the
// cards, which go through the live mode. The browser preview keeps a small
// copy of the schedule for its fake reminders: keep the two in step.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { Reminder } from "../bindings/Reminder";
import type { ReminderCard } from "../bindings/ReminderCard";
import type { ReminderIcon } from "../bindings/ReminderIcon";
import type { ReminderSettings } from "../bindings/ReminderSettings";
import type { ReminderState } from "../bindings/ReminderState";
import type { ReminderWhen } from "../bindings/ReminderWhen";
import type { MessageKey, Translate } from "./i18n";
import { inTauri } from "./ipc";
import { mockLiveHold, mockReturnAfter, submitLive, useLive } from "./live";
import type { Prepared } from "./screenImport";

export const REMINDER_SOURCE = "reminder";

/** Bit 0 Monday … bit 6 Sunday. */
export const WEEKDAYS = 0b001_1111;
export const WEEKEND = 0b110_0000;
export const EVERY_DAY = 0b111_1111;

export const DEFAULT_REMINDERS: ReminderSettings = { enabled: true, screen: true, stayMin: 2, awayWait: true, notify: false };
export const STAY_CHOICES = [1, 2, 5] as const;
export const EVERY_CHOICES = [20, 30, 45, 60, 90] as const;

export type Rgb = [number, number, number];

/** Colours offered for a reminder (the wave and the card). */
export const REMINDER_COLORS: Rgb[] = [
  [59, 164, 245],
  [76, 195, 138],
  [139, 124, 246],
  [255, 122, 89],
  [255, 176, 32],
  [236, 72, 153],
];

export const ICONS: ReminderIcon[] = ["bell", "drop", "stand", "eye", "pill"];

export type TemplateId = "water" | "stand" | "eyes" | "pill" | "own";

/** The next round hour from now, for a new one-time reminder. */
function nextHour() {
  const d = new Date();
  d.setMinutes(0, 0, 0);
  d.setHours(d.getHours() + 1);
  return d.getTime();
}

/** Ready-made reminders (owner's choice: frequent ones light only, the
 *  important ones with a card too). */
export const TEMPLATES: {
  id: TemplateId;
  label: MessageKey;
  note?: MessageKey;
  icon: ReminderIcon;
  color: Rgb;
  card: boolean;
  when: () => ReminderWhen;
}[] = [
  { id: "water", label: "reminder.tpl.water", icon: "drop", color: [59, 164, 245], card: false, when: () => ({ kind: "every", every: 60, from: 540, to: 1080, days: WEEKDAYS }) },
  { id: "stand", label: "reminder.tpl.stand", icon: "stand", color: [76, 195, 138], card: false, when: () => ({ kind: "every", every: 45, from: 540, to: 1080, days: WEEKDAYS }) },
  { id: "eyes", label: "reminder.tpl.eyes", note: "reminder.tpl.eyes.note", icon: "eye", color: [139, 124, 246], card: false, when: () => ({ kind: "every", every: 20, from: 540, to: 1080, days: WEEKDAYS }) },
  { id: "pill", label: "reminder.tpl.pill", icon: "pill", color: [255, 122, 89], card: true, when: () => ({ kind: "daily", minute: 14 * 60, days: EVERY_DAY }) },
  { id: "own", label: "reminder.tpl.own", icon: "bell", color: [255, 176, 32], card: true, when: () => ({ kind: "once", at: nextHour() }) },
];

export function fromTemplate(id: TemplateId, t: (k: MessageKey) => string): Reminder {
  const tpl = TEMPLATES.find((x) => x.id === id)!;
  return {
    id: "",
    title: id === "own" ? "" : t(tpl.label),
    note: tpl.note ? t(tpl.note) : "",
    icon: tpl.icon,
    color: [...tpl.color],
    when: tpl.when(),
    enabled: true,
    card: tpl.card,
    sound: false,
  };
}

export const rgbCss = (c: readonly number[]) => `rgb(${c[0]},${c[1]},${c[2]})`;

/** "09:00" from minutes after midnight. */
export function minuteText(m: number) {
  const h = Math.floor(m / 60) % 24;
  return `${String(h).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

/** Minutes after midnight from "09:00". */
export function parseMinute(s: string): number | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(s.trim());
  if (!m) return null;
  const v = Number(m[1]) * 60 + Number(m[2]);
  return Number(m[1]) < 24 && Number(m[2]) < 60 ? v : null;
}

/** "14:00". */
export function hhmm(ms: number) {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** "Her gün", "Hafta içi", "Pzt, Çar". */
export function daysText(days: number, t: (k: MessageKey) => string) {
  const d = days & EVERY_DAY;
  if (d === EVERY_DAY) return t("reminder.days.all");
  if (d === WEEKDAYS) return t("reminder.days.weekdays");
  if (d === WEEKEND) return t("reminder.days.weekend");
  return [0, 1, 2, 3, 4, 5, 6]
    .filter((i) => d & (1 << i))
    .map((i) => t(`reminder.day.${i}` as MessageKey))
    .join(", ");
}

/** "Bugün", "Yarın" or "13 Kas". */
export function dayWord(ms: number, t: (k: MessageKey) => string, locale: string) {
  const d = new Date(ms);
  const today = new Date();
  const tomorrow = new Date(today.getTime() + 86_400_000);
  if (d.toDateString() === today.toDateString()) return t("reminder.today");
  if (d.toDateString() === tomorrow.toDateString()) return t("reminder.tomorrow");
  return d.toLocaleDateString(locale, { day: "numeric", month: "short" });
}

/** How a reminder repeats, in words. */
export function whenText(w: ReminderWhen, t: Translate, locale: string) {
  switch (w.kind) {
    case "once":
      return `${dayWord(w.at, t, locale)} ${hhmm(w.at)} · ${t("reminder.once")}`;
    case "daily":
      return `${daysText(w.days, t)} ${minuteText(w.minute)}`;
    case "every":
      return `${t("reminder.everyMin", { min: w.every })} · ${minuteText(w.from)}–${minuteText(w.to)} · ${daysText(w.days, t).toLocaleLowerCase(locale)}`;
  }
}

/** How many times a day it goes off at most. */
export function perDay(w: ReminderWhen) {
  return w.kind === "every" ? Math.max(0, Math.floor((w.to - w.from) / w.every)) : 1;
}

/** Error words from the backend, in the UI language. */
export function reminderError(e: unknown, t: (k: MessageKey) => string) {
  const s = String(e);
  if (s.includes("title empty")) return t("reminder.err.title");
  if (s.includes("time passed")) return t("reminder.err.passed");
  if (s.includes("too many")) return t("reminder.err.many");
  return t("reminder.err.time");
}

// ------------------------------------------------------------------ mock

const MIN = 60_000;

function atMinute(day: Date, minute: number) {
  const d = new Date(day);
  d.setHours(0, 0, 0, 0);
  d.setMinutes(minute);
  return d.getTime();
}

const dayOn = (days: number, d: Date) => (days & (1 << ((d.getDay() + 6) % 7))) !== 0;

/** The backend's `next_timed` and `next_every`. */
function mockNext(w: ReminderWhen, after: number, anchor: number): number | null {
  if (w.kind === "once") return w.at > after ? w.at : null;
  const start = new Date(w.kind === "every" ? anchor : after);
  for (let i = 0; i < 9; i++) {
    const day = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
    if (!dayOn(w.days, day)) continue;
    if (w.kind === "daily") {
      const t = atMinute(day, w.minute);
      if (t > after) return t;
    } else {
      const t = Math.max(i === 0 ? anchor : -Infinity, atMinute(day, w.from)) + w.every * MIN;
      if (t <= atMinute(day, w.to)) return t;
    }
  }
  return null;
}

const mock = {
  settings: { ...DEFAULT_REMINDERS },
  reminders: [] as Reminder[],
  /** Next time of each, and a repeating one's anchor. */
  next: new Map<string, number | null>(),
  anchor: new Map<string, number>(),
  showing: null as { id: string; until: number } | null,
  timer: 0,
  emit: null as ((s: ReminderState) => void) | null,
  onCard: null as ((c: ReminderCard) => void) | null,
};

function mockState(): ReminderState {
  const now = Date.now();
  return {
    settings: { ...mock.settings },
    reminders: mock.reminders.map((r) => ({
      reminder: { ...r },
      nextMs: mock.settings.enabled && r.enabled ? mockNext(r.when, now, mock.anchor.get(r.id) ?? now) : null,
    })),
    showing: mock.showing?.id ?? null,
    showingUntilMs: mock.showing?.until ?? null,
    away: false,
  };
}

function mockTick() {
  const now = Date.now();
  const liveOn = !!useLive.getState().state?.enabled;
  if (mock.settings.enabled) {
    for (const r of mock.reminders.filter((r) => r.enabled)) {
      const t = mockNext(r.when, now - 1_000, mock.anchor.get(r.id) ?? now);
      if (t == null || t > now) continue;
      mock.anchor.set(r.id, t);
      if (r.when.kind === "once") r.enabled = false;
      const held = useLive.getState().state?.holders.includes("calendar");
      if (r.card && mock.settings.screen && liveOn && !held) {
        mock.showing = { id: r.id, until: now + mock.settings.stayMin * MIN };
        mockLiveHold(REMINDER_SOURCE, true);
        mock.onCard?.({ reminder: { ...r }, atMs: t, nextMs: r.when.kind === "once" ? null : mockNext(r.when, t, t) });
      }
    }
  }
  if (mock.showing && (now >= mock.showing.until || !liveOn)) {
    mock.showing = null;
    mockLiveHold(REMINDER_SOURCE, false);
    if (liveOn) mockReturnAfter(REMINDER_SOURCE, 5_000);
  }
  mock.emit?.(mockState());
}

function mockStart() {
  if (!mock.timer) mock.timer = window.setInterval(mockTick, 1_000);
}

// ------------------------------------------------------------------- ipc

export function getReminders(): Promise<ReminderState> {
  if (!inTauri) return Promise.resolve(mockState());
  return invoke<ReminderState>("reminders_state");
}

export function configureReminders(settings: ReminderSettings): Promise<ReminderState> {
  if (!inTauri) {
    mock.settings = { ...settings };
    return Promise.resolve(mockState());
  }
  return invoke<ReminderState>("reminders_configure", { settings });
}

export function saveReminder(reminder: Reminder): Promise<ReminderState> {
  if (!inTauri) {
    const r = { ...reminder, title: reminder.title.trim(), note: reminder.note.trim() };
    if (!r.title) return Promise.reject(new Error("title empty"));
    if (r.enabled && r.when.kind === "once" && r.when.at <= Date.now()) return Promise.reject(new Error("time passed"));
    if (r.when.kind === "every" && r.when.from >= r.when.to) return Promise.reject(new Error("bad time"));
    if (!r.id) r.id = `r${Date.now().toString(16)}${mock.reminders.length}`;
    const i = mock.reminders.findIndex((x) => x.id === r.id);
    if (i >= 0) mock.reminders[i] = r;
    else mock.reminders.push(r);
    mock.anchor.set(r.id, Date.now());
    mockStart();
    return Promise.resolve(mockState());
  }
  return invoke<ReminderState>("reminders_save", { reminder });
}

export function removeReminder(id: string): Promise<ReminderState> {
  if (!inTauri) {
    mock.reminders = mock.reminders.filter((r) => r.id !== id);
    return Promise.resolve(mockState());
  }
  return invoke<ReminderState>("reminders_remove", { id });
}

/** "Dene": the wave in this colour on the keys (and the sound). */
export function tryReminderLight(color: Rgb, sound: boolean): Promise<void> {
  if (!inTauri) return Promise.resolve();
  return invoke("reminders_try", { color, sound });
}

/** After a tried card went to the live mode: send it now, own picture
 *  back after a minute. */
export function tryReminderCardBack(): Promise<void> {
  if (!inTauri) {
    mockReturnAfter(REMINDER_SOURCE, 60_000);
    return Promise.resolve();
  }
  return invoke("reminders_try_card");
}

/** The card a reminder would show if it went off at its next time. */
export function previewCard(r: Reminder): ReminderCard {
  const now = Date.now();
  const at = mockNext(r.when, now, now) ?? (r.when.kind === "once" ? r.when.at : now);
  return { reminder: r, atMs: at, nextMs: r.when.kind === "once" ? null : mockNext(r.when, at, at) };
}

// ----------------------------------------------------------------- store

interface RemindersStore {
  state: ReminderState | null;
  /** Card drawing failed. */
  drawError: string | null;
  load: () => Promise<void>;
  configure: (patch: Partial<ReminderSettings>) => Promise<void>;
  save: (r: Reminder) => Promise<ReminderState>;
  remove: (id: string) => Promise<void>;
}

export const useReminders = create<RemindersStore>((set, get) => ({
  state: null,
  drawError: null,
  load: async () => set({ state: await getReminders() }),
  configure: async (patch) => {
    const s = get().state;
    const settings = { ...(s?.settings ?? DEFAULT_REMINDERS), ...patch };
    if (s) set({ state: { ...s, settings } });
    set({ state: await configureReminders(settings) });
  },
  save: async (r) => {
    const state = await saveReminder(r);
    set({ state });
    return state;
  },
  remove: async (id) => set({ state: await removeReminder(id) }),
}));

/**
 * Listen to the backend for the whole app session: the reminders' state,
 * and the cards it asks for, which are drawn by `draw` and handed to the
 * live mode.
 */
export function startReminders(draw: (c: ReminderCard) => Promise<Prepared>): () => void {
  const onCard = async (c: ReminderCard) => {
    if (!useLive.getState().state?.enabled) return;
    try {
      const card = await draw(c);
      useLive.setState({ state: await submitLive(card, REMINDER_SOURCE) });
      useReminders.setState({ drawError: null });
    } catch (e) {
      console.error(e);
      useReminders.setState({ drawError: String(e) });
    }
  };
  useReminders.getState().load().catch(console.error);
  if (!inTauri) {
    mock.emit = (s) => useReminders.setState({ state: s });
    mock.onCard = (c) => void onCard(c);
    return () => {
      mock.emit = null;
      mock.onCard = null;
      window.clearInterval(mock.timer);
      mock.timer = 0;
    };
  }
  const offs = [
    listen<ReminderState>("reminders", (e) => useReminders.setState({ state: e.payload })),
    listen<ReminderCard>("reminder-card", (e) => void onCard(e.payload)),
  ];
  return () => offs.forEach((p) => p.then((off) => off()).catch(console.error));
}
