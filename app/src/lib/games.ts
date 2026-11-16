// Game session ("Oyun oturumu"). The backend notices games, keeps the
// session, its readings and the history, and lights the keys
// (app/src-tauri/src/games.rs); this file follows its state, draws the
// cards it asks for and hands them to the live screen. The browser preview
// keeps a small made-up state instead.

import { create } from "zustand";
import type { GameCard } from "../bindings/GameCard";
import type { GamesSettings } from "../bindings/GamesSettings";
import type { GamesState } from "../bindings/GamesState";
import { inTauri, invoke, listen } from "./ipc";
import { submitLive, useLive } from "./live";
import type { Prepared } from "./screenImport";

/** Live screen source id (games.rs `SOURCE`). */
export const GAME_SOURCE = "game";

/** Keys lit by default: movement and the keys next to it. */
export const GAME_KEYS_DEFAULT = ["w", "a", "s", "d", "lshift", "lctrl", "space"];

/** Ready-made key sets. */
export const GAME_KEY_SETS: Record<"wasd" | "moba" | "numbers", string[]> = {
  wasd: GAME_KEYS_DEFAULT,
  moba: ["q", "w", "e", "r", "d", "f", "b"],
  numbers: ["1", "2", "3", "4", "5", "6"],
};

export const DEFAULT_GAMES: GamesSettings = {
  enabled: false,
  launchers: true,
  startCard: true,
  summaryCard: true,
  lights: "own",
  keys: GAME_KEYS_DEFAULT,
  color: [255, 138, 61],
  rest: "dim",
  brightness: 4,
  quiet: false,
  marks: [],
  lol: { enabled: true, bars: true, death: true, events: true },
};

/** Sessions shorter than this get no summary card (games.rs). */
export const SUMMARY_MIN_MS = 5 * 60_000;

// ------------------------------------------------------------------ mock

const HOUR = 3_600_000;
const mock = {
  state: {
    settings: { ...DEFAULT_GAMES },
    session: null,
    games: [
      { key: "c:\\steam\\steamapps\\common\\example", name: "Örnek Oyun", source: "steam", totalMs: 31 * HOUR, lastMs: Date.now() - 2 * HOUR, played: true },
      { key: "c:\\riot games\\league of legends\\game\\league of legends.exe", name: "League of Legends", source: "riot", totalMs: 12 * HOUR, lastMs: Date.now() - 26 * HOUR, played: true },
      { key: "d:\\games\\racer\\racer.exe", name: "Yarış Oyunu", source: "mine", totalMs: 4 * HOUR, lastMs: Date.now() - 5 * 24 * HOUR, played: true },
      { key: "c:\\riot games\\valorant\\live\\shootergame\\binaries\\win64\\valorant-win64-shipping.exe", name: "VALORANT", source: "riot", totalMs: 0, lastMs: null, played: false },
    ],
    suggestions: [{ path: "D:\\Emulators\\emulator.exe", name: "emulator" }],
    todayMs: 2 * HOUR + 2 * 60_000,
    weekMs: 9 * HOUR + 40 * 60_000,
    recent: [
      {
        key: "c:\\steam\\steamapps\\common\\example",
        name: "Örnek Oyun",
        source: "steam",
        startMs: Date.now() - 4 * HOUR - 14 * 60_000,
        endMs: Date.now() - 2 * HOUR,
        gpuAvg: 78,
        gpuPeak: 97,
        cpuAvg: 34,
        tempPeak: 71,
        lol: null,
      },
    ],
    temperature: true,
  } as GamesState,
  emit: null as ((s: GamesState) => void) | null,
  onCard: null as ((c: GameCard) => void) | null,
};

const mockCopy = (): GamesState => structuredClone(mock.state);

if (import.meta.env.DEV && !inTauri && typeof window !== "undefined") {
  const w = window as unknown as Record<string, unknown>;
  /** Browser preview: a game session starts (`true`) or ends. */
  w.__ksMockGame = (on: boolean) => {
    const s = mock.state;
    if (on) {
      s.session = { key: s.games[0]!.key, name: s.games[0]!.name, source: "steam", startedMs: Date.now() - 42 * 60_000, gpuAvg: 74, tempPeak: 69 };
      mock.onCard?.({ kind: "start", name: s.session.name, steamApp: null, startedMs: s.session.startedMs, endedMs: null, gpuAvg: null, gpuPeak: null, cpuAvg: null, tempPeak: null, todayMs: s.todayMs, lol: null });
    } else if (s.session) {
      const ended = Date.now();
      mock.onCard?.({ kind: "summary", name: s.session.name, steamApp: null, startedMs: s.session.startedMs, endedMs: ended, gpuAvg: 78, gpuPeak: 97, cpuAvg: 34, tempPeak: 71, todayMs: s.todayMs, lol: null });
      s.session = null;
    }
    mock.emit?.(mockCopy());
  };
}

