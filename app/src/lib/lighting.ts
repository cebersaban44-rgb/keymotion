import { create } from "zustand";
import type { EffectRequest } from "../bindings/EffectRequest";
import type { LedColor } from "../bindings/LedColor";
import type { MessageKey } from "./i18n";
import { applyEffect, applyPerKey, lightingLoad, lightingSave } from "./ipc";

export type LightingTab = "effects" | "per-key";
/** Per-key tools: paint what you touch, or select keys and then colour them. */
export type PaintTool = "brush" | "select";
/** The lighting setup with edits not yet stored on the keyboard (last edited wins). */
export type LightingDraft = "effect" | "per-key" | null;

/** led index → "#rrggbb" */
export type ColorMap = Record<number, string>;

export interface LightingState {
  tab: LightingTab;
  effect: EffectRequest;
  /** Last effect successfully stored on the keyboard. */
  appliedEffect: EffectRequest | null;
  perKey: ColorMap;
  perKeyBrightness: number;
  /**
   * Per-key design last stored on the keyboard by this app, or `null` when
   * the keyboard is not showing one (nothing saved yet, or an effect was
   * applied since).
   */
  savedPerKey: { colors: ColorMap; brightness: number } | null;
  brush: string;
  tool: PaintTool;
  livePreview: boolean;
  /**
   * Which setup was edited since the last store. The keyboard holds one
   * lighting setup at a time, so only the most recently edited one counts.
   */
  draft: LightingDraft;
  setTab: (t: LightingTab) => void;
  setTool: (t: PaintTool) => void;
  setEffect: (patch: Partial<EffectRequest>) => void;
  markApplied: (e: EffectRequest) => void;
  markPerKeySaved: (colors: ColorMap, brightness: number) => void;
  /** Back to the effect last stored on the keyboard. */
  revertEffect: () => void;
  revertPerKey: () => void;
  paint: (leds: number[], color: string | null) => void;
  setPerKey: (m: ColorMap) => void;
  setPerKeyBrightness: (b: number) => void;
  setBrush: (c: string) => void;
  setLivePreview: (on: boolean) => void;
  /** Store the effect being edited on the keyboard (flash write). */
  applyEffectDraft: () => Promise<void>;
  /** Store the per-key design on the keyboard (flash write). */
  savePerKeyDraft: () => Promise<void>;
  /** Drop unsaved edits: back to what the keyboard stores, where known. */
  discardDraft: () => void;
}

/**
 * Where the state lived before it moved to the backend (`lighting.json`,
 * lighting.rs). `initLighting` moves it over once and removes it.
 */
const LEGACY_KEY = "ks.lighting.v1";

const defaults = {
  tab: "effects" as LightingTab,
  effect: { mode: 11, color: "#ff8a3d", rainbow: true, brightness: 5, speed: 3, direction: 0 } as EffectRequest,
  appliedEffect: null as EffectRequest | null,
  perKey: {} as ColorMap,
  perKeyBrightness: 5,
  savedPerKey: null as { colors: ColorMap; brightness: number } | null,
  brush: "#ff8a3d",
  tool: "brush" as PaintTool,
  livePreview: true,
  draft: null as LightingDraft,
};

type Persisted = typeof defaults;

const HEX = /^#[0-9a-f]{6}$/i;

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const isByte = (v: unknown, max = 255): v is number => Number.isInteger(v) && (v as number) >= 0 && (v as number) <= max;
const isHex = (v: unknown): v is string => typeof v === "string" && HEX.test(v);

function cleanEffect(v: unknown): EffectRequest | null {
  if (!isObject(v)) return null;
  const { mode, color, rainbow, brightness, speed, direction } = v;
  if (!isByte(mode) || !isHex(color) || typeof rainbow !== "boolean") return null;
  if (!isByte(brightness, 5) || !isByte(speed) || !isByte(direction)) return null;
  return { mode, color, rainbow, brightness, speed, direction };
}

function cleanColors(v: unknown): ColorMap {
  const out: ColorMap = {};
  if (!isObject(v)) return out;
  for (const [led, hex] of Object.entries(v)) {
    const n = Number(led);
    if (isByte(n) && isHex(hex)) out[n] = hex;
  }
  return out;
}

/**
 * Saved lighting state from any source (the backend, the old local copy, a
 * backup): each field checked, anything odd back to its default.
 */
