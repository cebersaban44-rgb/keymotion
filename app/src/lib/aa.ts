// A keyboard of AJAZZ's "AA" family on the cable (the AK980 PRO on firmware
// 3.01, "769"): what the backend says about it beyond the model file
// (`KeyboardInfo.aaStatus`), and its backups.
//
// - Read-only: nothing is written to it (what it reported doesn't look as
//   expected, a firmware update was left unfinished, or its backup couldn't
//   be saved). Every page that writes says why and keeps its save off.
// - Live lights: it shows them only in its per-key mode, which the backend
//   switches to and back; the pages that start live lights say what happens.
// - Backups: taken before Keymotion first writes to it and before every
//   restore; the Connection page lists them and puts one back.

import type { AaStatus } from "../bindings/AaStatus";
import type { KeyboardBackupInfo } from "../bindings/KeyboardBackupInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { KeyboardRestoreResult } from "../bindings/KeyboardRestoreResult";
import type { ReadOnlyReason } from "../bindings/ReadOnlyReason";
import { useT, type MessageKey, type Translate } from "./i18n";
import { flashWrite, inTauri, invoke } from "./ipc";
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
  // The Fn key and a slot past the keys stay as the keyboard has them.
  return {
    written: ["base layer", "Fn layer", "effect", "per-key table", "macros"],
    kept: [
      { layer: "base", slot: 85, led: 96, reason: "the Fn key is never reassigned" },
      { layer: "fn", slot: 126, led: null, reason: "slot 126 (end of the table) must stay as read" },
    ],
  };
}