// ------------------------------------------------------------------- ipc

export function getGames(): Promise<GamesState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockCopy());
  return invoke<GamesState>("games_state");
}

export function configureGames(settings: GamesSettings): Promise<GamesState> {
  if (import.meta.env.DEV && !inTauri) {
    mock.state.settings = { ...settings };
    return Promise.resolve(mockCopy());
  }
  return invoke<GamesState>("games_configure", { settings });
}

/** The user's word on a program: a game, not a game, or forget it (`null`). */
export function markGame(path: string, name: string, game: boolean | null): Promise<GamesState> {
  if (import.meta.env.DEV && !inTauri) {
    const s = mock.state;
    s.settings.marks = s.settings.marks.filter((m) => m.path.toLowerCase() !== path.toLowerCase());
    if (game !== null) s.settings.marks.push({ path, name, game });
    s.suggestions = s.suggestions.filter((x) => x.path.toLowerCase() !== path.toLowerCase());
    if (game) s.games.unshift({ key: path.toLowerCase(), name, source: "mine", totalMs: 0, lastMs: null, played: false });
    if (game === false) s.games = s.games.filter((g) => g.key !== path.toLowerCase());
    return Promise.resolve(mockCopy());
  }
  return invoke<GamesState>("games_mark", { path, name, game });
}

export function clearGameHistory(): Promise<GamesState> {
  if (import.meta.env.DEV && !inTauri) {
    Object.assign(mock.state, { recent: [], todayMs: 0, weekMs: 0 });
    mock.state.games = mock.state.games.map((g) => ({ ...g, totalMs: 0, lastMs: null, played: false }));
    return Promise.resolve(mockCopy());
  }
  return invoke<GamesState>("games_clear_history");
}

/** Show the game lights on the keyboard for a moment. */
export function tryGameLights(): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve();
  return invoke("games_try_lights");
}

export function rescanGames(): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve();
  return invoke("games_rescan");
}

/** Steam's picture of the game, if it has one. */
export async function gameCover(appId: number): Promise<Blob | null> {
  if (import.meta.env.DEV && !inTauri) return null;
  try {
    const data = await invoke<ArrayBuffer>("games_cover", { appId });
    return new Blob([data]);
  } catch {
    return null;
  }
}

// ----------------------------------------------------------------- store

interface GamesStore {
  state: GamesState | null;
  /** Drawing a card failed (shown on the page). */
  drawError: string | null;
  load: () => Promise<void>;
  configure: (patch: Partial<GamesSettings>) => Promise<void>;
  mark: (path: string, name: string, game: boolean | null) => Promise<void>;
  clearHistory: () => Promise<void>;
}

export const useGames = create<GamesStore>((set, get) => ({
  state: null,
  drawError: null,
  load: async () => set({ state: await getGames() }),
  configure: async (patch) => {
    const s = get().state;
    const settings = { ...(s?.settings ?? DEFAULT_GAMES), ...patch };
    if (s) set({ state: { ...s, settings } });
    set({ state: await configureGames(settings) });
  },
  mark: async (path, name, game) => set({ state: await markGame(path, name, game) }),
  clearHistory: async () => set({ state: await clearGameHistory() }),
}));

/**
 * Follow the backend for the whole app session: state changes, and the
 * cards it asks for, which `draw` draws and the live screen gets.
 */
export function startGames(draw: (c: GameCard) => Promise<Prepared>): () => void {
  const onCard = async (c: GameCard) => {
    if (!useLive.getState().state?.enabled) return;
    try {
      const card = await draw(c);
      useLive.setState({ state: await submitLive(card, GAME_SOURCE) });
      useGames.setState({ drawError: null });
    } catch (e) {
      console.error(e);
      useGames.setState({ drawError: String(e) });
    }
  };
  useGames.getState().load().catch(console.error);
  if (import.meta.env.DEV && !inTauri) {
    mock.emit = (s) => useGames.setState({ state: s });
    mock.onCard = (c) => void onCard(c);
    return () => {
      mock.emit = null;
      mock.onCard = null;
    };
  }
  const offs = [
    listen<GamesState>("games", (e) => useGames.setState({ state: e.payload })),
    listen<GameCard>("game-card", (e) => void onCard(e.payload)),
  ];
  return () => offs.forEach((p) => p.then((off) => off()).catch(console.error));
}

/** "2 h 14 min", "45 min", "0 min" from words with `{h}` and `{min}`. */
export function playTime(ms: number, words: { hours: string; minutes: string }): string {
  const total = Math.max(0, Math.floor(ms / 60_000));
  const h = Math.floor(total / 60);
  const min = total % 60;
  return h > 0 ? words.hours.replace("{h}", String(h)).replace("{min}", String(min)) : words.minutes.replace("{min}", String(min));
}
