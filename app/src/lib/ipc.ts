// Typed bridge to the Rust backend. Types come from ts-rs bindings.
//
// Browser mock (development only): outside Tauri, `pnpm dev` in a browser
// uses stand-ins for the backend so the UI can be designed and tested
// (`?mock=usb|bluetooth|dongle|none`, `?fw=`, `?reset=`, `?failPart=`,
// `?unsupported`, `?crash`, `mock-close-requested`; lib/screen.ts, live.ts, pomodoro.ts, calendar.ts,
// reminders.ts and gauges.ts have their own). Every mock branch is guarded
// by `import.meta.env.DEV`, written out at the guard itself: Vite replaces
// it with `false` in a production build, so the mock code and its switches
// are not in the installed app. There, outside Tauri, calls reject (below).

import { invoke as tauriInvoke, isTauri, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";
import { listen as tauriListen, type EventCallback, type EventName, type UnlistenFn } from "@tauri-apps/api/event";
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
import type { PartLoaded } from "../bindings/PartLoaded";
import type { ProfileInfo } from "../bindings/ProfileInfo";
import type { BackgroundSettings } from "../bindings/BackgroundSettings";
import type { ProfilePart } from "../bindings/ProfilePart";
import type { ProfileParts } from "../bindings/ProfileParts";
import type { ProfileSaved } from "../bindings/ProfileSaved";
import type { ProfilesState } from "../bindings/ProfilesState";
import type { ScreenItem } from "../bindings/ScreenItem";
import type { TrayTexts } from "../bindings/TrayTexts";
import type { CrashNote } from "../bindings/CrashNote";
import type { IssueKind } from "../bindings/IssueKind";
import type { UnsupportedKeyboard } from "../bindings/UnsupportedKeyboard";
// Generated from Rust by `cargo test` (dto.rs export_mock_geometry).
import ak980Geometry from "../bindings/geometry-ajazz-ak980-pro.json";
import mockKeymapCatalog from "../bindings/keymap-catalog.json";
import mockCatalog from "../bindings/lighting-catalog.json";

export const inTauri = isTauri();

const outsideApp = (what: string) => new Error(`Keymotion's backend is not available: this page runs outside the app (${what})`);

/** Tauri's `invoke`; outside the app it rejects with a clear error. Use this one, not Tauri's. */
export function invoke<T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  if (!inTauri) return Promise.reject(outsideApp(cmd));
  return tauriInvoke<T>(cmd, args, options);
}

/** Tauri's `listen`; outside the app it rejects with a clear error. Use this one, not Tauri's. */
export function listen<T>(event: EventName, handler: EventCallback<T>): Promise<UnlistenFn> {
  if (!inTauri) return Promise.reject(outsideApp(event));
  return tauriListen<T>(event, handler);
}

const EVENT_KEYBOARDS_CHANGED = "keyboards-changed";

export async function getKeyboards(): Promise<KeyboardInfo[]> {
  if (import.meta.env.DEV && !inTauri) return mockKeyboards();
  return invoke<KeyboardInfo[]>("keyboards");
}

export async function refreshKeyboards(): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("refresh_keyboards");
}

export async function getUiEnvironment(): Promise<UiEnvironment> {
  if (import.meta.env.DEV && !inTauri) return { appVersion: "0.1.0-dev", mica: false, osBuild: 0 };
  return invoke<UiEnvironment>("ui_environment");
}

/** Open one of the project's own pages in the browser (the backend opens
 *  nothing else). */
export async function openLink(url: string): Promise<void> {
  if (import.meta.env.DEV && !inTauri) {
    window.open(url, "_blank", "noopener");
    return;
  }
  await invoke("open_link", { url });
}

export async function setWindowTheme(theme: ThemePreference): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("set_window_theme", { theme });
}

// ------------------------------------------------------- closing the app

/** Tell the backend whether closing should ask about unsaved changes. */
export async function setUnsavedFlag(unsaved: boolean): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("set_unsaved", { unsaved });
}

/**
 * Called when the user closes the window while changes are unsaved. The
 * handler must acknowledge (done here) or the backend closes anyway.
 */