function sanitize(raw: unknown): Persisted {
  const s = isObject(raw) ? raw : {};
  const saved = isObject(s.savedPerKey) ? s.savedPerKey : null;
  const state: Persisted = {
    tab: s.tab === "per-key" ? "per-key" : "effects",
    effect: cleanEffect(s.effect) ?? defaults.effect,
    appliedEffect: cleanEffect(s.appliedEffect),
    perKey: cleanColors(s.perKey),
    perKeyBrightness: isByte(s.perKeyBrightness, 5) ? s.perKeyBrightness : defaults.perKeyBrightness,
    savedPerKey: saved
      ? { colors: cleanColors(saved.colors), brightness: isByte(saved.brightness, 5) ? saved.brightness : 5 }
      : null,
    brush: isHex(s.brush) ? s.brush : defaults.brush,
    tool: s.tool === "select" ? "select" : "brush",
    livePreview: typeof s.livePreview === "boolean" ? s.livePreview : defaults.livePreview,
    draft: s.draft === "effect" || s.draft === "per-key" ? s.draft : null,
  };
  // State saved before drafts were tracked: infer pending edits once, so
  // changes left unsaved then are still flagged.
  if (!("draft" in s)) state.draft = inferDraft(state);
  return state;
}

function inferDraft(s: Persisted): LightingDraft {
  const saved = s.savedPerKey;
  if (saved && (saved.brightness !== s.perKeyBrightness || !sameColors(saved.colors, s.perKey))) return "per-key";
  if (s.appliedEffect && !sameEffect(s.effect, s.appliedEffect)) return "effect";
  return null;
}

function readLegacy(): Persisted | null {
  try {
    const raw = localStorage.getItem(LEGACY_KEY);
    return raw ? sanitize(JSON.parse(raw)) : null;
  } catch {
    return null;
  }
}

function removeLegacy() {
  try {
    localStorage.removeItem(LEGACY_KEY);
  } catch {
    /* nothing to clean up */
  }
}

// Saving: the newest state goes to the backend, one write at a time and at
// most every SAVE_MS (painting changes the state on every key touched).
const SAVE_MS = 150;
let ready = false;
let pending: Persisted | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;
let writing = false;

function persist(s: Persisted) {
  // Before `initLighting` the store holds defaults, not the saved state.
  if (!ready) return;
  pending = s;
  if (!timer && !writing) timer = setTimeout(flush, SAVE_MS);
}

async function flush() {
  timer = null;
  if (writing || !pending) return;
  const next = pending;
  pending = null;
  writing = true;
  try {
    await lightingSave(next);
  } catch (e) {
    console.error(e);
  } finally {
    writing = false;
    if (pending && !timer) timer = setTimeout(flush, SAVE_MS);
  }
}

/**
 * Load the saved state from the backend before the first render. The first
 * time, the copy the app kept in local storage before is moved over (and
 * removed once the backend has it). Safe to call again (after an import).
 */
export async function initLighting(): Promise<void> {
  // A write still waiting would put back what is being replaced.
  pending = null;
  if (timer) clearTimeout(timer);
  timer = null;
  let state: Persisted | null = null;
  try {
    const saved = await lightingLoad();
    if (saved) {
      state = sanitize(saved);
      removeLegacy();
    } else {
      state = readLegacy();
      if (state) {
        await lightingSave(state);
        removeLegacy();
      }
    }
  } catch (e) {
    console.error(e);
    state ??= readLegacy();
  }
  if (state) useLighting.setState(state);
  ready = true;
}

