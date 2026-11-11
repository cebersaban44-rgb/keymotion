// Device safety: the questions and notices that guard the keyboard's memory.
//
// - Untested firmware: the backend refuses the first write of a session on
//   a firmware version the model file does not list (`firmware-unverified`).
//   The user is asked once; a yes holds until the app closes.
// - Settings files that could not be read went back to their defaults; the
//   old file was kept aside. The user is told once which records.
//
// Refusals for several keyboards or another program on the keyboard need no
// question: they come back as errors (lib/errors.ts).

import { create } from "zustand";
import type { SettingsReset } from "../bindings/SettingsReset";
import type { MessageKey } from "./i18n";
import {
  confirmFirmware,
  dismissSettingsResets,
  getSettingsResets,
  onFirmwareUnverified,
  setFirmwareGate,
} from "./ipc";

interface SafetyStore {
  /** Firmware version the user is being asked about. */
  firmware: string | null;
  /** Records reset at start-up, not yet acknowledged. */
  resets: SettingsReset[];
  answerFirmware: (go: boolean) => void;
  dismissResets: () => void;
}

let pending: Promise<boolean> | null = null;
let resolveAsk: ((go: boolean) => void) | null = null;
/** The user said no in this session: the live mode does not ask again. */
let declined = false;

export const useSafety = create<SafetyStore>((set) => ({
  firmware: null,
  resets: [],
  answerFirmware: (go) => {
    set({ firmware: null });
    resolveAsk?.(go);
    resolveAsk = null;
  },
  dismissResets: () => {
    set({ resets: [] });
    dismissSettingsResets().catch(console.error);
  },
}));

/**
 * Ask whether to write on untested firmware. One question at a time: the
 * refusal event and the refused call of the same write share it.
 */
function askFirmware(version: string, fromUser: boolean): Promise<boolean> {
  if (pending) return pending;
  if (!fromUser && declined) return Promise.resolve(false);
  pending = new Promise<boolean>((resolve) => {
    resolveAsk = resolve;
    useSafety.setState({ firmware: version });
  })
    .then(async (go) => {
      declined = !go;
      if (go) await confirmFirmware(version);
      return go;
    })
    .finally(() => {
      pending = null;
    });
  return pending;
}

/** Start answering the backend. Call once from the app; returns the cleanup. */
export function startSafety(): () => void {
  setFirmwareGate(askFirmware);
  let stopped = false;
  let off: (() => void) | undefined;
  onFirmwareUnverified((version) => void askFirmware(version, false).catch(console.error)).then((u) => {
    if (stopped) u();
    else off = u;
  });
  getSettingsResets()
    .then((resets) => {
      if (!stopped) useSafety.setState({ resets });
    })
    .catch(console.error);
  return () => {
    stopped = true;
    off?.();
  };
}

/** What each settings file holds, in the user's words. */
const RECORDS: Record<string, MessageKey> = {
  "keymap.json": "safety.record.keymap",
  "macros.json": "safety.record.macros",
  "calibration.json": "safety.record.calibration",
  "clock.json": "safety.record.clock",
  "gauges.json": "safety.record.gauges",
  "pomodoro.json": "safety.record.pomodoro",
  "calendar.json": "safety.record.calendar",
  "reminders.json": "safety.record.reminders",
};

/** The record a settings file holds; `null` for files not worth a notice (the battery cache). */
export function recordKey(file: string): MessageKey | null {
  return RECORDS[file] ?? null;
}

/** Records only the app knows about the keyboard: it cannot report them. */
export const KEYBOARD_RECORDS = ["keymap.json", "macros.json"];
