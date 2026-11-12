// What a failed action says to the user. The backend's error text is
// technical and English; the user gets what happened and what to do, in
// the UI language, in plain words. The original goes to the console
// and to the app's log, so "Copy diagnostics" can carry it. An unexpected
// one is also offered for reporting, once a session (lib/report.ts).

import type { MessageKey } from "./i18n";
import { logUiError } from "./ipc";
import { offerProblem } from "./report";

const RULES: [RegExp, MessageKey][] = [
  // Writes refused before anything was sent (device.rs `WriteBlock`).
  [/more than one keyboard on the usb cable/i, "err.several"],
  [/another program is using the keyboard \(ajazz-driver\)/i, "err.conflict.ajazz"],
  [/another program is using the keyboard \(signalrgb\)/i, "err.conflict.signalrgb"],
  [/another program is using the keyboard \(openrgb\)/i, "err.conflict.openrgb"],
  [/another program is using the keyboard \(kb-cli\)/i, "err.conflict.kb"],
  [/untested keyboard firmware/i, "err.untested"],
  [/write cancelled by the user/i, "err.cancelled"],
  // Profiles and profile files (profiles.rs). A load's own failure reads
  // "profile part failed (keymap): …" and falls through to its cause.
  [/a backup file, not a profile/i, "err.profile.backup"],
  [/not a profile file/i, "err.profile.notProfile"],
  [/profile from a newer version/i, "err.profile.newer"],
  [/profile file damaged/i, "err.profile.damaged"],
  [/profile file too large/i, "err.profile.tooLarge"],
  [/profile for another keyboard model/i, "err.profile.otherModel"],
  [/profile name taken/i, "err.profile.nameTaken"],
  [/profile name empty/i, "err.profile.name"],
  [/profile limit reached/i, "err.profile.full"],
  [/nothing to put in the profile/i, "err.profile.nothing"],
  [/blocked by safety guard/i, "err.guard"],
  [/screen upload stopped/i, "err.uploadStopped"],
  [/live mode is off/i, "err.liveOff"],
  [/fn key cannot be reassigned/i, "err.fn"],
  [/unsupported keyboard firmware/i, "err.firmware"],
  [/factory backup cannot be removed/i, "err.factory"],
  [/no official driver copy/i, "err.noDriver"],
  [/factory animation; not acknowledged/i, "err.factoryAck"],
  [/larger than|too large|at most 4096|frames; the keyboard takes|more than \d+ frames/i, "err.tooLarge"],
  [/not a gif|not a readable gif|no frames|expected binary data|unsupported/i, "err.badFile"],
  [/echoed different data|checksum/i, "err.verify"],
  [/refused|no valid echo|stayed busy|timed? ?out|status 0x/i, "err.noAnswer"],
  [/no keyboard|not connected|no command channel|disconnected|unavailable|no device|not found|access denied/i, "err.noKeyboard"],
  [/not saved|os error|permission/i, "err.save"],
];

/** Situations the user can fix (another program open, no cable, a name
 *  taken, a wrong file): the log records them as warnings ("action
 *  refused"), so real errors stand out in the diagnostics. */
const EXPECTED: ReadonlySet<MessageKey> = new Set<MessageKey>([
  "err.several",
  "err.conflict.ajazz",
  "err.conflict.signalrgb",
  "err.conflict.openrgb",
  "err.conflict.kb",
  "err.untested",
  "err.cancelled",
  "err.profile.backup",
  "err.profile.notProfile",
  "err.profile.newer",
  "err.profile.damaged",
  "err.profile.tooLarge",
  "err.profile.otherModel",
  "err.profile.nameTaken",
  "err.profile.name",
  "err.profile.full",
  "err.profile.nothing",
  "err.liveOff",
  "err.fn",
  "err.factory",
  "err.noDriver",
  "err.factoryAck",
  "err.tooLarge",
  "err.badFile",
  "err.noKeyboard",
]);

/** A short, translated message for an error from the backend. */
export function friendlyError(e: unknown, t: (key: MessageKey) => string, quiet = false): string {
  const raw = e instanceof Error ? e.message : String(e);
  const hit = RULES.find(([re]) => re.test(raw));
  const key = hit ? hit[1] : "err.generic";
  if (!quiet) {
    const expected = EXPECTED.has(key);
    (expected ? console.warn : console.error)(raw);
    void logUiError(`${expected ? "action refused" : "action failed"}: ${raw}`);
    if (!expected) offerProblem(raw);
  }
  return t(key);
}
