// Game session ("Oyun oturumu"). The backend notices games, keeps the
// session, its readings and the history, and lights the keys
// (app/src-tauri/src/games.rs); this file follows its state, draws the
// cards it asks for and hands them to the live screen. The browser preview
// keeps a small made-up state instead.

import { create } from "zustand";
import type { GameCard } from "../bindings/GameCard";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import ak980Geometry from "../bindings/geometry-ajazz-ak980-pro.json";
import type { GamesSettings } from "../bindings/GamesSettings";
import type { GamesState } from "../bindings/GamesState";
import type { LolScene } from "../bindings/LolScene";
import type { LolSettings } from "../bindings/LolSettings";
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

const GOLD: [number, number, number] = [240, 199, 94];

/** League of Legends lights as they come (dto.rs `LolSettings::default`). */
export const DEFAULT_LOL: LolSettings = {
  enabled: true,
  bars: true,
  death: true,
  events: true,
  layout: "keyboard",
  motion: "rightToLeft",
  healthColor: null,
  mana: true,
  manaColor: [61, 139, 255],
  lights: {
    win: { on: true, anim: "sparkle", color: [52, 199, 89] },
    lose: { on: true, anim: "drain", color: [255, 59, 48] },
    levelUp: { on: true, anim: "wave", color: GOLD },
    kill: { on: true, anim: "flash", color: GOLD },
    ace: { on: true, anim: "ring", color: GOLD },
    dragon: { on: true, anim: "flash", color: [255, 90, 30] },
    baron: { on: true, anim: "pulse", color: [155, 93, 229] },
    herald: { on: true, anim: "wave", color: [196, 120, 255] },
  },
  dragonKindColor: true,
  teamOnly: true,
};

export const DEFAULT_GAMES: GamesSettings = {
  enabled: false,
  launchers: true,
  startCard: true,
  summaryCard: true,
  lights: "own",
  lightsProfile: null,
  keys: GAME_KEYS_DEFAULT,
  color: [255, 138, 61],
  rest: "dim",
  brightness: 4,
  quiet: false,
  quietReminders: true,
  quietGames: [],
  marks: [],
  lol: DEFAULT_LOL,
};

/** Sessions shorter than this get no summary card (games.rs). */
export const SUMMARY_MIN_MS = 5 * 60_000;

// ------------------------------------------------------------------ mock

const HOUR = 3_600_000;
const mock = {
  state: {
    rev: 0,
    settings: { ...DEFAULT_GAMES },
    session: null,
    games: [
      { key: "c:\\steam\\steamapps\\common\\example", name: "Ember Isles", source: "steam", totalMs: 31 * HOUR, lastMs: Date.now() - 2 * HOUR, played: true, exe: null },
      { key: "c:\\riot games\\league of legends\\game\\league of legends.exe", name: "League of Legends", source: "riot", totalMs: 12 * HOUR, lastMs: Date.now() - 26 * HOUR, played: true, exe: null },
      { key: "d:\\games\\racer\\racer.exe", name: "Skyline Rally", source: "mine", totalMs: 4 * HOUR, lastMs: Date.now() - 5 * 24 * HOUR, played: true, exe: null },
      { key: "c:\\riot games\\valorant\\live\\shootergame\\binaries\\win64\\valorant-win64-shipping.exe", name: "VALORANT", source: "riot", totalMs: 0, lastMs: null, played: false, exe: null },
    ],
    suggestions: [{ path: "D:\\Emulators\\emulator.exe", name: "emulator" }],
    todayMs: 2 * HOUR + 2 * 60_000,
    weekMs: 9 * HOUR + 40 * 60_000,
    recent: [
      {
        key: "c:\\steam\\steamapps\\common\\example",
        name: "Ember Isles",
        source: "steam",
        startMs: Date.now() - 4 * HOUR - 14 * 60_000,
        endMs: Date.now() - 2 * HOUR,
        gpuAvg: 78,
        gpuPeak: 97,
        cpuAvg: 34,
        cpuPeak: 81,
        tempPeak: 71,
        lol: null,
      },
      {
        key: "c:\\riot games\\league of legends\\game\\league of legends.exe",
        name: "League of Legends",
        source: "riot",
        startMs: Date.now() - 26 * HOUR - 33 * 60_000,
        endMs: Date.now() - 26 * HOUR,
        gpuAvg: 61,
        gpuPeak: 88,
        cpuAvg: 27,
        cpuPeak: 64,
        tempPeak: 66,
        lol: { win: true, kills: 8, deaths: 3, assists: 12, cs: 214, champion: "Ahri", mode: "CLASSIC" },
      },
    ],
    temperature: true,
    programs: [
      { path: "C:\\Program Files\\Paint.NET\\paintdotnet.exe", name: "paintdotnet" },
      { path: "D:\\Games\\Retro\\retro.exe", name: "retro" },
    ],
  } as GamesState,
  emit: null as ((s: GamesState) => void) | null,
  onCard: null as ((c: GameCard) => void) | null,
};

