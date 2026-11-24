// What the pages say about a keyboard of AJAZZ's "AA" family on the cable
// (the AK980 PRO on firmware 3.01, lib/aa.ts), one component for each
// thing, so every page says it the same way:
//
// - KeyboardNotices: nothing is written to it, and why; another program
//   seems to talk to it. On every page that writes to the keyboard.
// - LiveLightsNotice: live lighting doesn't show on its keys for now, and
//   why. On every page that turns live lighting on.
// - BetaPill / BetaNote: support for this firmware is new.

import { FlaskConical, MessageSquareWarning, RadioTower } from "lucide-react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { aaStatusOf } from "../lib/aa";
import { betaIssue, liveOffReason, notSaneBody } from "../lib/aaText";
import { errorRule } from "../lib/errorRules";
import { friendlyError } from "../lib/errors";
import { useT, type MessageKey } from "../lib/i18n";
import { openIssue } from "../lib/ipc";
import { useReport } from "../lib/report";
import { useActiveKeyboard } from "../lib/store";
import { InfoBar } from "./controls";

const READ_ONLY: Record<string, { title: MessageKey; body: MessageKey }> = {
  "not-sane": { title: "aa.ro.notSane.title", body: "aa.ro.notSane.body" },
  "update-pending": { title: "aa.ro.update.title", body: "aa.ro.update.body" },
  "no-backup": { title: "aa.ro.backup.title", body: "aa.ro.backup.body" },
};

/** Nothing is written to the keyboard, and why; another program seems to
 *  talk to it. Nothing for any other keyboard. */
export function KeyboardNotices({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const openReport = useReport((s) => s.openReport);
  const status = aaStatusOf(kb);
  if (!status) return null;
  const ro = status.readOnly;
  const text = ro ? READ_ONLY[ro.kind] : null;
  // A read that did not finish says what to try (the background reads
  // again); data that failed the check asks for a report.
  const readFailed = ro?.kind === "not-sane" && ro.readFailed;
  const body = ro?.kind === "not-sane" ? notSaneBody(ro.readFailed) : text?.body;
  return (
    <>
      {ro && text && body && (
        <InfoBar
          tone="warn"
          title={t(text.title)}
          data={`aa-read-only-${ro.kind}${readFailed ? "-read" : ""}`}
          actions={
            ro.kind === "not-sane" && !readFailed ? (
              <button
                type="button"
                className="btn-secondary"
                onClick={() =>
                  openReport({
                    kind: "bug",
                    title: `[Problem] ${kb.modelName} firmware ${kb.firmware ?? "?"}: only read, not as expected`,
                    // The reasons come grouped (diagnostics.rs
                    // summarize_reasons), one per line.
                    context: `The keyboard does not look as expected:\n${
                      ro.reasons.map((r) => `- ${r}`).join("\n") || "(no reason given)"
                    }`,
                  })
                }
                data-aa-report
              >
                <MessageSquareWarning size={14} aria-hidden />
                {t("report.settings.problem")}
              </button>
            ) : undefined
          }
        >
          <span className="block max-w-[80ch]">{t(body)}</span>
        </InfoBar>
      )}
      {status.otherProgramSuspected && (
        <InfoBar icon={<RadioTower size={20} strokeWidth={1.6} />} title={t("aa.other.title")} data="aa-other-program">
          <span className="block max-w-[80ch]">{t("aa.other.body")}</span>
        </InfoBar>
      )}
    </>
  );
}

/**
 * Live lighting on an "AA" keyboard: it shows in any of the keyboard's
 * lighting modes, is never saved, and the keyboard's own lighting comes
 * back by itself a few seconds after it ends, so there is nothing to say
 * while it shows. Says that it doesn't show on the keys for now, and why
 * when the keyboard takes none (just after it is switched on, during a
 * reset or an update). One component for every page that turns live
 * lighting on.
 */
export function LiveLightsNotice() {
  const t = useT();
  const kb = useActiveKeyboard();
  const off = liveOffReason(aaStatusOf(kb)?.liveLightsOff);
  if (!off) return null;
  // A reason the app has no words for gets the general line, not
  // "something went wrong".
  const body =
    off.reason && errorRule(off.reason).key !== "err.generic"
      ? friendlyError(off.reason, t, true)
      : t("aa.live.off.body");
  return (
    <InfoBar tone="warn" title={t("aa.live.off.title")} data="aa-live-off">
      <span className="block max-w-[80ch]">{body}</span>
    </InfoBar>
  );
}

/** "Beta" beside the firmware of an "AA" keyboard. */
export function BetaPill() {
  const t = useT();
  return (
    <span
      className="inline-flex h-[20px] items-center gap-1 rounded-full bg-signal/15 px-2 text-[11.5px] font-medium text-signal"
      data-aa-beta
    >
      <FlaskConical size={11} aria-hidden />
      {t("aa.beta")}
    </span>
  );
}

/** Support for this keyboard's firmware is new: what that means, and the
 *  GitHub form to say how it works. */
export function BetaNote({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  if (kb.protocol !== "ajazz-aa") return null;
  const issue = betaIssue(kb.modelName, kb.firmware);
  return (
    <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12.5px] text-ink-2" data-aa-beta-note>
      <BetaPill />
      <span>{t("aa.beta.body")}</span>
      <button
        type="button"
        className="text-signal underline-offset-2 hover:underline"
        onClick={() => openIssue("keyboard", issue.title, issue.model).catch(console.error)}
        data-aa-beta-tell
      >
        {t("aa.beta.tell")}
      </button>
    </p>
  );
}
