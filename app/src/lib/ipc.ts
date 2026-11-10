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
// Generated from Rust by `cargo test` (dto.rs export_mock_geometry).
import ak980Geometry from "../bindings/geometry-ajazz-ak980-pro.json";
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
let pendingFrame: { colors: LedColor[] | null } | null = null;
let frameInFlight = false;

/** RAM-only live frame; `null` stops the stream and the keyboard reverts. */
export function setLiveFrame(colors: LedColor[] | null): Promise<void> {
  if (!inTauri) return Promise.resolve();
  pendingFrame = { colors };
  if (frameInFlight) return Promise.resolve();
  frameInFlight = true;
  return serial(async () => {
    try {
      while (pendingFrame) {
        const next = pendingFrame;
        pendingFrame = null;
        await invoke("set_live_frame", { colors: next.colors });
      }
    } finally {
      frameInFlight = false;
    }
  });
}

/** Ask the backend to mirror the colours shown on the keyboard. */
export function setMirror(enabled: boolean): Promise<void> {
  if (!inTauri) return Promise.resolve();
  return serial(() => invoke<void>("set_mirror", { enabled }));
}

export async function onMirror(cb: (colors: LedColor[]) => void): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  return listen<LedColor[]>("lighting-mirror", (e) => cb(e.payload));
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