export async function onCloseRequested(cb: () => void): Promise<UnlistenFn> {
  if (import.meta.env.DEV && !inTauri) {
    // Browser mock: `dispatchEvent(new Event("mock-close-requested"))`
    // stands in for the window's close button.
    const on = () => cb();
    window.addEventListener("mock-close-requested", on);
    return () => window.removeEventListener("mock-close-requested", on);
  }
  return listen("close-requested", () => {
    invoke("ack_close_request").catch(console.error);
    cb();
  });
}

/** Close the window without asking again (after save or discard). */
export async function closeWindow(): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("close_window");
}

/** The user stayed after "Quit" in the tray asked about unsaved changes:
 *  closing the window hides it again. */
export async function cancelQuit(): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("quit_cancelled");
}

// ------------------------------------------------------- tray and background

let mockBackground: BackgroundSettings = { closeToTray: true, startWithWindows: false, startTurnedOff: false };

/** Keep running in the tray when closed; start with Windows. */
export async function getBackground(): Promise<BackgroundSettings> {
  if (import.meta.env.DEV && !inTauri) return mockBackground;
  return invoke<BackgroundSettings>("background_state");
}

export async function setBackground(settings: BackgroundSettings): Promise<BackgroundSettings> {
  if (import.meta.env.DEV && !inTauri) {
    mockBackground = { ...settings, startTurnedOff: false };
    return mockBackground;
  }
  return invoke<BackgroundSettings>("background_configure", { settings });
}

/** The tray menu's words and the background notice, in the UI language. */
export async function setTrayTexts(texts: TrayTexts): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("tray_texts", { texts });
}

/** Whether the window is on screen (hidden: the app runs in the tray). */
export async function isWindowShown(): Promise<boolean> {
  if (import.meta.env.DEV && !inTauri) return true;
  return invoke<boolean>("window_shown");
}

export async function onWindowShown(cb: (shown: boolean) => void): Promise<UnlistenFn> {
  if (import.meta.env.DEV && !inTauri) return () => {};
  return listen<boolean>("window-shown", (e) => cb(e.payload));
}

/** A profile chosen in the tray menu (its id). */
export async function onTrayProfile(cb: (id: string) => void): Promise<UnlistenFn> {
  if (import.meta.env.DEV && !inTauri) return () => {};
  return listen<string>("tray-profile", (e) => cb(e.payload));
}

export async function onKeyboardsChanged(cb: (k: KeyboardInfo[]) => void): Promise<UnlistenFn> {
  if (import.meta.env.DEV && !inTauri) return () => {};
  return listen<KeyboardInfo[]>(EVENT_KEYBOARDS_CHANGED, (e) => cb(e.payload));
}

// ---------------------------------------------------------------- lighting

export async function getLightingCatalog(): Promise<EffectInfo[]> {
  if (import.meta.env.DEV && !inTauri) return mockCatalog as EffectInfo[];
  return invoke<EffectInfo[]>("lighting_catalog");
}

/** Store a firmware effect on the keyboard (flash write). */
export function applyEffect(request: EffectRequest): Promise<void> {
  return flashWrite(() =>
    import.meta.env.DEV && !inTauri ? mockDelay(300) : serial(() => invoke<void>("apply_effect", { request })),
  );
}

/** Store per-key colours on the keyboard (flash write). */
export function applyPerKey(colors: LedColor[], brightness: number): Promise<void> {
  return flashWrite(() =>
    import.meta.env.DEV && !inTauri ? mockDelay(900) : serial(() => invoke<void>("apply_per_key", { colors, brightness })),
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
    if (import.meta.env.DEV && !inTauri) mockFirmwareCheck();
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
  if (import.meta.env.DEV && !inTauri) {
    mockConfirmedFirmware = version;
    return;
  }
  await invoke("confirm_firmware", { firmware: version });
}

/** A write was refused for untested firmware (also from the live mode). */
export async function onFirmwareUnverified(cb: (version: string) => void): Promise<UnlistenFn> {
  if (import.meta.env.DEV && !inTauri) return () => {};
  return listen<string>("firmware-unverified", (e) => cb(e.payload));
}

/** Settings files that could not be read and went back to their defaults. */
export async function getSettingsResets(): Promise<SettingsReset[]> {
  if (import.meta.env.DEV && !inTauri) {
    // `?reset=keymap,macros` shows the notice in the browser.
    const names = new URLSearchParams(location.search).get("reset");
    return names ? names.split(",").map((n) => ({ file: `${n}.json`, backup: `${n}.json.bak-20261111-120000` })) : [];
  }
  return invoke<SettingsReset[]>("settings_resets");
}

/** The notice about the resets was shown; it does not come back. */
export async function dismissSettingsResets(): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
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
  if (import.meta.env.DEV && !inTauri) return Promise.resolve();
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
  if (import.meta.env.DEV && !inTauri) return Promise.resolve();
  return serial(() => invoke<void>("set_mirror", { enabled, hint }));
}