const mockCopy = (): GamesState => structuredClone(mock.state);

/** Browser preview: health and mana drawn as lol.rs `frame` draws them on
 *  the AK980 PRO (the still scene only; death and events play nothing). */
function mockLolPreview(s: GamesSettings, scene: LolScene): LolFrame[] {
  if (scene.kind !== "bars") return [];
  type Rgb = [number, number, number];
  const g = ak980Geometry as GeometryInfo;
  const l = s.lol;
  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  const scale = (c: Rgb, k: number) => c.map((v) => Math.round(v * clamp(k))) as Rgb;
  const mix = (a: Rgb, b: Rgb, k: number) => a.map((v, i) => Math.round(v * (1 - clamp(k)) + b[i]! * clamp(k))) as Rgb;
  const rest: Rgb = s.rest === "dark" ? [0, 0, 0] : scale(s.color, 0.12);
  const frame = new Map(g.keys.map((k) => [k.id, s.lights === "keys" && s.keys.includes(k.id) ? s.color : rest]));
  const bar = (ids: string[], share: number, c: Rgb) =>
    ids.forEach((id, i) => frame.set(id, scale(c, Math.max(clamp(share * ids.length - i), 0.06))));
  const health = clamp(scene.health);
  const hp: Rgb = l.healthColor ?? (health > 0.6 ? [52, 199, 89] : health > 0.3 ? [255, 176, 32] : [255, 59, 48]);
  if (l.layout === "keyboard") {
    for (const k of g.keys) {
      const x = clamp((k.x + k.w / 2) / g.width);
      const y = clamp((k.y + k.h / 2) / g.height);
      const order = { rightToLeft: x, leftToRight: 1 - x, topToBottom: 1 - y, toCenter: Math.abs(x - 0.5) * 2, fade: 0 }[l.motion];
      const edge = health * 1.08 - 0.04;
      const lit = l.motion === "fade" ? health : clamp((edge - order) / 0.04 + 0.5);
      frame.set(k.id, mix(scale(hp, 0.06), hp, lit));
    }
  } else {
    bar(["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"], health, hp);
  }
  if (l.mana) bar(["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"], clamp(scene.mana), l.manaColor);
  return [g.keys.map((k) => [k.led, frame.get(k.id)!] as [number, Rgb])];
}

