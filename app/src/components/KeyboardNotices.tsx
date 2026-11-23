// What the pages say about a keyboard of AJAZZ's "AA" family on the cable
// (the AK980 PRO on firmware 3.01, lib/aa.ts), one component for each
// thing, so every page says it the same way:
//
// - KeyboardNotices: nothing is written to it, and why; another program
//   seems to talk to it. On every page that writes to the keyboard.
// - LiveLightsNotice: why live lighting doesn't show on its keys, or that
//   its own lighting comes back about 30 seconds after live lighting ends.
//   On every page that turns live lighting on.
// - BetaPill / BetaNote: support for this firmware is new.

import { Cable, FlaskConical, Lightbulb, MessageSquareWarning, RadioTower } from "lucide-react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { aaStatusOf } from "../lib/aa";
import { betaIssue, untestedFirmware } from "../lib/aaText";
import { friendlyError } from "../lib/errors";
import { useT, type MessageKey } from "../lib/i18n";
import { openIssue } from "../lib/ipc";
import { useReport } from "../lib/report";
import { askFirmwareAgain } from "../lib/safety";
import { useActiveKeyboard, useApp } from "../lib/store";
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
  return (
    <>
      {ro && text && (
        <InfoBar
          tone="warn"
          title={t(text.title)}
          data={`aa-read-only-${ro.kind}`}
          actions={
            ro.kind === "not-sane" ? (
              <button
                type="button"
                className="btn-secondary"
                onClick={() =>
                  openReport({
                    kind: "bug",
                    title: `[Problem] ${kb.modelName} firmware ${kb.firmware ?? "?"}: only read, not as expected`,
                    context: `The keyboard does not look as expected: ${ro.reasons.join("; ") || "(no reason given)"}`,
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
          <span className="block max-w-[80ch]">{t(text.body)}</span>
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
 * Live lighting on an "AA" keyboard (plan K-4): it shows only in the
 * keyboard's per-key mode, which Keymotion switches to and back. Says why
 * it doesn't show on the keys, or, while it does, that the keyboard's own
 * lighting comes back about 30 seconds after it ends. One component for
 * every page that turns live lighting on.
 */
export function LiveLightsNotice() {
  const t = useT();
  const kb = useActiveKeyboard();
  const setPage = useApp((s) => s.setPage);
  const page = useApp((s) => s.page);
  const status = aaStatusOf(kb);
  if (!status) return null;
  const off = status.liveLightsOff;
  if (off) {
    const title = t("aa.live.off.title");
    switch (off.kind) {
      case "color-unknown":
        return (
          <InfoBar
            tone="warn"
            icon={<Lightbulb size={20} strokeWidth={1.6} />}
            title={title}
            data="aa-live-color"
            actions={
              page !== "lighting" ? (
                <button type="button" className="btn-secondary" onClick={() => setPage("lighting")}>
                  {t("overview.light.go")}
                </button>
              ) : undefined
            }
          >
            <span className="block max-w-[80ch]">{t("aa.live.color")}</span>
          </InfoBar>
        );
      case "read-only":
        return (
          <InfoBar tone="warn" title={title} data="aa-live-read-only">
            <span className="block max-w-[80ch]">{t("aa.live.readOnly")}</span>
          </InfoBar>
        );
      case "blocked": {
        const version = untestedFirmware(off.reason);
        return (
          <InfoBar
            tone="warn"
            title={title}
            data="aa-live-blocked"
            actions={
              version ? (
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={() => void askFirmwareAgain(version).catch(console.error)}
                  data-aa-ask-again
                >
                  {t("aa.live.askAgain")}
                </button>
              ) : undefined
            }
          >
            <span className="block max-w-[80ch]">
              {version ? t("aa.live.untested") : friendlyError(off.reason, t, true)}
            </span>
          </InfoBar>
        );
      }
      case "failed":
        return (
          <InfoBar tone="warn" title={title} data="aa-live-failed">
            <span className="block max-w-[80ch]">
              {t("aa.live.failed")} {friendlyError(off.error, t, true)}
            </span>
          </InfoBar>
        );
    }
  }
  if (!status.liveArmed) return null;
  return (
    <InfoBar icon={<Cable size={20} strokeWidth={1.6} />} title={t("aa.live.armed.title")} data="aa-live-armed">
      <span className="block max-w-[80ch]">{t("aa.live.armed.body")}</span>
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
