// Typed bridge to the Rust backend. Types come from ts-rs bindings.
// Outside Tauri (plain `pnpm dev` in a browser) a mock backend keeps the
// UI usable for design work.

import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { ThemePreference } from "../bindings/ThemePreference";
import type { UiEnvironment } from "../bindings/UiEnvironment";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { EffectInfo } from "../bindings/EffectInfo";
import type { EffectRequest } from "../bindings/EffectRequest";
import type { LedColor } from "../bindings/LedColor";
import type { MirrorHint } from "../bindings/MirrorHint";
import type { ClockStatus } from "../bindings/ClockStatus";
import type { Calibration } from "../bindings/Calibration";
import type { CalibrationInfo } from "../bindings/CalibrationInfo";
import type { KeyAssignment } from "../bindings/KeyAssignment";
import type { KeymapCatalog } from "../bindings/KeymapCatalog";
import type { KeymapLayer } from "../bindings/KeymapLayer";
import type { KeymapState } from "../bindings/KeymapState";
import type { MacroInfo } from "../bindings/MacroInfo";
import type { MacrosState } from "../bindings/MacrosState";
// Generated from Rust by `cargo test` (dto.rs export_mock_geometry).
import ak980Geometry from "../bindings/geometry-ajazz-ak980-pro.json";
import mockKeymapCatalog from "../bindings/keymap-catalog.json";
import mockCatalog from "../bindings/lighting-catalog.json";

export const inTauri = isTauri();

const EVENT_KEYBOARDS_CHANGED = "keyboards-changed";

export async function getKeyboards(): Promise<KeyboardInfo[]> {
  if (!inTauri) return mockKeyboards();
  return invoke<KeyboardInfo[]>("keyboards");
}

export async function refreshKeyboards(): Promise<void> {
  if (!inTauri) return;
  await invoke("refresh_keyboards");
}

export async function getUiEnvironment(): Promise<UiEnvironment> {
  if (!inTauri) return { appVersion: "0.1.0-dev", mica: false, osBuild: 0 };
  return invoke<UiEnvironment>("ui_environment");
}

export async function setWindowTheme(theme: ThemePreference): Promise<void> {
  if (!inTauri) return;
  await invoke("set_window_theme", { theme });
}

// ------------------------------------------------------- closing the app

/** Tell the backend whether closing should ask about unsaved changes. */
export async function setUnsavedFlag(unsaved: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke("set_unsaved", { unsaved });
}

/**
 * Called when the user closes the window while changes are unsaved. The
 * handler must acknowledge (done here) or the backend closes anyway.
 */
export async function onCloseRequested(cb: () => void): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  return listen("close-requested", () => {
    invoke("ack_close_request").catch(console.error);
    cb();
  });
}

/** Close the window without asking again (after save or discard). */
export async function closeWindow(): Promise<void> {
  if (!inTauri) return;
  await invoke("close_window");
}

export async function onKeyboardsChanged(cb: (k: KeyboardInfo[]) => void): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  return listen<KeyboardInfo[]>(EVENT_KEYBOARDS_CHANGED, (e) => cb(e.payload));
}

// ---------------------------------------------------------------- lighting

export async function getLightingCatalog(): Promise<EffectInfo[]> {
  if (!inTauri) return mockCatalog as EffectInfo[];
  return invoke<EffectInfo[]>("lighting_catalog");
}

/** Store a firmware effect on the keyboard (flash write). */
export function applyEffect(request: EffectRequest): Promise<void> {
  if (!inTauri) return mockDelay(300);
  return serial(() => invoke<void>("apply_effect", { request }));
}

/** Store per-key colours on the keyboard (flash write). */
export function applyPerKey(colors: LedColor[], brightness: number): Promise<void> {
  if (!inTauri) return mockDelay(900);
  return serial(() => invoke<void>("apply_per_key", { colors, brightness }));
}

// Tauri dispatches IPC calls concurrently, so two quick calls can arrive in
// the wrong order (seen 2026-11-10: mirror on → off → on arrived as
// on, on, off). Stateful commands therefore go through one serial queue.
let queue: Promise<unknown> = Promise.resolve();
function serial<T>(task: () => Promise<T>): Promise<T> {
  const run = queue.then(task, task);
  queue = run.catch(() => undefined);
  return run;
}

