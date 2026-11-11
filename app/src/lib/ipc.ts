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
import type { SettingsReset } from "../bindings/SettingsReset";
import type { AppFolder } from "../bindings/AppFolder";
import type { BackupExported } from "../bindings/BackupExported";
import type { BackupImported } from "../bindings/BackupImported";
import type { BackupSection } from "../bindings/BackupSection";
import type { BackupSummary } from "../bindings/BackupSummary";
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
  return flashWrite(() => (inTauri ? serial(() => invoke<void>("apply_effect", { request })) : mockDelay(300)));
}

/** Store per-key colours on the keyboard (flash write). */
export function applyPerKey(colors: LedColor[], brightness: number): Promise<void> {
  return flashWrite(() =>
    inTauri ? serial(() => invoke<void>("apply_per_key", { colors, brightness })) : mockDelay(900),
  );
}

// ----------------------------------------------------------- device safety

/**
 * Asks the user whether to write on firmware nobody tested (lib/safety.ts
 * sets it). `fromUser`: the user's own action asked, not the live mode in
 * the background. Resolves `true` to go ahead.
 */
export type FirmwareGate = (version: string, fromUser: boolean) => Promise<boolean>;
let firmwareGate: FirmwareGate = () => Promise.resolve(false);

export function setFirmwareGate(gate: FirmwareGate) {
  firmwareGate = gate;
}

const UNTESTED = /untested keyboard firmware (\S+)/i;

/**
 * Run a write to the keyboard's memory. On untested firmware the backend
 * refuses the first write of a session; the user is asked here, outside
 * the serial queue so live previews keep going, and the write goes again
 * if they continue.
 */
export async function flashWrite<T>(run: () => Promise<T>): Promise<T> {
  try {
    if (!inTauri) mockFirmwareCheck();
    return await run();
  } catch (e) {
    const version = (e instanceof Error ? e.message : String(e)).match(UNTESTED)?.[1];
    if (!version) throw e;
    if (!(await firmwareGate(version, true))) throw new Error("write cancelled by the user", { cause: e });
    return run();
  }
}

/** Allow writes on this untested firmware version until the app closes. */
export async function confirmFirmware(version: string): Promise<void> {
  if (!inTauri) {
    mockConfirmedFirmware = version;
    return;
  }
  await invoke("confirm_firmware", { firmware: version });
}

/** A write was refused for untested firmware (also from the live mode). */
export async function onFirmwareUnverified(cb: (version: string) => void): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  return listen<string>("firmware-unverified", (e) => cb(e.payload));
}

/** Settings files that could not be read and went back to their defaults. */
export async function getSettingsResets(): Promise<SettingsReset[]> {
  if (!inTauri) {
    // `?reset=keymap,macros` shows the notice in the browser.
    const names = new URLSearchParams(location.search).get("reset");
    return names ? names.split(",").map((n) => ({ file: `${n}.json`, backup: `${n}.json.bak-20261111-120000` })) : [];
  }
  return invoke<SettingsReset[]>("settings_resets");
}

/** The notice about the resets was shown; it does not come back. */
export async function dismissSettingsResets(): Promise<void> {
  if (!inTauri) return;
  await invoke("dismiss_settings_resets");
}

