// Pomodoro (phase C, second source). The timer, the key lights and the
// screen hold live in the backend; the UI shows the timer, changes its
// settings and draws the cards it asks for, which go through the live mode.

import { create } from "zustand";
import type { PomodoroAction } from "../bindings/PomodoroAction";
import type { PomodoroBlock } from "../bindings/PomodoroBlock";
import type { PomodoroCard } from "../bindings/PomodoroCard";
import type { PomodoroPhase } from "../bindings/PomodoroPhase";
import type { PomodoroSettings } from "../bindings/PomodoroSettings";
import type { PomodoroState } from "../bindings/PomodoroState";
import { inTauri, invoke, listen } from "./ipc";
import { mockCancelReturn, mockLiveHold, mockReturnAfter, submitLive, useLive } from "./live";
import { TRY_MS, phaseRgb, toHex } from "./pomodoroLights";
import type { Prepared } from "./screenImport";

export const POMODORO_SOURCE = "pomodoro";

const focus = (minutes: number): PomodoroBlock => ({ phase: "focus", minutes });
const rest = (minutes: number): PomodoroBlock => ({ phase: "break", minutes });

/** Ready-made sequences (owner's choice from the preview, 2026-11-12):
 *  "25 / 5 klasik" is the default. */
export const TEMPLATES = {
  classic: [focus(25), rest(5), focus(25), rest(5), focus(25), rest(15)],
  deep: [focus(50), rest(10), focus(50), rest(20)],
  empty: [focus(25)],
} satisfies Record<string, PomodoroBlock[]>;
export type TemplateId = keyof typeof TEMPLATES;

export const DEFAULT_SETTINGS: PomodoroSettings = {
  blocks: TEMPLATES.classic,
  repeat: 1,
  lights: true,
  screen: true,
  motion: "rightToLeft",
  focusColor: [255, 80, 20],
  breakColor: [30, 220, 100],
  brightness: 4,
  pulse: true,
  transition: "blink",
  sound: false,
  endScreen: "back",
};

/** After a stop, the user's own picture goes back up this long later
 *  (`BACK_AFTER` in pomodoro.rs). */
export const BACK_AFTER_MS = 15_000;

/** Block length in minutes, blocks in a sequence and runs of it
 *  (`BLOCK_MAX_MIN`, `BLOCKS_MAX`, `REPEAT_MAX` in dto.rs). */
export const LIMITS = { minutes: [1, 180], blocks: 16, repeat: 9 } as const;

/** Minutes of one run of the sequence. */
export function totalMinutes(s: PomodoroSettings) {
  return s.blocks.reduce((a, b) => a + b.minutes, 0);
}

/** Focus blocks in one run (at least 1). */
export function focusBlocks(s: PomodoroSettings) {
  return Math.max(1, s.blocks.filter((b) => b.phase === "focus").length);
}

/** The focus block block `i` (0-based) is or follows, 1-based. */
function roundAt(s: PomodoroSettings, i: number) {
  return Math.max(1, s.blocks.slice(0, i + 1).filter((b) => b.phase === "focus").length);
}

/** The template these blocks are, if any. */
export function templateOf(blocks: PomodoroBlock[]): TemplateId | null {
  const same = (a: PomodoroBlock[]) => a.length === blocks.length && a.every((b, i) => b.phase === blocks[i]?.phase && b.minutes === blocks[i]?.minutes);
  return (Object.keys(TEMPLATES) as TemplateId[]).find((k) => same(TEMPLATES[k])) ?? null;
}

/** A phase's colour (the user's choice) as CSS hex. */
export function phaseColor(s: PomodoroSettings, p: PomodoroPhase) {
  return toHex(phaseRgb(s, p));
}

/** Minutes of block `i` (the last one if the sequence got shorter). */
export function minutesAt(s: PomodoroSettings, i: number) {
  return (s.blocks[i] ?? s.blocks[s.blocks.length - 1])?.minutes ?? 25;
}

/** Time left now, counting down from the last state while it runs. */
export function remainingNow(s: PomodoroState, now = Date.now()) {
  return s.status === "running" ? Math.max(0, s.remainingMs - (now - s.atMs)) : s.remainingMs;
}

