// What the app says about a keyboard of AJAZZ's "AA" family (the AK980 PRO
// on firmware 3.01, "769"), worked out from the backend's English texts:
// the sections a restore wrote, why it left a key as it was, and the
// numbers in the screen upload's cancel and wait messages. The words for
// the user are in the dictionaries (lib/i18n); this file only tells which.
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

/** "screen upload cancelled after 399 of 586 parts": how far it got. */
export function uploadCancelledAt(raw: string): { done: number; total: number } | null {
  const m = /screen upload cancelled after (\d+) of (\d+) parts/i.exec(raw);
  return m ? { done: Number(m[1]), total: Number(m[2]) } : null;
}

/** "a screen upload stopped part-way; no commands for another 4200 ms":
 *  the wait in whole seconds, at least 1. */
export function uploadWaitSeconds(raw: string): number | null {
  const m = /no commands for another (\d+) ms/i.exec(raw);
  return m ? Math.max(1, Math.ceil(Number(m[1]) / 1000)) : null;
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