if (import.meta.env.DEV && !inTauri && typeof window !== "undefined") {
  const w = window as unknown as Record<string, unknown>;
  /** Browser preview: a game session starts (`true`) or ends. */
  w.__ksMockGame = (on: boolean) => {
    const s = mock.state;
    if (on) {
      s.session = {
        key: s.games[0]!.key,
        name: s.games[0]!.name,
        source: "steam",
        startedMs: Date.now() - 42 * 60_000,
        gpuAvg: 74,
        gpuPeak: 96,
        cpuAvg: 31,
        cpuPeak: 77,
        tempPeak: 69,
        quiet: s.settings.quiet || s.settings.quietGames.includes(s.games[0]!.key),
        card: "shown",
      };
      mock.onCard?.({ kind: "start", name: s.session.name, steamApp: null, exe: null, startedMs: s.session.startedMs, endedMs: null, gpuAvg: null, gpuPeak: null, cpuAvg: null, cpuPeak: null, tempPeak: null, todayMs: s.todayMs, lol: null });
    } else if (s.session) {
      const ended = Date.now();
      mock.onCard?.({ kind: "summary", name: s.session.name, steamApp: null, exe: null, startedMs: s.session.startedMs, endedMs: ended, gpuAvg: 78, gpuPeak: 97, cpuAvg: 34, cpuPeak: 81, tempPeak: 71, todayMs: s.todayMs, lol: null });
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
    if (game) s.games.unshift({ key: path.toLowerCase(), name, source: "mine", totalMs: 0, lastMs: null, played: false, exe: path });
    if (game === false) s.games = s.games.filter((g) => g.key !== path.toLowerCase());
    return Promise.resolve(mockCopy());
  }
  return invoke<GamesState>("games_mark", { path, name, game });
}

export function clearGameHistory(): Promise<GamesState> {
  if (import.meta.env.DEV && !inTauri) {
    Object.assign(mock.state, { recent: [], todayMs: 0, weekMs: 0 });
    mock.state.games = mock.state.games.map((g) => ({ ...g, totalMs: 0, lastMs: null, played: false, exe: null }));
    return Promise.resolve(mockCopy());
  }
  return invoke<GamesState>("games_clear_history");
}

/** One frame of the keys: LED and colour. */
export type LolFrame = [number, [number, number, number]][];

/** Time between the frames of `lolPreview` (lol.rs `PREVIEW_STEP_MS`). */
export const LOL_STEP_MS = 40;

/** Every frame of a League of Legends scene, drawn by the backend's own
 *  light code, for the keyboard model `modelId`. */
export function lolPreview(modelId: string, settings: GamesSettings, scene: LolScene): Promise<LolFrame[]> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockLolPreview(settings, scene));
  return invoke<LolFrame[]>("games_lol_preview", { modelId, settings, scene });
}

/** Play a League of Legends scene on the keyboard for a moment. */
export function tryLol(scene: LolScene): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve();
  return invoke("games_try_lol", { scene });
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

/** Take a state from the backend unless a newer one is here already (a
 *  reply and an event can cross). */
function accept(next: GamesState) {
  const cur = useGames.getState().state;
  if (!cur || next.rev >= cur.rev || cur.rev - next.rev > 1_000_000) useGames.setState({ state: next });
}

export const useGames = create<GamesStore>((set, get) => ({
  state: null,
  drawError: null,
  load: async () => accept(await getGames()),
  configure: async (patch) => {
    const s = get().state;
    const settings = { ...(s?.settings ?? DEFAULT_GAMES), ...patch };
    if (s) set({ state: { ...s, settings } });
    accept(await configureGames(settings));
  },
  mark: async (path, name, game) => accept(await markGame(path, name, game)),
  clearHistory: async () => accept(await clearGameHistory()),
}));

/**
 * Follow the backend for the whole app session: state changes, and the
 * cards it asks for, which `draw` draws and the live screen gets.
 */
export function startGames(draw: (c: GameCard) => Promise<Prepared>): () => void {
  // Cards are drawn one after another; one that a newer card overtook
  // while it was drawn isn't sent.
  let latest = 0;
  let queue: Promise<void> = Promise.resolve();
  const onCard = (c: GameCard) => {
    const mine = ++latest;
    queue = queue.then(() => drawAndSend(c, mine));
  };
  const drawAndSend = async (c: GameCard, mine: number) => {
    if (!useLive.getState().state?.enabled) return;
    try {
      const card = await draw(c);
      if (mine !== latest) return;
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
    mock.onCard = (c) => onCard(c);
    return () => {
      mock.emit = null;
      mock.onCard = null;
    };
  }
  const offs = [
    listen<GamesState>("games", (e) => accept(e.payload)),
    listen<GameCard>("game-card", (e) => onCard(e.payload)),
  ];
  return () => offs.forEach((p) => p.then((off) => off()).catch(console.error));
}

export { playTime } from "./liveCards";