export async function onMirror(cb: (colors: LedColor[]) => void): Promise<UnlistenFn> {
  if (import.meta.env.DEV && !inTauri) return () => {};
  return listen<LedColor[]>("lighting-mirror", (e) => cb(e.payload));
}

// Browser-only stand-in for the keyboard clock state.
let mockClock: ClockStatus = { autoSync: true, lastSyncMs: Date.now() - 3 * 60_000 };

/** Keyboard clock sync state. */
export function getClockStatus(): Promise<ClockStatus> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockClock);
  return invoke<ClockStatus>("clock_status");
}

/** Set the keyboard clock to the PC's local time now (USB only). */
export async function syncClock(): Promise<ClockStatus> {
  if (import.meta.env.DEV && !inTauri) {
    await mockDelay(300);
    return (mockClock = { ...mockClock, lastSyncMs: Date.now() });
  }
  return serial(() => invoke<ClockStatus>("sync_clock"));
}

/** Turn automatic clock sync (on USB connect, at most once a day) on or off. */
export function setClockAuto(enabled: boolean): Promise<ClockStatus> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve((mockClock = { ...mockClock, autoSync: enabled }));
  return serial(() => invoke<ClockStatus>("set_clock_auto", { enabled }));
}

/** Clock state changes, including automatic syncs in the background. */
export async function onClockChanged(cb: (status: ClockStatus) => void): Promise<UnlistenFn> {
  if (import.meta.env.DEV && !inTauri) return () => {};
  return listen<ClockStatus>("clock-changed", (e) => cb(e.payload));
}

// ------------------------------------------------------------------ keymap

export async function getKeymapCatalog(): Promise<KeymapCatalog> {
  if (import.meta.env.DEV && !inTauri) return mockKeymapCatalog as KeymapCatalog;
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
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockKeymap);
  return invoke<KeymapState>("keymap_state", { modelId });
}

/**
 * Write one whole keymap layer (flash write); keys not listed go back to
 * the default. Resolves with the app's updated record.
 */
