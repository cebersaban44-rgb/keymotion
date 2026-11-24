// What a failed action says to the user. The backend's error text is
// technical and English; the user gets what happened and what to do, in
// the UI language, in plain words (the rules are in lib/errorRules.ts). The
// original goes to the console and to the app's log, so "Copy diagnostics"
// can carry it. An unexpected one is also offered for reporting, once a
// session (lib/report.ts).

import { errorRule, EXPECTED } from "./errorRules";
import type { MessageKey, MessageVars } from "./i18n";
import { logUiError } from "./ipc";
import { offerProblem } from "./report";

/** A short, translated message for an error from the backend. */
export function friendlyError(e: unknown, t: (key: MessageKey, vars?: MessageVars) => string, quiet = false): string {
  const raw = e instanceof Error ? e.message : String(e);
  const { key, vars } = errorRule(raw);
  if (!quiet) {
    const expected = EXPECTED.has(key);
    (expected ? console.warn : console.error)(raw);
    void logUiError(`${expected ? "action refused" : "action failed"}: ${raw}`);
    if (!expected) offerProblem(raw);
  }
  return vars ? t(key, vars) : t(key);
}