export const useLighting = create<LightingState>((set, get) => {
  const save = () => {
    const { tab, effect, appliedEffect, perKey, perKeyBrightness, savedPerKey, brush, tool, livePreview, draft } = get();
    persist({ tab, effect, appliedEffect, perKey, perKeyBrightness, savedPerKey, brush, tool, livePreview, draft });
  };
  const update = (patch: Partial<LightingState>) => {
    set(patch);
    save();
  };
  return {
    ...defaults,
    setTab: (tab) => update({ tab }),
    setTool: (tool) => update({ tool }),
    setEffect: (patch) => update({ effect: { ...get().effect, ...patch }, draft: "effect" }),
    // The keyboard shows one lighting setup at a time: storing an effect
    // replaces the per-key design and vice versa.
    markApplied: (appliedEffect) => update({ appliedEffect, savedPerKey: null, draft: null }),
    markPerKeySaved: (colors, brightness) =>
      update({ savedPerKey: { colors: { ...colors }, brightness }, appliedEffect: null, draft: null }),
    revertEffect: () => {
      const { appliedEffect, draft } = get();
      if (appliedEffect) update({ effect: { ...appliedEffect }, draft: draft === "effect" ? null : draft });
    },
    revertPerKey: () => {
      const { savedPerKey: saved, draft } = get();
      if (saved)
        update({ perKey: { ...saved.colors }, perKeyBrightness: saved.brightness, draft: draft === "per-key" ? null : draft });
    },
    paint: (leds, color) => {
      const next = { ...get().perKey };
      for (const l of leds) {
        if (color) next[l] = color;
        else delete next[l];
      }
      update({ perKey: next, draft: "per-key" });
    },
    setPerKey: (perKey) => update({ perKey, draft: "per-key" }),
    setPerKeyBrightness: (perKeyBrightness) => update({ perKeyBrightness, draft: "per-key" }),
    setBrush: (brush) => update({ brush }),
    setLivePreview: (livePreview) => update({ livePreview }),
    applyEffectDraft: async () => {
      const effect = { ...get().effect };
      await applyEffect(effect);
      get().markApplied(effect);
    },
    savePerKeyDraft: async () => {
      const { perKey, perKeyBrightness } = get();
      const colors = { ...perKey };
      await applyPerKey(toLedColors(colors), perKeyBrightness);
      get().markPerKeySaved(colors, perKeyBrightness);
    },
    discardDraft: () => {
      const { draft, appliedEffect, savedPerKey } = get();
      if (draft === "effect" && appliedEffect) update({ effect: { ...appliedEffect }, draft: null });
      else if (draft === "per-key" && savedPerKey)
        update({ perKey: { ...savedPerKey.colors }, perKeyBrightness: savedPerKey.brightness, draft: null });
      else update({ draft: null });
    },
  };
});

export function hexToRgb(hex: string): [number, number, number] {
  const v = parseInt(hex.replace("#", ""), 16);
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

export function rgbToHex(r: number, g: number, b: number): string {
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}

export function sameEffect(a: EffectRequest, b: EffectRequest): boolean {
  return (
    a.mode === b.mode &&
    a.color.toLowerCase() === b.color.toLowerCase() &&
    a.rainbow === b.rainbow &&
    a.brightness === b.brightness &&
    a.speed === b.speed &&
    a.direction === b.direction
  );
}

/**
 * Which lighting edits are not on the keyboard yet, or `null`. Edits that
 * end up equal to what is stored do not count.
 */
export function lightingUnsaved(
  s: Pick<LightingState, "draft" | "effect" | "appliedEffect" | "perKey" | "perKeyBrightness" | "savedPerKey">,
): LightingDraft {
  if (s.draft === "effect") return s.appliedEffect && sameEffect(s.effect, s.appliedEffect) ? null : "effect";
  if (s.draft === "per-key") {
    const saved = s.savedPerKey;
    const same = saved
      ? saved.brightness === s.perKeyBrightness && sameColors(saved.colors, s.perKey)
      : Object.keys(s.perKey).length === 0;
    return same ? null : "per-key";
  }
  return null;
}

/** Same colours on the same keys (case-insensitive hex). */
export function sameColors(a: ColorMap, b: ColorMap): boolean {
  const ka = Object.keys(a);
  if (ka.length !== Object.keys(b).length) return false;
  return ka.every((k) => b[Number(k)]?.toLowerCase() === a[Number(k)]?.toLowerCase());
}

export function toLedColors(map: ColorMap): LedColor[] {
  return Object.entries(map).map(([led, hex]) => {
    const [r, g, b] = hexToRgb(hex);
    return { led: Number(led), r, g, b };
  });
}

/** Effects that react to key presses rather than animating on their own. */
export const REACTIVE_EFFECTS = new Set([2, 3, 13, 14, 15]);

/**
 * The one colour palette of the app's light pickers (effects, per-key,
 * focus timer, gauges) and of the calibration pattern. Names are read out
 * instead of the colour code.
 */
export const PALETTE: readonly { hex: string; name: MessageKey }[] = [
  { hex: "#ffffff", name: "color.white" },
  { hex: "#ff3b30", name: "color.red" },
  { hex: "#ff8a3d", name: "color.orange" },
  { hex: "#ffcc00", name: "color.yellow" },
  { hex: "#a8e43a", name: "color.lime" },
  { hex: "#34c759", name: "color.green" },
  { hex: "#2ee6d6", name: "color.cyan" },
  { hex: "#38b6ff", name: "color.sky" },
  { hex: "#3a5bff", name: "color.blue" },
  { hex: "#8b5cf6", name: "color.violet" },
  { hex: "#e040fb", name: "color.magenta" },
  { hex: "#ff4fa3", name: "color.pink" },
];

export const SWATCHES = PALETTE.map((p) => p.hex);
