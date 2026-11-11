// What a failed action says to the user. The backend's error text is
// technical and English; the user gets what happened and what to do, in
// the UI language (docs/arayuz-dili.md). The original goes to the console.

import type { MessageKey } from "./i18n";

const RULES: [RegExp, MessageKey][] = [
  [/blocked by safety guard/i, "err.guard"],
  [/screen upload stopped/i, "err.uploadStopped"],
  [/live mode is off/i, "err.liveOff"],
  [/fn key cannot be reassigned/i, "err.fn"],
  [/factory backup cannot be removed/i, "err.factory"],
  [/larger than|too large|at most 4096|frames; the keyboard takes|more than \d+ frames/i, "err.tooLarge"],
  [/not a gif|not a readable gif|no frames|expected binary data|unsupported/i, "err.badFile"],
  [/echoed different data|checksum/i, "err.verify"],
  [/refused|no valid echo|stayed busy|timed? ?out|status 0x/i, "err.noAnswer"],
  [/no keyboard|not connected|no command channel|disconnected|unavailable|no device|not found|access denied/i, "err.noKeyboard"],
  [/not saved|os error|permission/i, "err.save"],
];

/** A short, translated message for an error from the backend. */
export function friendlyError(e: unknown, t: (key: MessageKey) => string): string {
  const raw = e instanceof Error ? e.message : String(e);
  console.error(raw);
  const hit = RULES.find(([re]) => re.test(raw));
  return t(hit ? hit[1] : "err.generic");
}