/** "24:59". */
export function fmtClock(ms: number) {
  const total = Math.ceil(ms / 1000);
  const m = Math.floor(total / 60);
  const sec = total % 60;
  return `${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

// ------------------------------------------------------------------ mock

/** Browser preview without Tauri: the same timer in memory. With
 *  `pomofast` in the address a minute lasts two seconds. */
const MINUTE = import.meta.env.DEV && typeof location !== "undefined" && location.search.includes("pomofast") ? 2_000 : 60_000;

const mock = {
  settings: { ...DEFAULT_SETTINGS },
  status: "idle" as PomodoroState["status"],
  phase: "focus" as PomodoroPhase,
  block: 0,
  cycle: 1,
  phaseMs: 0,
  anchor: 0,
  focusDone: 0,
  focusMs: 0,
  last: null as PomodoroState["last"],
  timer: 0,
  emit: null as ((s: PomodoroState) => void) | null,
  onCard: null as ((c: PomodoroCard) => void) | null,
  cardShown: false,
  trying: 0,
};

function mockElapsed(now = Date.now()) {
  if (mock.status === "running") return Math.min(mock.phaseMs, Math.max(0, now - mock.anchor));
  if (mock.status === "paused") return Math.min(mock.phaseMs, mock.anchor);
  return 0;
}

function mockState(): PomodoroState {
  const now = Date.now();
  return {
    settings: { ...mock.settings },
    status: mock.status,
    phase: mock.phase,
    block: mock.block + 1,
    round: roundAt(mock.settings, mock.block),
    cycle: mock.cycle,
    phaseMs: mock.phaseMs,
    remainingMs: Math.max(0, mock.phaseMs - mockElapsed(now)),
    atMs: now,
    focusDone: mock.focusDone,
    last: mock.last,
    trying: mock.trying > 0,
  };
}

function mockCardsWanted() {
  const live = useLive.getState().state;
  return mock.status !== "idle" && mock.settings.screen && !!live?.enabled;
}

function mockSync() {
  mockLiveHold(POMODORO_SOURCE, mockCardsWanted());
  mock.emit?.(mockState());
}

/** Whether the sequence runs again after its last block. */
const mockRunsAgain = () => mock.settings.repeat === 0 || mock.cycle < mock.settings.repeat;

function mockNext(): PomodoroBlock | null {
  const b = mock.settings.blocks;
  if (mock.block + 1 < b.length) return b[mock.block + 1] ?? null;
  return mockRunsAgain() ? (b[0] ?? null) : null;
}

function mockBegin(block: number) {
  const s = mock.settings;
  mock.block = Math.min(block, s.blocks.length - 1);
  const phase = s.blocks[mock.block]?.phase ?? "focus";
  mock.phase = phase;
  mock.phaseMs = minutesAt(s, mock.block) * MINUTE;
  mock.anchor = Date.now();
  mock.status = "running";
  if (mockCardsWanted()) {
    const card: PomodoroCard = {
      kind: "phase",
      phase,
      round: roundAt(s, mock.block),
      rounds: focusBlocks(s),
      cycle: mock.cycle,
      cycles: s.repeat,
      minutes: minutesAt(s, mock.block),
      next: mockNext(),
      focusDone: mock.focusDone,
      focusMin: 0,
      complete: false,
      endedMs: 0,
    };
    window.setTimeout(() => mock.onCard?.(card), 1000);
    mock.cardShown = true;
  }
}

function mockEnd(complete: boolean) {
  // At the natural end the last block is counted already.
  const current = mock.phase === "focus" && !complete ? mockElapsed() : 0;
  const summary = {
    focusDone: mock.focusDone,
    focusMin: Math.round((mock.focusMs + current) / MINUTE),
    complete,
    endedMs: Date.now(),
  };
  const cards = mockCardsWanted();
  Object.assign(mock, { status: "idle", phase: "focus", block: 0, cycle: 1, phaseMs: 0, anchor: 0, focusDone: 0, focusMs: 0 });
  mock.last = summary;
  // Like the backend: a playing song takes the screen back by itself (its
  // card cancels the return if it can go); otherwise the user's own picture,
  // or a summary card.
  const playing = !!useLive.getState().state?.track?.playing;
  if (cards && mock.cardShown) {
    if (mock.settings.endScreen === "summary" && !playing) {
      mock.onCard?.({ kind: "done", phase: "focus", round: 0, rounds: 0, cycle: 0, cycles: 0, minutes: 0, next: null, ...summary });
    } else mockReturnAfter(POMODORO_SOURCE, MINUTE < 60_000 ? 3_000 : BACK_AFTER_MS);
  }
  mock.cardShown = false;
}

function mockAdvance() {
  if (mock.phase === "focus") {
    mock.focusMs += mockElapsed();
    mock.focusDone += 1;
  }
  if (mock.block + 1 < mock.settings.blocks.length) mockBegin(mock.block + 1);
  else if (mockRunsAgain()) {
    mock.cycle += 1;
    mockBegin(0);
  } else mockEnd(true);
}

function mockTick() {
  if (mock.status === "running" && Date.now() - mock.anchor >= mock.phaseMs) {
    mockAdvance();
    mockSync();
  }
  if (mock.status === "idle") {
    window.clearInterval(mock.timer);
    mock.timer = 0;
  }
}

function mockControl(action: PomodoroAction): PomodoroState {
  if (action !== "try" && mock.trying) {
    window.clearTimeout(mock.trying);
    mock.trying = 0;
  }
  if (action === "try") {
    if (mock.status === "idle" && !mock.trying) {
      mock.trying = window.setTimeout(() => {
        mock.trying = 0;
        mockSync();
      }, TRY_MS);
    }
  } else if (action === "start") {
    mockCancelReturn(POMODORO_SOURCE);
    if (mock.status === "idle") {
      Object.assign(mock, { block: 0, cycle: 1, focusDone: 0, focusMs: 0 });
      mockBegin(0);
    } else if (mock.status === "paused") {
      mock.anchor = Date.now() - mock.anchor;
      mock.status = "running";
    }
  } else if (action === "pause" && mock.status === "running") {
    mock.anchor = mockElapsed();
    mock.status = "paused";
  } else if (action === "skip" && mock.status !== "idle") {
    mockAdvance();
  } else if (action === "stop" && mock.status !== "idle") {
    mockEnd(false);
  }
  if (mock.status !== "idle" && !mock.timer) mock.timer = window.setInterval(mockTick, 250);
  mockSync();
  return mockState();
}

// ------------------------------------------------------------------- ipc

export function getPomodoro(): Promise<PomodoroState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockState());
  return invoke<PomodoroState>("pomodoro_state");
}

export function controlPomodoro(action: PomodoroAction): Promise<PomodoroState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockControl(action));
  return invoke<PomodoroState>("pomodoro_control", { action });
}

export function configurePomodoro(settings: PomodoroSettings): Promise<PomodoroState> {
  if (import.meta.env.DEV && !inTauri) {
    // Cards turned off while it runs: its card leaves the screen.
    const wanted = mockCardsWanted();
    mock.settings = { ...settings };
    if (wanted && !mockCardsWanted()) {
      mockReturnAfter(POMODORO_SOURCE, MINUTE < 60_000 ? 3_000 : BACK_AFTER_MS);
    }
    if (mock.status !== "idle") {
      mock.block = Math.min(mock.block, settings.blocks.length - 1);
      mock.phase = settings.blocks[mock.block]?.phase ?? "focus";
      mock.phaseMs = minutesAt(settings, mock.block) * MINUTE;
      if (settings.repeat !== 0) mock.cycle = Math.min(mock.cycle, settings.repeat);
    }
    mockSync();
    return Promise.resolve(mockState());
  }
  return invoke<PomodoroState>("pomodoro_configure", { settings });
}

// ----------------------------------------------------------------- store

interface PomodoroStore {
  state: PomodoroState | null;
  /** Card drawing failed. */
  drawError: string | null;
  load: () => Promise<void>;
  control: (action: PomodoroAction) => Promise<void>;
  configure: (patch: Partial<PomodoroSettings>) => Promise<void>;
}

export const usePomodoro = create<PomodoroStore>((set, get) => ({
  state: null,
  drawError: null,
  load: async () => set({ state: await getPomodoro() }),
  control: async (action) => set({ state: await controlPomodoro(action) }),
  configure: async (patch) => {
    const s = get().state;
    const settings = { ...(s?.settings ?? DEFAULT_SETTINGS), ...patch };
    // Show the change at once; the backend answers with the same.
    if (s) set({ state: { ...s, settings } });
    set({ state: await configurePomodoro(settings) });
  },
}));

/**
 * Listen to the backend for the whole app session: timer changes, and the
 * cards it asks for, which are drawn by `draw` and handed to the live mode.
 */
export function startPomodoro(draw: (c: PomodoroCard) => Promise<Prepared>): () => void {
  const onCard = async (c: PomodoroCard) => {
    const live = useLive.getState().state;
    if (!live?.enabled) return;
    try {
      const card = await draw(c);
      useLive.setState({ state: await submitLive(card, POMODORO_SOURCE) });
      usePomodoro.setState({ drawError: null });
    } catch (e) {
      console.error(e);
      usePomodoro.setState({ drawError: String(e) });
    }
  };
  usePomodoro.getState().load().catch(console.error);
  if (import.meta.env.DEV && !inTauri) {
    mock.emit = (s) => usePomodoro.setState({ state: s });
    mock.onCard = (c) => void onCard(c);
    // The live mode switch changes whether the mock holds the screen.
    const off = useLive.subscribe((x, prev) => {
      if (x.state?.enabled !== prev.state?.enabled) mockSync();
    });
    return () => {
      mock.emit = null;
      mock.onCard = null;
      off();
    };
  }
  const offs = [
    listen<PomodoroState>("pomodoro", (e) => usePomodoro.setState({ state: e.payload })),
    listen<PomodoroCard>("pomodoro-card", (e) => void onCard(e.payload)),
  ];
  return () => offs.forEach((p) => p.then((off) => off()).catch(console.error));
}