// Live frames: only the newest pending frame is sent.
let pendingFrame: { colors: LedColor[] | null; brightness: number } | null = null;
let frameInFlight = false;

/**
 * RAM-only live frame in screen colours; `null` stops the stream and the
 * keyboard reverts. `brightness` (0–5) dims after calibration with the
 * firmware's own brightness scale, so the preview matches the saved design.
 */
export function setLiveFrame(colors: LedColor[] | null, brightness = 5): Promise<void> {
  if (!inTauri) return Promise.resolve();
  pendingFrame = { colors, brightness };
  if (frameInFlight) return Promise.resolve();
  frameInFlight = true;
  return serial(async () => {
    try {
      while (pendingFrame) {
        const next = pendingFrame;
        pendingFrame = null;
        await invoke("set_live_frame", { colors: next.colors, brightness: next.brightness });
      }
    } finally {
      frameInFlight = false;
    }
  });
}

/**
 * Ask the backend to mirror the colours shown on the keyboard. `hint` is
 * what the app last saved there; it resolves the read-back's red channel.
 */
export function setMirror(enabled: boolean, hint: MirrorHint | null = null): Promise<void> {
  if (!inTauri) return Promise.resolve();
  return serial(() => invoke<void>("set_mirror", { enabled, hint }));
}

export async function onMirror(cb: (colors: LedColor[]) => void): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  return listen<LedColor[]>("lighting-mirror", (e) => cb(e.payload));
}

// Browser-only stand-in for the keyboard clock state.
let mockClock: ClockStatus = { autoSync: true, lastSyncMs: Date.now() - 3 * 60_000 };

/** Keyboard clock sync state. */
export function getClockStatus(): Promise<ClockStatus> {
  if (!inTauri) return Promise.resolve(mockClock);
  return invoke<ClockStatus>("clock_status");
}

/** Set the keyboard clock to the PC's local time now (USB only). */
export async function syncClock(): Promise<ClockStatus> {
  if (!inTauri) {
    await mockDelay(300);
    return (mockClock = { ...mockClock, lastSyncMs: Date.now() });
  }
  return serial(() => invoke<ClockStatus>("sync_clock"));
}

/** Turn automatic clock sync (on USB connect, at most once a day) on or off. */
export function setClockAuto(enabled: boolean): Promise<ClockStatus> {
  if (!inTauri) return Promise.resolve((mockClock = { ...mockClock, autoSync: enabled }));
  return serial(() => invoke<ClockStatus>("set_clock_auto", { enabled }));
}

/** Clock state changes, including automatic syncs in the background. */
export async function onClockChanged(cb: (status: ClockStatus) => void): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  return listen<ClockStatus>("clock-changed", (e) => cb(e.payload));
}

// ------------------------------------------------------------------ keymap

export async function getKeymapCatalog(): Promise<KeymapCatalog> {
  if (!inTauri) return mockKeymapCatalog as KeymapCatalog;
  return invoke<KeymapCatalog>("keymap_catalog");
}

// Browser-only stand-in for the app's keymap record.
let mockKeymap: KeymapState = {
  base: { assignments: [], writtenMs: null },
  fn: { assignments: [], writtenMs: null },
};

/**
 * What the app last wrote to this model's keymap. The keyboard cannot
 * report its keymap; layers never written are the factory default.
 */
export function getKeymapState(modelId: string): Promise<KeymapState> {
  if (!inTauri) return Promise.resolve(mockKeymap);
  return invoke<KeymapState>("keymap_state", { modelId });
}

/**
 * Write one whole keymap layer (flash write); keys not listed go back to
 * the default. Resolves with the app's updated record.
 */
export async function applyKeymap(layer: KeymapLayer, assignments: KeyAssignment[]): Promise<KeymapState> {
  if (!inTauri) {
    await mockDelay(700);
    mockKeymap = { ...mockKeymap, [layer]: { assignments, writtenMs: Date.now() } };
    return mockKeymap;
  }
  return serial(() => invoke<KeymapState>("apply_keymap", { layer, assignments }));
}

// ------------------------------------------------------------------ macros

