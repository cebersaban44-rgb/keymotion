// Takvim (phase C, third source). The backend reads the calendars, decides
// when a meeting's card is due and blinks the reminder key
// (app/src-tauri/src/calendar.rs); the UI sets it up, shows the meetings
// ahead and draws the cards it asks for, which go through the live mode.
// The browser preview keeps a small copy of the card times for its fake
// meetings: keep the two in step.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { CalendarCard } from "../bindings/CalendarCard";
import type { CalendarEvent } from "../bindings/CalendarEvent";
import type { CalendarProvider } from "../bindings/CalendarProvider";
import type { CalendarSettings } from "../bindings/CalendarSettings";
import type { CalendarState } from "../bindings/CalendarState";
import { inTauri } from "./ipc";
import { mockLiveHold, mockReturnAfter, submitLive, useLive } from "./live";
import type { Prepared } from "./screenImport";

export const CALENDAR_SOURCE = "calendar";

export const DEFAULT_CALENDAR: CalendarSettings = {
  screen: true,
  leadMin: 10,
  until: "shortly",
  reminder: true,
  reminderKey: "esc",
};

export const LEAD_CHOICES = [5, 10, 15, 30] as const;

/** Where a calendar comes from, by the host of its address. */
export function providerOf(url: string): CalendarProvider {
  const host = (url.trim().split("://")[1] ?? "").split(/[/?#]/)[0]!.toLowerCase();
  if (host.endsWith("google.com")) return "google";
  if (host.includes("outlook.") || host.endsWith("office365.com") || host.endsWith("live.com")) return "outlook";
  if (host.endsWith("icloud.com")) return "apple";
  return "other";
}

/** The backend takes `https://` and `webcal://` addresses only. */
export function addressOk(url: string) {
  return /^(https|webcals?):\/\/[^\s/?#]+/i.test(url.trim());
}

const rgb = (c: readonly number[]) => `rgb(${c[0]},${c[1]},${c[2]})`;
export const calendarColor = rgb;

/** "14:00". */
export function hhmm(ms: number) {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

// ------------------------------------------------------------------ mock

const MIN = 60_000;
const SHORTLY = 5 * MIN;
const BRIDGE = 3 * MIN;

function mockEvents(source: string, color: [number, number, number]): CalendarEvent[] {
  // Round to the minute: a meeting soon, one later, one tomorrow, a whole day.
  const now = Math.floor(Date.now() / MIN) * MIN;
  const day = new Date();
  day.setHours(0, 0, 0, 0);
  const ev = (title: string, location: string, start: number, len: number, allDay = false): CalendarEvent => ({
    title,
    location,
    startMs: start,
    endMs: start + len,
    allDay,
    color,
    source,
  });
  return [
    ev("Proje teslimi", "", day.getTime(), 24 * 60 * MIN, true),
    ev("Günlük toplantı", "Meet", now + 3 * MIN, 15 * MIN),
    ev("Tasarım incelemesi", "Toplantı odası 2", now + 120 * MIN, 60 * MIN),
    ev("Birebir görüşme", "Ofis", now + 22 * 60 * MIN, 30 * MIN),
  ];
}

/** The backend's `card_windows` and `showing`. */
function mockShowing(events: CalendarEvent[], s: CalendarSettings, now: number): CalendarEvent | null {
  const windows: [number, number, CalendarEvent][] = [];
  for (const e of events.filter((e) => !e.allDay).sort((a, b) => a.startMs - b.startMs)) {
    const to = s.until === "start" ? e.startMs : s.until === "shortly" ? e.startMs + SHORTLY : Math.max(e.endMs, e.startMs + SHORTLY);
    let from = e.startMs - s.leadMin * MIN;
    const prev = Math.max(...windows.map((w) => w[1]), -Infinity);
    if (from > prev && from - prev < BRIDGE) from = prev;
    windows.push([from, to, e]);
  }
  const due = windows.filter(([from, to]) => from <= now && now < to).map((w) => w[2]);
  return due.sort((a, b) => b.startMs - a.startMs)[0] ?? null;
}

function mockNext(events: CalendarEvent[], s: CalendarSettings, now: number): number | null {
  const froms = events.filter((e) => !e.allDay).map((e) => e.startMs - s.leadMin * MIN).filter((f) => f > now);
  return froms.length ? Math.min(...froms) : null;
}

const mock = {
  settings: { ...DEFAULT_CALENDAR },
  sources: [] as (CalendarState["sources"][number] & { list: CalendarEvent[] })[],
  shownKey: null as string | null,
  timer: 0,
  emit: null as ((s: CalendarState) => void) | null,
  onCard: null as ((c: CalendarCard) => void) | null,
};

function mockAll() {
  return mock.sources
    .filter((s) => s.enabled)
    .flatMap((s) => s.list.map((e) => ({ ...e, color: s.color })))
    .sort((a, b) => a.startMs - b.startMs);
}

function mockState(): CalendarState {
  const now = Date.now();
  const all = mockAll();
  return {
    settings: { ...mock.settings },
    sources: mock.sources.map(({ list, ...s }) => ({ ...s, events: list.filter((e) => e.endMs > now).length })),
    upcoming: all.filter((e) => e.endMs > now && e.startMs < now + 24 * 60 * MIN).slice(0, 12),
    showing: mock.settings.screen ? mockShowing(all, mock.settings, now) : null,
    nextCardMs: mock.settings.screen ? mockNext(all, mock.settings, now) : null,
    reading: false,
  };
}

function mockTick() {
  const s = mockState();
  const liveOn = !!useLive.getState().state?.enabled;
  const show = liveOn ? s.showing : null;
  const key = show ? `${show.source}@${show.startMs}` : null;
  if (key !== mock.shownKey) {
    mock.shownKey = key;
    mockLiveHold(CALENDAR_SOURCE, !!show);
    if (show) {
      const all = mockAll();
      const next = all.find((n) => !n.allDay && n.startMs > show.startMs && n.startMs < show.startMs + 12 * 60 * MIN) ?? null;
      mock.onCard?.({ event: show, next });
    } else if (liveOn) {
      mockReturnAfter(CALENDAR_SOURCE, 5_000);
    }
  }
  mock.emit?.(s);
}

function mockStart() {
  if (!mock.timer) mock.timer = window.setInterval(mockTick, 1_000);
}

// ------------------------------------------------------------------- ipc

export function getCalendar(): Promise<CalendarState> {
  if (!inTauri) return Promise.resolve(mockState());
  return invoke<CalendarState>("calendar_state");
}

export function configureCalendar(settings: CalendarSettings): Promise<CalendarState> {
  if (!inTauri) {
    mock.settings = { ...settings };
    return Promise.resolve(mockState());
  }
  return invoke<CalendarState>("calendar_configure", { settings });
}

export async function addCalendar(url: string, name: string): Promise<CalendarState> {
  if (!inTauri) {
    if (!addressOk(url)) throw new Error("not https");
    await new Promise((r) => setTimeout(r, 600));
    const n = mock.sources.length;
    const colors: [number, number, number][] = [
      [66, 133, 244],
      [52, 168, 83],
      [234, 67, 53],
    ];
    const color = colors[n % colors.length]!;
    const id = `c${Date.now().toString(16)}${n}`;
    const host = (url.trim().split("://")[1] ?? "").split(/[/?#]/)[0]!.toLowerCase();
    mock.sources.push({
      id,
      name: name.trim() || host,
      provider: providerOf(url),
      host,
      color,
      enabled: true,
      lastOkMs: Date.now(),
      error: null,
      events: 0,
      list: mockEvents(id, color),
    });
    mockStart();
    return mockState();
  }
  return invoke<CalendarState>("calendar_add", { url, name });
}

export function updateCalendar(
  id: string,
  patch: { name?: string; color?: [number, number, number]; enabled?: boolean },
): Promise<CalendarState> {
  if (!inTauri) {
    const src = mock.sources.find((s) => s.id === id);
    if (src) Object.assign(src, patch);
    return Promise.resolve(mockState());
  }
  return invoke<CalendarState>("calendar_update", {
    id,
    name: patch.name ?? null,
    color: patch.color ?? null,
    enabled: patch.enabled ?? null,
  });
}

export function removeCalendar(id: string): Promise<CalendarState> {
  if (!inTauri) {
    mock.sources = mock.sources.filter((s) => s.id !== id);
    return Promise.resolve(mockState());
  }
  return invoke<CalendarState>("calendar_remove", { id });
}

export function refreshCalendars(): Promise<CalendarState> {
  if (!inTauri) {
    for (const s of mock.sources) s.lastOkMs = Date.now();
    return Promise.resolve(mockState());
  }
  return invoke<CalendarState>("calendar_refresh");
}

export function tryReminder(): Promise<void> {
  if (!inTauri) return Promise.resolve();
  return invoke("calendar_try_reminder");
}

// ----------------------------------------------------------------- store

interface CalendarStore {
  state: CalendarState | null;
  /** Card drawing failed. */
  drawError: string | null;
  load: () => Promise<void>;
  configure: (patch: Partial<CalendarSettings>) => Promise<void>;
  add: (url: string, name: string) => Promise<CalendarState>;
  update: (id: string, patch: { name?: string; color?: [number, number, number]; enabled?: boolean }) => Promise<void>;
  remove: (id: string) => Promise<void>;
  refresh: () => Promise<void>;
}

export const useCalendar = create<CalendarStore>((set, get) => ({
  state: null,
  drawError: null,
  load: async () => set({ state: await getCalendar() }),
  configure: async (patch) => {
    const s = get().state;
    const settings = { ...(s?.settings ?? DEFAULT_CALENDAR), ...patch };
    if (s) set({ state: { ...s, settings } });
    set({ state: await configureCalendar(settings) });
  },
  add: async (url, name) => {
    const state = await addCalendar(url, name);
    set({ state });
    return state;
  },
  update: async (id, patch) => set({ state: await updateCalendar(id, patch) }),
  remove: async (id) => set({ state: await removeCalendar(id) }),
  refresh: async () => set({ state: await refreshCalendars() }),
}));

/**
 * Listen to the backend for the whole app session: the calendars' state,
 * and the cards it asks for, which are drawn by `draw` and handed to the
 * live mode.
 */
export function startCalendar(draw: (c: CalendarCard) => Promise<Prepared>): () => void {
  const onCard = async (c: CalendarCard) => {
    if (!useLive.getState().state?.enabled) return;
    try {
      const card = await draw(c);
      useLive.setState({ state: await submitLive(card, CALENDAR_SOURCE) });
      useCalendar.setState({ drawError: null });
    } catch (e) {
      console.error(e);
      useCalendar.setState({ drawError: String(e) });
    }
  };
  useCalendar.getState().load().catch(console.error);
  if (!inTauri) {
    mock.emit = (s) => useCalendar.setState({ state: s });
    mock.onCard = (c) => void onCard(c);
    return () => {
      mock.emit = null;
      mock.onCard = null;
      window.clearInterval(mock.timer);
      mock.timer = 0;
    };
  }
  const offs = [
    listen<CalendarState>("calendar", (e) => useCalendar.setState({ state: e.payload })),
    listen<CalendarCard>("calendar-card", (e) => void onCard(e.payload)),
  ];
  return () => offs.forEach((p) => p.then((off) => off()).catch(console.error));
}
