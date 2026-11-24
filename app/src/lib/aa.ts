// A keyboard of AJAZZ's "AA" family on the cable (the AK980 PRO on firmware
// 3.01, "769"): what the backend says about it beyond the model file
// (`KeyboardInfo.aaStatus`), and its backups.
//
// - Read-only: nothing is written to it (what it reported doesn't look as
//   expected, a firmware update was left unfinished, or its backup couldn't
//   be saved). Every page that writes says why and keeps its save off.
// - Live lights: it shows them in any of its lighting modes, never saves
//   them, and goes back to its own lighting by itself a few seconds after
//   they end; the pages that start live lights say when it takes none.
// - Backups: taken at its first connection and before every restore; the
//   Connection page lists them and puts one back (the restore flow below).
// - Its key mapping and macros: it reports them, and the app's records
//   follow it (lib/keymap.ts and lib/macros.ts show them as the keyboard
//   has them).

import { create } from "zustand";
import type { AaStatus } from "../bindings/AaStatus";
import type { KeyboardBackupInfo } from "../bindings/KeyboardBackupInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { KeyboardRestoreResult } from "../bindings/KeyboardRestoreResult";
import type { ReadOnlyReason } from "../bindings/ReadOnlyReason";
import { friendlyError } from "./errors";
import { useT, type MessageKey, type Translate } from "./i18n";
import { flashWrite, inTauri, invoke } from "./ipc";
import { adoptKeymapRecord } from "./keymap";
import { useLighting } from "./lighting";
import { useLive } from "./live";
import { adoptMacrosRecord } from "./macros";
import { useScreen } from "./screen";
import { useActiveKeyboard } from "./store";

/** What the backend says about an "AA" keyboard on the cable; `null` for
 *  any other keyboard or connection. */
export function aaStatusOf(kb: KeyboardInfo | null | undefined): AaStatus | null {
  return kb?.connection === "usb" ? (kb.aaStatus ?? null) : null;
}

/** The short reason for each read-only state: under a save button, in a
 *  save bar, as a disabled button's tooltip. */
const READ_ONLY_SHORT: Record<ReadOnlyReason["kind"], MessageKey> = {
  "not-sane": "aa.ro.notSane.short",
  "update-pending": "aa.ro.update.short",
  "no-backup": "aa.ro.backup.short",
};

/** Why nothing can be written to `kb` now, in one sentence; `null` when it
 *  can (as far as its read-only state goes: the cable and the rest are
 *  each page's own checks). */
export function writeBlock(kb: KeyboardInfo | null | undefined, t: Translate): string | null {
  const r = aaStatusOf(kb)?.readOnly;
  return r ? t(READ_ONLY_SHORT[r.kind]) : null;
}

/** `writeBlock` for `kb`, or for the keyboard the UI shows. */
export function useWriteBlock(kb?: KeyboardInfo | null): string | null {
  const t = useT();
  const active = useActiveKeyboard();
  return writeBlock(kb === undefined ? active : kb, t);
}

/** The keyboard the UI shows is of the "AA" family and on the cable. */
export function useAaOnCable(): boolean {
  const kb = useActiveKeyboard();
  return kb?.protocol === "ajazz-aa" && kb.connection === "usb";
}

/** The keyboard the UI shows is of the "AA" family (on any connection):
 *  AJAZZ's old official driver doesn't know its firmware, so the screen
 *  pages don't point to it. */
export function useAaKeyboard(): boolean {
  return useActiveKeyboard()?.protocol === "ajazz-aa";
}

/** A screen upload runs (library, editor, profile or live screen): a
 *  restore would wait minutes behind it. */
export function useUploadRunning(): boolean {
  const screen = useScreen((s) => s.upload !== null);
  const live = useLive((s) => !!s.state?.uploading);
  return screen || live;
}

// --------------------------------------------------------- restore flow

/**
 * The restore of a backup, in visible steps (components/KeyboardBackups):
 *
 *   1. What comes back (and what doesn't) → Restore
 *   2. The lighting's colour, only when the backup doesn't know it and the
 *      keyboard refused without one → Restore
 *   3. Done: what was put back, the keys left as they are → OK.
 *
 * Kept here, not in the component: a restore runs on when the user opens
 * another page or the cable drops, and its outcome (done, or stopped
 * part-way) waits for them on the Connection page and in a notice over
 * every other page.
 */
export type RestoreFlow =
  | { step: "check"; backup: KeyboardBackupInfo; busy: boolean; error: string | null }
  | { step: "color"; backup: KeyboardBackupInfo; busy: boolean; error: string | null; color: string; rainbow: boolean }
  | { step: "done"; backup: KeyboardBackupInfo; result: KeyboardRestoreResult; askedColor: boolean };

interface RestoreStore {
  flow: RestoreFlow | null;
  /** The backup last put back: "the backup from … is back on the
   *  keyboard" over the list. */
  restored: KeyboardBackupInfo | null;
  /** The backup list is on screen (the outcome shows there, not in the
   *  notice over the page). */
  listShown: number;
  /** Bumped after every restore: the list is read again. */
  listVersion: number;
}

export const useRestore = create<RestoreStore>(() => ({ flow: null, restored: null, listShown: 0, listVersion: 0 }));

/** The keyboard refused a restore for want of the effect's colour. */
const COLOR_NEEDED = /does not report its effect colou?r/i;