// Browser-only stand-in for the app's macro record.
let mockMacros: MacrosState = {
  slots: [],
  writtenMs: null,
  budget: 0xa8c,
  maxSlots: 100,
  minDelayMs: 10,
  maxName: 40,
};

/** What the app last wrote to this model's macros, with the keyboard's limits. */
export function getMacrosState(modelId: string): Promise<MacrosState> {
  if (!inTauri) return Promise.resolve(mockMacros);
  return invoke<MacrosState>("macros_state", { modelId });
}

/**
 * Write every macro (flash write); slot `i` is macro `i`, `null` leaves a
 * slot unused. Resolves with the app's updated record.
 */
export async function applyMacros(slots: (MacroInfo | null)[]): Promise<MacrosState> {
  if (!inTauri) {
    await mockDelay(600);
    while (slots.length && slots[slots.length - 1] === null) slots = slots.slice(0, -1);
    mockMacros = { ...mockMacros, slots, writtenMs: Date.now() };
    return mockMacros;
  }
  return serial(() => invoke<MacrosState>("apply_macros", { slots }));
}

// ------------------------------------------------------------- calibration

const MOCK_CALIBRATION: CalibrationInfo = {
  current: { gamma: [2.2, 2.2, 2.2], gain: [1, 1, 1] },
  factory: { gamma: [2.2, 2.2, 2.2], gain: [1, 1, 1] },
  gammaMin: 1,
  gammaMax: 3,
};

export async function getCalibration(): Promise<CalibrationInfo> {
  if (!inTauri) return MOCK_CALIBRATION;
  return invoke<CalibrationInfo>("calibration");
}

// Previews while a slider moves: only the newest value is sent.
let pendingCalibration: Calibration | null = null;
let calibrationInFlight = false;

/**
 * Use `calibration` at once. With `persist` it is also saved to disk and
 * the promise reports whether saving worked; previews are coalesced.
 */
export function setCalibration(calibration: Calibration, persist: boolean): Promise<void> {
  if (!inTauri) return persist ? mockDelay(120) : Promise.resolve();
  if (persist) return serial(() => invoke<void>("set_calibration", { calibration, persist: true }));
  pendingCalibration = calibration;
  if (calibrationInFlight) return Promise.resolve();
  calibrationInFlight = true;
  return serial(async () => {
    try {
      while (pendingCalibration) {
        const next = pendingCalibration;
        pendingCalibration = null;
        await invoke("set_calibration", { calibration: next, persist: false });
      }
    } finally {
      calibrationInFlight = false;
    }
  });
}

const mockDelay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Browser-only stand-in mirroring what the hardware reported on 2026-11-10. */
function mockKeyboards(): KeyboardInfo[] {
  const mode = new URLSearchParams(location.search).get("mock") ?? "bluetooth";
  if (mode === "none") return [];
  const usb = mode === "usb";
  return [
    {
      key: `ajazz-ak980-pro:${usb ? "Usb" : "Bluetooth"}`,
      modelId: "ajazz-ak980-pro",
      modelName: "AJAZZ AK980 PRO",
      vendor: "AJAZZ",
      layout: "98",
      keyCount: 97,
      hasKnob: true,
      hasSideLight: true,
      screen: { width: 240, height: 135, verified: false },
      connection: usb ? "usb" : "bluetooth",
      product: usb ? "SONiX AK980 PRO" : null,
      features: usb
        ? ["lighting", "per-key-rgb", "keymap", "macros", "options", "screen", "clock"]
        : ["battery"],
      modes: [
        {
          connection: "usb",
          features: ["lighting", "per-key-rgb", "keymap", "macros", "options", "screen", "clock"],
          verified: true,
        },
        { connection: "dongle", features: ["lighting", "clock", "battery"], verified: false },
        { connection: "bluetooth", features: ["battery"], verified: true },
      ],
      hasCommandChannel: usb,
      hasDisplayChannel: usb,
      bleName: usb ? null : "AK980 PRO-1",
      bleAddress: usb ? null : "AA:BB:CC:DD:EE:FF",
      battery: usb ? null : { percent: 94, readAtMs: Date.now() - 40_000 },
      geometry: ak980Geometry as GeometryInfo,
    },
  ];
}