export function applyKeymap(layer: KeymapLayer, assignments: KeyAssignment[]): Promise<KeymapState> {
  return flashWrite(async () => {
    if (import.meta.env.DEV && !inTauri) {
      await mockDelay(700);
      mockKeymap = { ...mockKeymap, [layer]: { assignments, writtenMs: Date.now() } };
      return mockKeymap;
    }
    return serial(() => invoke<KeymapState>("apply_keymap", { layer, assignments }));
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
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockMacros);
  return invoke<MacrosState>("macros_state", { modelId });
}

/**
 * Write every macro (flash write); slot `i` is macro `i`, `null` leaves a
 * slot unused. Resolves with the app's updated record.
 */
export function applyMacros(slots: (MacroInfo | null)[]): Promise<MacrosState> {
  return flashWrite(async () => {
    if (import.meta.env.DEV && !inTauri) {
      await mockDelay(600);
      while (slots.length && slots[slots.length - 1] === null) slots = slots.slice(0, -1);
      mockMacros = { ...mockMacros, slots, writtenMs: Date.now() };
      return mockMacros;
    }
    return serial(() => invoke<MacrosState>("apply_macros", { slots }));
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
  if (import.meta.env.DEV && !inTauri) return MOCK_CALIBRATION;
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
  if (import.meta.env.DEV && !inTauri) return persist ? mockDelay(120) : Promise.resolve();
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
  if (import.meta.env.DEV && !inTauri) {
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
  if (import.meta.env.DEV && !inTauri) {
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
  if (import.meta.env.DEV && !inTauri) return Promise.resolve();
  return invoke<void>("log_ui_error", { message }).catch(() => undefined);
}

/** The diagnostics report as plain text (versions, keyboards, last log
 *  lines). A problem report passes what happened (`context`) for the top. */
export async function getDiagnostics(language: string, context?: string): Promise<string> {
  if (import.meta.env.DEV && !inTauri) {
    await mockDelay(150);
    const what = context ? `\nWhat happened:\n  ${context.split("\n").join("\n  ")}\n` : "";
    return `Browser preview diagnostics\nUI language: ${language}${what}\nUser agent: ${navigator.userAgent}\n`;
  }
  return invoke<string>("diagnostics_report", { language, context: context ?? null });
}

// ------------------------------------------------------- reporting problems

const ISSUE_PAGE = "https://github.com/cebersaban44-rgb/keymotion/issues/new";

/** Open a new issue on the project's GitHub page with its form; the title
 *  and (for a keyboard) the model are filled in. */
export async function openIssue(kind: IssueKind, title: string, model?: string): Promise<void> {
  if (import.meta.env.DEV && !inTauri) {
    const q = new URLSearchParams({ template: `${kind}.yml` });
    if (title.trim()) q.set("title", title.trim());
    if (model?.trim()) q.set("model", model.trim());
    window.open(`${ISSUE_PAGE}?${q}`, "_blank", "noopener");
    return;
  }
  await invoke("open_issue", { kind, title, model: model ?? null });
}

/** The newest crash note from an earlier run not offered yet. */
export async function crashPending(): Promise<CrashNote | null> {
  if (import.meta.env.DEV && !inTauri) {
    if (!new URLSearchParams(location.search).has("crash")) return null;
    return {
      name: "crash-2026-11-12-101500.txt",
      text: "Crash note\nVersion: 0.1.0-dev\nTime: 2026-11-12T10:15:00+03:00\nThread: device-actor\nLocation: src\\device.rs:812:9\nMessage: browser preview crash\n",
    };
  }
  return invoke<CrashNote | null>("crash_pending");
}

/** The crash note `name` was offered: it isn't again. */
export async function crashSeen(name: string): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("crash_seen", { name });
}

const MOCK_UNSUPPORTED: UnsupportedKeyboard[] = [{ id: "0C45:8009:usb", name: "SONiX AK820 PRO" }];

/** Keyboards that look like SONiX or AJAZZ ones but aren't supported yet. */
export async function getUnsupportedKeyboards(): Promise<UnsupportedKeyboard[]> {
  if (import.meta.env.DEV && !inTauri) {
    return new URLSearchParams(location.search).has("unsupported") ? MOCK_UNSUPPORTED : [];
  }
  return invoke<UnsupportedKeyboard[]>("unsupported_keyboards");
}

export async function onUnsupportedChanged(cb: (list: UnsupportedKeyboard[]) => void): Promise<UnlistenFn> {
  if (import.meta.env.DEV && !inTauri) return () => {};
  return listen<UnsupportedKeyboard[]>("unsupported-keyboards-changed", (e) => cb(e.payload));
}

/** A support request for an unsupported keyboard: versions, its ids, names
 *  and HID descriptions (read from Windows, nothing is sent to it). */
export async function unsupportedReport(id: string): Promise<string> {
  if (import.meta.env.DEV && !inTauri) {
    await mockDelay(200);
    return `Keymotion keyboard support request\nApp version: 0.1.0-dev\n\nKeyboard: SONiX AK820 PRO\nUSB ids: ${id}\n\nCollections (1):\n  1. usage page 0x0001, usage 0x0006 (keyboard)\n     descriptor, 4 bytes:\n     05 01 09 06\n`;
  }
  return invoke<string>("unsupported_report", { id });
}

/** Open the app's log or data folder in Explorer. */
export async function openFolder(folder: AppFolder): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("open_folder", { folder });
}

/**
 * Write a backup file into the Downloads folder. Calendar addresses go in
 * only with `withAddresses`. In the browser preview the file downloads.
 */
export async function exportBackup(withAddresses: boolean, fileName: string): Promise<BackupExported> {
  if (import.meta.env.DEV && !inTauri) {
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
  if (import.meta.env.DEV && !inTauri) return;
  await invoke("backup_reveal");
}

// Browser-only stand-in for the backup waiting to be imported.
let mockPending: Record<string, unknown> | null = null;

/** Read and check a backup file; it waits for `importBackup`. Nothing changes yet. */
export async function inspectBackup(bytes: ArrayBuffer): Promise<BackupSummary> {
  if (import.meta.env.DEV && !inTauri) return mockInspect(bytes);
  return invoke<BackupSummary>("backup_inspect", bytes);
}

/** Restore the chosen parts of the inspected backup. Nothing is written to the keyboard. */
export async function importBackup(sections: BackupSection[]): Promise<BackupImported> {
  if (import.meta.env.DEV && !inTauri) {
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
  if (import.meta.env.DEV && !inTauri) {
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
  if (list(b.profiles, "profiles")) sections.push({ section: "profiles", count: list(b.profiles, "profiles") });
  mockPending = b;
  return {
    createdMs: typeof b.createdMs === "number" ? b.createdMs : 0,
    appVersion: typeof b.appVersion === "string" ? b.appVersion : "",
    sections,
    screenNew: list(b.screen, "items"),
  };
}

// ---------------------------------------------------------------- profiles

/** The profiles for the keyboard of `modelId`: which one it has, what loading each would write. */
export async function getProfiles(modelId: string): Promise<ProfilesState> {
  if (import.meta.env.DEV && !inTauri) return mockProfilesState(modelId);
  return invoke<ProfilesState>("profiles_state", { modelId });
}

/** A new profile from what this computer recorded as saved on the keyboard (not the editors' drafts). */
export async function createProfile(modelId: string, name: string, parts: ProfileParts): Promise<ProfileSaved> {
  if (import.meta.env.DEV && !inTauri) return mockCreateProfile(modelId, name, parts);
  return invoke<ProfileSaved>("profiles_create", { modelId, name, parts });
}

/** The profile's own parts from what the keyboard has now. */
export async function updateProfile(modelId: string, id: string): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return mockUpdateProfile(modelId, id);
  await invoke("profiles_update", { modelId, id });
}

export async function renameProfile(id: string, name: string): Promise<void> {
  if (import.meta.env.DEV && !inTauri) return mockChange(id, (list, p) => {
    if (mockNameTaken(list, name, id)) throw new Error("profile name taken");
    p.name = name.trim().slice(0, 40);
  });
  await invoke("profiles_rename", { id, name });
}

/** A copy named `name` (made unique when taken). */
export async function duplicateProfile(id: string, name: string): Promise<ProfileSaved> {
  if (import.meta.env.DEV && !inTauri) {
    const source = id === MOCK_DEFAULT.id ? MOCK_DEFAULT : mockProfiles().find((p) => p.id === id);
    if (!source) throw new Error("unknown profile");
    return mockAdd({ ...structuredClone(source), name });
  }
  return invoke<ProfileSaved>("profiles_duplicate", { id, name });
}

export async function deleteProfile(id: string): Promise<void> {
  if (import.meta.env.DEV && !inTauri) {
    if (!mockProfiles().some((p) => p.id === id)) throw new Error("unknown profile");
    saveMockProfiles(mockProfiles().filter((p) => p.id !== id));
    return;
  }
  await invoke("profiles_delete", { id });
}

/** Write one profile to a file in the Downloads folder (its screen picture with it). */
export async function exportProfile(id: string, fileName: string): Promise<BackupExported> {
  if (import.meta.env.DEV && !inTauri) {
    const p = mockProfiles().find((x) => x.id === id);
    if (!p) throw new Error("unknown profile");
    const profile = { name: p.name, lighting: p.lighting, keymap: p.keymap, macros: p.macros, screen: p.screen };
    const file = { kind: "keymotion-profile", version: 1, createdMs: Date.now(), appVersion: "0.1.0-dev", profile };
    const url = URL.createObjectURL(new Blob([JSON.stringify(file, null, 2)], { type: "application/json" }));
    const a = Object.assign(document.createElement("a"), { href: url, download: `${fileName}.json` });
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return { file: `${fileName}.json`, inDownloads: true };
  }
  return invoke<BackupExported>("profiles_export", { id, fileName });
}

/** Add the profile in a file; it is checked whole first. */
export async function importProfile(bytes: ArrayBuffer): Promise<ProfileSaved> {
  if (import.meta.env.DEV && !inTauri) return mockImportProfile(bytes);
  return invoke<ProfileSaved>("profiles_import", bytes);
}

/**
 * Put one part of a profile on the keyboard (flash writes), unless it has
 * it already. The screen part is a library upload (`useScreen`).
 */
export function loadProfilePart(modelId: string, id: string, part: ProfilePart): Promise<PartLoaded> {
  return flashWrite(() =>
    import.meta.env.DEV && !inTauri
      ? mockLoadProfilePart(modelId, id, part)
      : serial(() => invoke<PartLoaded>("profiles_load", { modelId, id, part })),
  );
}

// Browser-only stand-in for profiles.rs: profiles in local storage, the
// keyboard's records from the mocks above. `?failPart=keymap` makes that
// part of a load fail, as another program on the keyboard would.

type MockLighting =
  | ({ kind: "effect" } & EffectRequest)
  | { kind: "per-key"; colors: Record<string, string>; brightness: number };

interface MockProfile {
  id: string;
  name: string;
  createdMs: number;
  updatedMs: number;
  lighting: MockLighting | null;
  keymap: { base: KeyAssignment[]; fn: KeyAssignment[] } | null;
  macros: (MacroInfo | null)[] | null;
  screen: { id: string; name: string } | null;
}

interface MockCurrent {
  lighting: MockLighting | null;
  keymap: { base: KeyAssignment[]; fn: KeyAssignment[] };
  macros: (MacroInfo | null)[];
  screen: { id: string; name: string } | null;
}

const MOCK_PROFILES = "ks.mock.profiles";
const MOCK_DEFAULT: MockProfile = {
  id: "default",
  name: "",
  createdMs: 0,
  updatedMs: 0,
  lighting: { kind: "effect", mode: 11, color: "#ff0000", rainbow: true, brightness: 5, speed: 3, direction: 0 },
  keymap: { base: [], fn: [] },
  macros: [],
  screen: null,
};

function mockProfiles(): MockProfile[] {
  try {
    const raw = localStorage.getItem(MOCK_PROFILES);
    return raw ? (JSON.parse(raw) as MockProfile[]) : [];
  } catch {
    return [];
  }
}

function saveMockProfiles(list: MockProfile[]) {
  try {
    localStorage.setItem(MOCK_PROFILES, JSON.stringify(list));
  } catch {
    /* browser preview only */
  }
}

const mockTrim = <T>(slots: (T | null)[]) => {
  const out = slots.slice();
  while (out.length && out[out.length - 1] === null) out.pop();
  return out;
};

async function mockLibrary(modelId: string): Promise<{ items: ScreenItem[]; shown: string | null }> {
  // Imported when used: screen.ts imports this module.
  const { getScreenState } = await import("./screen");
  const s = await getScreenState(modelId);
  return { items: s.items, shown: s.shown };
}

async function mockCurrent(modelId: string): Promise<MockCurrent> {
  const rec = (await lightingLoad()) ?? {};
  const saved = rec.savedPerKey as { colors?: Record<string, string>; brightness?: number } | null | undefined;
  const applied = rec.appliedEffect as EffectRequest | null | undefined;
  const lighting: MockLighting | null =
    saved && typeof saved === "object"
      ? { kind: "per-key", colors: { ...(saved.colors ?? {}) }, brightness: saved.brightness ?? 5 }
      : applied && typeof applied === "object"
        ? { kind: "effect", ...applied }
        : null;
  const lib = await mockLibrary(modelId);
  const shown = lib.items.find((i) => i.id === lib.shown);
  return {
    lighting,
    keymap: { base: mockKeymap.base.assignments, fn: mockKeymap.fn.assignments },
    macros: mockTrim(mockMacros.slots),
    screen: shown ? { id: shown.id, name: shown.name } : null,
  };
}

const sortedKeys = (l: KeyAssignment[]) => JSON.stringify([...l].sort((a, b) => a.led - b.led));

function mockSameLight(a: MockLighting, b: MockLighting | null): boolean {
  if (!b) return false;
  if (a.kind === "per-key" || b.kind === "per-key") {
    if (a.kind !== "per-key" || b.kind !== "per-key" || a.brightness !== b.brightness) return false;
    const ka = Object.keys(a.colors);
    return ka.length === Object.keys(b.colors).length && ka.every((k) => b.colors[k]?.toLowerCase() === a.colors[k]?.toLowerCase());
  }
  // As profiles.rs: the colour does not count while the rainbow is on.
  return (
    a.mode === b.mode &&
    a.rainbow === b.rainbow &&
    a.brightness === b.brightness &&
    a.speed === b.speed &&
    a.direction === b.direction &&
    (a.rainbow || a.color.toLowerCase() === b.color.toLowerCase())
  );
}

function mockMatches(p: MockProfile, part: ProfilePart, cur: MockCurrent): boolean {
  switch (part) {
    case "lighting":
      return !p.lighting || mockSameLight(p.lighting, cur.lighting);
    case "keymap":
      return !p.keymap || (sortedKeys(p.keymap.base) === sortedKeys(cur.keymap.base) && sortedKeys(p.keymap.fn) === sortedKeys(cur.keymap.fn));
    case "macros":
      return !p.macros || JSON.stringify(mockTrim(p.macros)) === JSON.stringify(cur.macros);
    case "screen":
      return !p.screen || cur.screen?.id === p.screen.id;
  }
}

const PARTS: ProfilePart[] = ["lighting", "keymap", "macros", "screen"];

function mockInfo(p: MockProfile, builtIn: boolean, cur: MockCurrent, items: ScreenItem[]): ProfileInfo {
  const has = (part: ProfilePart) => p[part] !== null;
  const item = p.screen ? items.find((i) => i.id === p.screen?.id) : undefined;
  return {
    id: p.id,
    name: builtIn ? "" : p.name,
    builtIn,
    createdMs: p.createdMs,
    updatedMs: p.updatedMs,
    lighting: !p.lighting
      ? null
      : p.lighting.kind === "effect"
        ? { kind: "effect", mode: p.lighting.mode, color: p.lighting.color, rainbow: p.lighting.rainbow, brightness: p.lighting.brightness }
        : { kind: "per-key", keys: Object.keys(p.lighting.colors).length, brightness: p.lighting.brightness },
    keymap: p.keymap ? p.keymap.base.length + p.keymap.fn.length : null,
    macros: p.macros ? p.macros.filter(Boolean).length : null,
    screen: p.screen ? { id: p.screen.id, name: item?.name ?? p.screen.name, inLibrary: !!item, pages: item?.pages ?? 0 } : null,
    onKeyboard: PARTS.some(has) && PARTS.every((part) => mockMatches(p, part, cur)),
    toWrite: PARTS.filter((part) => has(part) && !mockMatches(p, part, cur) && !(part === "screen" && !item)),
    otherModel: false,
  };
}

async function mockProfilesState(modelId: string): Promise<ProfilesState> {
  const cur = await mockCurrent(modelId);
  const { items } = await mockLibrary(modelId);
  const list = mockProfiles();
  return {
    profiles: list.map((p) => mockInfo(p, false, cur, items)),
    factory: mockInfo(MOCK_DEFAULT, true, cur, items),
    current: { lighting: !!cur.lighting, screen: cur.screen?.name ?? null },
    full: list.length >= 100,
  };
}

function mockNameTaken(list: MockProfile[], name: string, except?: string) {
  const n = name.trim().toLowerCase();
  return list.some((p) => p.id !== except && p.name.toLowerCase() === n);
}

function mockAdd(p: MockProfile): ProfileSaved {
  const list = mockProfiles();
  if (list.length >= 100) throw new Error("profile limit reached");
  let name = p.name.trim().slice(0, 40);
  for (let n = 2; mockNameTaken(list, name); n++) name = `${p.name.trim().slice(0, 34)} (${n})`;
  const id = `p${Date.now().toString(16)}${list.length}`;
  saveMockProfiles([...list, { ...p, id, name, createdMs: Date.now(), updatedMs: Date.now() }]);
  return { id, name };
}

async function mockCreateProfile(modelId: string, name: string, parts: ProfileParts): Promise<ProfileSaved> {
  await mockDelay(150);
  if (!name.trim()) throw new Error("profile name empty");
  if (mockNameTaken(mockProfiles(), name)) throw new Error("profile name taken");
  const cur = await mockCurrent(modelId);
  const p: MockProfile = {
    id: "",
    name,
    createdMs: 0,
    updatedMs: 0,
    lighting: parts.lighting ? cur.lighting : null,
    keymap: parts.keymap ? structuredClone(cur.keymap) : null,
    macros: parts.macros ? structuredClone(cur.macros) : null,
    screen: parts.screen ? cur.screen : null,
  };
  if (!PARTS.some((part) => p[part] !== null)) throw new Error("nothing to put in the profile");
  return mockAdd(p);
}

function mockChange(id: string, f: (list: MockProfile[], p: MockProfile) => void): Promise<void> {
  if (id === MOCK_DEFAULT.id) return Promise.reject(new Error("the default profile cannot be changed"));
  const list = mockProfiles();
  const p = list.find((x) => x.id === id);
  if (!p) return Promise.reject(new Error("unknown profile"));
  try {
    f(list, p);
  } catch (e) {
    return Promise.reject(e);
  }
  p.updatedMs = Date.now();
  saveMockProfiles(list);
  return mockDelay(120);
}

async function mockUpdateProfile(modelId: string, id: string): Promise<void> {
  const cur = await mockCurrent(modelId);
  return mockChange(id, (_, p) => {
    if (p.lighting && cur.lighting) p.lighting = cur.lighting;
    if (p.keymap) p.keymap = structuredClone(cur.keymap);
    if (p.macros) p.macros = structuredClone(cur.macros);
    if (p.screen && cur.screen) p.screen = cur.screen;
  });
}

async function mockImportProfile(bytes: ArrayBuffer): Promise<ProfileSaved> {
  await mockDelay(200);
  let f: Record<string, unknown>;
  try {
    f = JSON.parse(new TextDecoder().decode(bytes).replace(/^\s*/, "")) as Record<string, unknown>;
  } catch {
    throw new Error("not a profile file");
  }
  if (f?.kind === "kb-backup") throw new Error("a backup file, not a profile");
  if (f?.kind !== "keymotion-profile") throw new Error("not a profile file");
  if (typeof f.version !== "number") throw new Error("profile file damaged");
  if (f.version > 1) throw new Error("profile from a newer version");
  const p = f.profile as Partial<MockProfile> | undefined;
  if (!p || typeof p.name !== "string" || !p.name.trim()) throw new Error("profile file damaged");
  const profile: MockProfile = {
    id: "",
    name: p.name,
    createdMs: 0,
    updatedMs: 0,
    lighting: p.lighting ?? null,
    keymap: p.keymap ?? null,
    macros: p.macros ?? null,
    screen: p.screen ?? null,
  };
  if (!PARTS.some((part) => profile[part] !== null)) throw new Error("profile file damaged");
  return mockAdd(profile);
}

async function mockLoadProfilePart(modelId: string, id: string, part: ProfilePart): Promise<PartLoaded> {
  const p = id === MOCK_DEFAULT.id ? MOCK_DEFAULT : mockProfiles().find((x) => x.id === id);
  if (!p) throw new Error("unknown profile");
  if (part === "screen") throw new Error("the screen part is uploaded from the library");
  if (mockMatches(p, part, await mockCurrent(modelId))) return "same";
  await mockDelay(part === "lighting" ? 400 : 700);
  if (new URLSearchParams(location.search).get("failPart") === part) {
    throw new Error(`profile part failed (${part}): another program is using the keyboard (signalrgb)`);
  }
  if (part === "lighting" && p.lighting) {
    const rec = (await lightingLoad()) ?? {};
    if (p.lighting.kind === "effect") {
      const { mode, color, rainbow, brightness, speed, direction } = p.lighting;
      const effect = { mode, color, rainbow, brightness, speed, direction };
      await lightingSave({ ...rec, effect, appliedEffect: effect, savedPerKey: null, tab: "effects", draft: null });
    } else {
      const { colors, brightness } = p.lighting;
      await lightingSave({
        ...rec,
        perKey: colors,
        perKeyBrightness: brightness,
        savedPerKey: { colors, brightness },
        appliedEffect: null,
        tab: "per-key",
        draft: null,
      });
    }
  }
  if (part === "keymap" && p.keymap) {
    mockKeymap = { base: { assignments: p.keymap.base, writtenMs: Date.now() }, fn: { assignments: p.keymap.fn, writtenMs: Date.now() } };
  }
  if (part === "macros" && p.macros) mockMacros = { ...mockMacros, slots: mockTrim(p.macros), writtenMs: Date.now() };
  return "written";
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
