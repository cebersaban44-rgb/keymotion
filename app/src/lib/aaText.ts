// What the app says about a keyboard of AJAZZ's "AA" family (the AK980 PRO
// on firmware 3.01, "769"), worked out from the backend's English texts:
// the sections a restore wrote, why it left a key as it was, and what the
// read-only and live-lights notices say. The words for the user are in the
// dictionaries (lib/i18n); this file only tells which.
//
// No imports: tools/aa-ui.test.mjs runs this file in Node.

/** A section a restore wrote (`KeyboardRestoreResult.written`). */
export type RestoredPart = "base" | "fn" | "effect" | "perKey" | "macros";

const PARTS: Record<string, RestoredPart> = {
  "base layer": "base",
  "Fn layer": "fn",
  effect: "effect",
  "per-key table": "perKey",
  macros: "macros",
};

/** The section `name` names, in the order the restore writes them;
 *  `null` for a name this version doesn't know. */
export function restoredPart(name: string): RestoredPart | null {
  return PARTS[name] ?? null;
}

/** Every known section of `written`, each once, in the order shown. */
export function restoredParts(written: readonly string[]): RestoredPart[] {
  const order: RestoredPart[] = ["base", "fn", "effect", "perKey", "macros"];
  const got = new Set(written.map(restoredPart).filter((p): p is RestoredPart => p !== null));
  return order.filter((p) => got.has(p));
}

/** Why a restore left a key as the keyboard has it (`KeptKey.reason`). */
export type KeptWhy = "fn" | "knob" | "fixed" | "function" | "other";

/** The reason behind a kept key, from the driver's English text. */
export function keptWhy(reason: string): KeptWhy {
  if (/the Fn key is never reassigned/i.test(reason)) return "fn";
  if (/the knob is never reassigned/i.test(reason)) return "knob";
  if (/end of the table|must stay as read/i.test(reason)) return "fixed";
  if (/firmware function|entries are never written/i.test(reason)) return "function";
  return "other";
}

/** The firmware version a live-lights block waits on ("untested keyboard
 *  firmware 3.01"), when that is the reason. */
export function untestedFirmware(reason: string): string | null {
  return /untested keyboard firmware (\S+)/i.exec(reason)?.[1] ?? null;
}

/** The GitHub issue form's title and model for a keyboard on firmware
 *  `firmware` (the "AA" family's beta: "tell us how it works"). */
export function betaIssue(model: string, firmware: string | null): { title: string; model: string } {
  const name = firmware ? `${model} firmware ${firmware}` : model;
  return { title: `[Keyboard] ${name}`, model: name };
}

/** The read-only notice's body when what the keyboard reported did not
 *  look as expected: a read that did not finish (another program holding
 *  the keyboard? the background reads again) tells what to try; data that
 *  failed the check asks for a report. */
export function notSaneBody(readFailed: boolean): "aa.ro.notSane.read" | "aa.ro.notSane.body" {
  return readFailed ? "aa.ro.notSane.read" : "aa.ro.notSane.body";
}

/** What the live-lights notice says, from `AaStatus`: why live lighting
 *  doesn't show, that the keyboard's own lighting comes back about 30
 *  seconds after it ends (only when Keymotion switched it and has something
 *  to put back), or nothing. */
export function liveNoticeKind(s: {
  liveLightsOff: unknown;
  liveArmed: boolean;
  liveRestores: boolean;
}): "off" | "armed" | null {
  if (s.liveLightsOff) return "off";
  return s.liveArmed && s.liveRestores ? "armed" : null;
}

/** Why a backup can't be put back now: the keyboard takes no writes, or a
 *  screen upload runs (the restore would wait minutes behind it). */
export function restoreHeldBy(readOnly: string | null, uploading: boolean): "read-only" | "upload" | null {
  if (readOnly) return "read-only";
  return uploading ? "upload" : null;
}

/** The first macro slot a new macro can take: not used, and not holding a
 *  macro made in another program (`fixed`, kept as the keyboard has it);
 *  `null` when none is left. */
export function freeMacroSlot(slots: readonly unknown[], fixed: readonly number[], max: number): number | null {
  for (let i = 0; i < max; i++) {
    if ((slots[i] ?? null) === null && !fixed.includes(i)) return i;
  }
  return null;
}

/** A layer of keys without the ones set in another program (`fixed`):
 *  Keymotion neither shows nor changes them. */
export function withoutFixed<T>(layer: Record<number, T>, fixed: readonly number[]): Record<number, T> {
  const out: Record<number, T> = {};
  for (const [led, v] of Object.entries(layer)) if (!fixed.includes(Number(led))) out[Number(led)] = v;
  return out;
}