/** Run step 1 or 2 of the flow; the outcome lands in the store. */
export async function runRestore(
  f: Extract<RestoreFlow, { step: "check" | "color" }>,
  kb: Pick<KeyboardInfo, "recordId">,
  t: Translate,
): Promise<void> {
  const set = (flow: RestoreFlow | null) => useRestore.setState({ flow });
  set({ ...f, busy: true, error: null });
  const color = f.step === "color" ? { color: f.color, rainbow: f.rainbow } : undefined;
  const bump = () => useRestore.setState((s) => ({ listVersion: s.listVersion + 1 }));
  try {
    const result = await restoreKeyboardBackup(f.backup.file, color);
    set({ step: "done", backup: f.backup, result, askedColor: f.step === "color" });
    bump();
    // The pages follow the keyboard: its keymap and macros (also told by
    // the backend's "keyboard-records-changed"), and the Lighting page's
    // effect when its colour is known, else "not saved yet".
    useLighting.getState().followKeyboard(result.effect);
    await Promise.all([adoptKeymapRecord(kb.recordId), adoptMacrosRecord(kb.recordId)].map((p) => p.catch(console.error)));
  } catch (e) {
    const raw = e instanceof Error ? e.message : String(e);
    if (f.step === "check" && COLOR_NEEDED.test(raw)) {
      // Nothing was written: the same backup goes again with a colour.
      const known = useLighting.getState().appliedEffect;
      set({ step: "color", backup: f.backup, busy: false, error: null, color: known?.color ?? "#ffffff", rainbow: false });
      return;
    }
    set({ ...f, busy: false, error: friendlyError(e, t) });
    bump();
  }
}

/** Close the flow; after the done step, the list says the backup is back. */
export function closeRestore(done?: KeyboardBackupInfo) {
  useRestore.setState({ flow: null, ...(done ? { restored: done } : {}) });
}

// --------------------------------------------------------------- backups

/** The "AA" keyboards' backups in the data folder, oldest first. */
export async function getKeyboardBackups(): Promise<KeyboardBackupInfo[]> {
  if (import.meta.env.DEV && !inTauri) return mockBackups();
  return invoke<KeyboardBackupInfo[]>("keyboard_backups");
}

/** Put a backup back on the keyboard on the cable (a write to its memory:
 *  the untested firmware question may come first). `color` only when the
 *  backup doesn't know its effect's colour and the keyboard refused
 *  without one. */
export function restoreKeyboardBackup(
  file: string,
  color?: { color: string; rainbow: boolean },
): Promise<KeyboardRestoreResult> {
  return flashWrite(async () => {
    if (import.meta.env.DEV && !inTauri) return mockRestore(file, color);
    return invoke<KeyboardRestoreResult>("keyboard_backup_restore", {
      file,
      color: color?.color ?? null,
      rainbow: color ? color.rainbow : null,
    });
  });
}

// ------------------------------------------------- browser preview (mock)

/** `?mock=aa769&aa=…` (lib/ipc.ts): the stand-in keyboard's states. */
export function mockAaFlags(): Set<string> {
  if (!import.meta.env.DEV || typeof location === "undefined") return new Set();
  return new Set((new URLSearchParams(location.search).get("aa") ?? "").split(",").filter(Boolean));
}

let mockList: KeyboardBackupInfo[] | null = null;

function mockBackups(): KeyboardBackupInfo[] {
  if (!mockList) {
    const flags = mockAaFlags();
    const day = 86_400_000;
    const now = Date.now();
    mockList = [
      // Another keyboard's (or this one before a firmware update): listed,
      // not restorable.
      {
        file: "aa-8039-0300-1763000000000.json",
        savedAtMs: now - 9 * day,
        reason: "first-connect",
        recordId: "ajazz-ak980-pro.aa",
        firmware: "3.00",
        matches: false,
        colorKnown: true,
      },
      {
        file: "aa-8039-0301-1763500000000.json",
        savedAtMs: now - 2 * day - 3_600_000,
        reason: "first-connect",
        recordId: "ajazz-ak980-pro.aa",
        firmware: "3.01",
        matches: true,
        colorKnown: !flags.has("nocolor"),
      },
    ];
  }
  return mockList.map((b) => ({ ...b }));
}

async function mockRestore(file: string, color?: { color: string; rainbow: boolean }): Promise<KeyboardRestoreResult> {
  await new Promise((r) => setTimeout(r, 400));
  const backup = mockBackups().find((b) => b.file === file);
  if (!backup) throw new Error("backup not read: the system cannot find the file specified. (os error 2)");
  if (!backup.matches) throw new Error("the backup is not of the keyboard on the USB cable");
  if (!backup.colorKnown && !color) {
    throw new Error(
      "the keyboard does not report its effect colour (it reads back white after any write); the colour to write has to be given",
    );
  }
  await new Promise((r) => setTimeout(r, 1400));
  mockList = [
    ...mockBackups(),
    {
      file: `aa-8039-0301-${Date.now()}.json`,
      savedAtMs: Date.now(),
      reason: "before-restore",
      recordId: "ajazz-ak980-pro.aa",
      firmware: "3.01",
      matches: true,
      colorKnown: false,
    },
  ];
  // The Fn key and a slot past the keys stay as the keyboard has them; the
  // effect's colour is known when the backup knew it or one was given.
  return {
    written: ["base layer", "Fn layer", "effect", "per-key table", "macros"],
    kept: [
      { layer: "base", slot: 85, led: 96, reason: "the Fn key is never reassigned" },
      { layer: "fn", slot: 126, led: null, reason: "slot 126 (end of the table) must stay as read" },
    ],
    effect:
      backup.colorKnown || color
        ? { mode: 1, color: color?.color ?? "#0080ff", rainbow: color?.rainbow ?? false, brightness: 5, speed: 3, direction: 0 }
        : null,
  };
}