// Browser-only: `?fw=1.07` plays a keyboard on untested firmware.
const mockFirmware = () => new URLSearchParams(location.search).get("fw") ?? "1.06";
let mockConfirmedFirmware: string | null = null;
function mockFirmwareCheck() {
  const fw = mockFirmware();
  if (fw !== "1.06" && fw !== mockConfirmedFirmware) throw new Error(`untested keyboard firmware ${fw}`);
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
export function applyKeymap(layer: KeymapLayer, assignments: KeyAssignment[]): Promise<KeymapState> {
  return flashWrite(async () => {
    if (inTauri) return serial(() => invoke<KeymapState>("apply_keymap", { layer, assignments }));
    await mockDelay(700);
    mockKeymap = { ...mockKeymap, [layer]: { assignments, writtenMs: Date.now() } };
    return mockKeymap;
  });
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
export function applyMacros(slots: (MacroInfo | null)[]): Promise<MacrosState> {
  return flashWrite(async () => {
    if (inTauri) return serial(() => invoke<MacrosState>("apply_macros", { slots }));
    await mockDelay(600);
    while (slots.length && slots[slots.length - 1] === null) slots = slots.slice(0, -1);
    mockMacros = { ...mockMacros, slots, writtenMs: Date.now() };
    return mockMacros;
  });
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

// ---------------------------------------------------- data and diagnostics

/** The Lighting page's saved state (lighting.ts owns the fields); `null` when none was saved yet. */
export async function lightingLoad(): Promise<Record<string, unknown> | null> {
  if (!inTauri) {
    try {
      const raw = localStorage.getItem(MOCK_LIGHTING);
      return raw ? (JSON.parse(raw) as Record<string, unknown>) : null;
    } catch {
      return null;
    }
  }
  return invoke<Record<string, unknown> | null>("lighting_state");
}

export async function lightingSave(state: Record<string, unknown>): Promise<void> {
  if (!inTauri) {
    try {
      localStorage.setItem(MOCK_LIGHTING, JSON.stringify(state));
    } catch {
      /* browser preview only */
    }
    return;
  }
  await invoke("lighting_save", { state });
}

const MOCK_LIGHTING = "ks.mock.lighting";

/** Put a UI failure into the app's log (the backend removes addresses). */
export function logUiError(message: string): Promise<void> {
  if (!inTauri) return Promise.resolve();
  return invoke<void>("log_ui_error", { message }).catch(() => undefined);
}

/** The diagnostics report as plain text (versions, keyboards, last log lines). */
export async function getDiagnostics(language: string): Promise<string> {
  if (!inTauri) {
    await mockDelay(150);
    return `Browser preview diagnostics\nUI language: ${language}\nUser agent: ${navigator.userAgent}\n`;
  }
  return invoke<string>("diagnostics_report", { language });
}

/** Open the app's log or data folder in Explorer. */
export async function openFolder(folder: AppFolder): Promise<void> {
  if (!inTauri) return;
  await invoke("open_folder", { folder });
}

/**
 * Write a backup file into the Downloads folder. Calendar addresses go in
 * only with `withAddresses`. In the browser preview the file downloads.
 */
export async function exportBackup(withAddresses: boolean, fileName: string): Promise<BackupExported> {
  if (!inTauri) {
    const lighting = await lightingLoad();
    const backup = { kind: "kb-backup", version: 1, createdMs: Date.now(), appVersion: "0.1.0-dev", lighting };
    const url = URL.createObjectURL(new Blob([JSON.stringify(backup, null, 2)], { type: "application/json" }));
    const a = Object.assign(document.createElement("a"), { href: url, download: `${fileName}.json` });
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return { file: `${fileName}.json`, inDownloads: true };
  }
  return invoke<BackupExported>("backup_export", { withAddresses, fileName });
}

/** Show the last exported file in Explorer. */
export async function revealExport(): Promise<void> {
  if (!inTauri) return;
  await invoke("backup_reveal");
}

// Browser-only stand-in for the backup waiting to be imported.
let mockPending: Record<string, unknown> | null = null;

/** Read and check a backup file; it waits for `importBackup`. Nothing changes yet. */
export async function inspectBackup(bytes: ArrayBuffer): Promise<BackupSummary> {
  if (!inTauri) return mockInspect(bytes);
  return invoke<BackupSummary>("backup_inspect", bytes);
}

/** Restore the chosen parts of the inspected backup. Nothing is written to the keyboard. */
export async function importBackup(sections: BackupSection[]): Promise<BackupImported> {
  if (!inTauri) {
    await mockDelay(300);
    const lighting = mockPending?.lighting;
    if (sections.includes("lighting") && lighting && typeof lighting === "object") {
      const local = (await lightingLoad()) ?? {};
      const tab = (lighting as Record<string, unknown>).tab;
      await lightingSave({
        ...(lighting as Record<string, unknown>),
        appliedEffect: local.appliedEffect ?? null,
        savedPerKey: local.savedPerKey ?? null,
        draft: tab === "per-key" ? "per-key" : "effect",
      });
    }
    mockPending = null;
    return { keymaps: [], macros: [], failed: [], skipped: 0 };
  }
  return invoke<BackupImported>("backup_import", { sections });
}

/** Forget the inspected backup. */
export async function cancelImport(): Promise<void> {
  if (!inTauri) {
    mockPending = null;
    return;
  }
  await invoke("backup_cancel");
}

/** Browser preview: the same checks and counts as backup.rs, roughly. */
async function mockInspect(bytes: ArrayBuffer): Promise<BackupSummary> {
  await mockDelay(200);
  let b: Record<string, unknown>;
  try {
    b = JSON.parse(new TextDecoder().decode(bytes).replace(/^\uFEFF/, "")) as Record<string, unknown>;
  } catch {
    throw new Error("not a backup");
  }
  if (!b || b.kind !== "kb-backup") throw new Error("not a backup");
  if (typeof b.version !== "number") throw new Error("backup damaged");
  if (b.version > 1) throw new Error("backup from a newer version");
  const sections: BackupSummary["sections"] = [];
  const list = (v: unknown, key: string) => {
    const x = v && typeof v === "object" ? (v as Record<string, unknown>)[key] : undefined;
    return Array.isArray(x) ? x.length : 0;
  };
  // Keymaps and macros count only when something was written (as in Rust).
  const models = (v: unknown) =>
    Object.values(((v ?? {}) as { models?: Record<string, Record<string, unknown>> }).models ?? {});
  const written = (l: unknown) => !!l && typeof l === "object" && (l as { writtenMs?: unknown }).writtenMs != null;
  const layers = models(b.keymaps).flatMap((m) => [m.base, m.fn]).filter(written).length;
  const macroModels = models(b.macros);
  if (b.lighting) sections.push({ section: "lighting", count: 0 });
  if (layers > 0) sections.push({ section: "keymap", count: layers });
  if (macroModels.some(written)) {
    const count = macroModels.reduce((n, m) => n + (Array.isArray(m.slots) ? m.slots.filter(Boolean).length : 0), 0);
    sections.push({ section: "macros", count });
  }
  if (list(b.screen, "items")) sections.push({ section: "screen", count: list(b.screen, "items") });
  if (b.calibration) sections.push({ section: "calibration", count: 0 });
  if (b.clock) sections.push({ section: "clock", count: 0 });
  if (b.pomodoro) sections.push({ section: "focus", count: 0 });
  if (b.gauges) sections.push({ section: "gauges", count: 0 });
  if (b.reminders) sections.push({ section: "reminders", count: list(b.reminders, "reminders") });
  if (b.calendar) sections.push({ section: "calendar", count: list(b.calendar, "sources") });
  mockPending = b;
  return {
    createdMs: typeof b.createdMs === "number" ? b.createdMs : 0,
    appVersion: typeof b.appVersion === "string" ? b.appVersion : "",
    sections,
    screenNew: list(b.screen, "items"),
  };
}

const mockDelay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Browser-only stand-in mirroring what the hardware reported on 2026-11-10. */
function mockKeyboards(): KeyboardInfo[] {
  const mode = new URLSearchParams(location.search).get("mock") ?? "bluetooth";
  if (mode === "none") return [];
  const usb = mode === "usb";
  const dongle = mode === "dongle";
  const usbFeatures: KeyboardInfo["features"] = ["lighting", "per-key-rgb", "keymap", "macros", "screen", "clock"];
  return [
    {
      key: `ajazz-ak980-pro:${usb ? "Usb" : dongle ? "Dongle" : "Bluetooth"}`,
      modelId: "ajazz-ak980-pro",
      modelName: "AJAZZ AK980 PRO",
      vendor: "AJAZZ",
      layout: "98",
      keyCount: 97,
      hasKnob: true,
      hasSideLight: true,
      screen: { width: 240, height: 135, verified: false },
      connection: usb ? "usb" : dongle ? "dongle" : "bluetooth",
      product: usb ? "SONiX AK980 PRO" : null,
      firmware: usb ? mockFirmware() : null,
      // What the app drives in each mode (kb_core::app_capabilities).
      features: usb ? usbFeatures : dongle ? [] : ["battery"],
      modes: [
        { connection: "usb", features: usbFeatures, verified: true },
        { connection: "dongle", features: [], verified: false },
        { connection: "bluetooth", features: ["battery"], verified: true },
      ],
      hasCommandChannel: usb || dongle,
      hasDisplayChannel: usb,
      bleName: usb || dongle ? null : "AK980 PRO-1",
      bleAddress: usb || dongle ? null : "AA:BB:CC:DD:EE:FF",
      battery: usb || dongle ? null : { percent: 94, readAtMs: Date.now() - 40_000 },
      geometry: ak980Geometry as GeometryInfo,
    },
  ];
}
