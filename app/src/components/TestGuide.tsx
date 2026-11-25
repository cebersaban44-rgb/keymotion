// The test guide for a keyboard of AJAZZ's "AA" family on the cable (the
// AK980 PRO on firmware 3.01; lib/aaGuide.ts, lib/aaGuideRules.ts), in its
// three places:
//
// - GuideInvite: once on the Overview, a strip that keeps the page usable
//   (never a dialog): "Start the test", "Not now", "Don't show again".
// - GuideEntry: always on the Connection page, beside "In beta": how far
//   the guide got, and the button that opens it.
// - GuidePanel: the guide itself, beside every page (App). Each step says
//   what to do, opens the page that does it ("Go there") and asks how it
//   went; Cancel at every step keeps the progress. After the last step, the
//   summary shows the exact text to share, which the user copies; GitHub's
//   form opens with its title and model filled in (only its text fields
//   take values from the address), and the panel names the answer to pick
//   for its question. On a keyboard Keymotion can't write to now, the panel
//   says the saving steps can't work and offers a report.
//
// Nothing here writes to the keyboard: the user saves on the pages.

import { ArrowRight, ArrowUpRight, Check, Copy, FlaskConical, Loader2, MessageSquareWarning, TriangleAlert, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import {
  answerStep,
  checkFacts,
  closeGuide,
  goThere,
  inviteLater,
  inviteNever,
  markShared,
  noteStep,
  openGuide,
  showStep,
  useGuide,
  useGuideInvite,
  GUIDE_WIDTH,
} from "../lib/aaGuide";
import { aaStatusOf } from "../lib/aa";
import {
  FORM_DETAILS,
  FORM_QUESTION,
  GUIDE_CHECK,
  GUIDE_GO,
  GUIDE_STEPS,
  NOTE_MAX,
  SUMMARY,
  answered,
  checkShown,
  failedSteps,
  formAnswer,
  guideIssue,
  guideState,
  nextAt,
  sharedKeyboard,
  sharedText,
  type GuideGo,
  type GuideResult,
  type GuideState,
  type GuideStep,
} from "../lib/aaGuideRules";
import { copyText } from "../lib/diagnostics";
import { useT, type MessageKey, type Translate } from "../lib/i18n";
import { getDiagnostics, openIssue } from "../lib/ipc";
import { useReport } from "../lib/report";
import { useActiveKeyboard, useApp } from "../lib/store";

const TITLE: Record<GuideStep, MessageKey> = {
  prepare: "guide.s.prepare.title",
  connection: "guide.s.connection.title",
  effect: "guide.s.effect.title",
  live: "guide.s.live.title",
  perKey: "guide.s.perKey.title",
  keymap: "guide.s.keymap.title",
  macros: "guide.s.macros.title",
  screen: "guide.s.screen.title",
  restore: "guide.s.restore.title",
};

const BODY: Record<GuideStep, MessageKey> = {
  prepare: "guide.s.prepare.body",
  connection: "guide.s.connection.body",
  effect: "guide.s.effect.body",
  live: "guide.s.live.body",
  perKey: "guide.s.perKey.body",
  keymap: "guide.s.keymap.body",
  macros: "guide.s.macros.body",
  screen: "guide.s.screen.body",
  restore: "guide.s.restore.body",
};

/** What Keymotion's own check on a step says (steps with one). */
const CHECKED: Partial<Record<GuideStep, MessageKey>> = {
  prepare: "guide.s.prepare.check",
  connection: "guide.s.connection.check",
  effect: "guide.s.effect.check",
  perKey: "guide.s.perKey.check",
  keymap: "guide.s.keymap.check",
  macros: "guide.s.macros.check",
  restore: "guide.s.restore.check",
};

const RESULTS: GuideResult[] = ["worked", "failed", "skipped"];
const RESULT: Record<GuideResult, MessageKey> = { worked: "guide.r.worked", failed: "guide.r.failed", skipped: "guide.r.skipped" };

const STATE: Record<GuideState, MessageKey> = {
  new: "guide.state.new",
  partial: "guide.state.partial",
  done: "guide.state.done",
  shared: "guide.state.shared",
};

/** Where "Go there" leads, in the words of the sidebar and the tabs. */
function goLabel(t: Translate, go: GuideGo): string {
  switch (go.page) {
    case "connection":
      return go.backups ? `${t("nav.connection")} › ${t("aa.bk.title")}` : t("nav.connection");
    case "lighting":
      return `${t("nav.lighting")} › ${t(go.tab === "effects" ? "light.tab.effects" : "light.tab.perKey")}`;
    case "screen":
      return `${t("nav.screen")} › ${t("sc.tab.visuals")}`;
    case "music":
      return t("nav.music");
    case "keymap":
      return t("nav.keymap");
    case "macros":
      return t("nav.macros");
  }
}

// ------------------------------------------------------------ invitation

/** Once on the Overview, for an "AA" keyboard on the cable that Keymotion
 *  can write to (lib/aaGuideRules.ts `inviteShown`). */
export function GuideInvite({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const shown = useGuideInvite();
  // What the answer did, until the page is left.
  const [after, setAfter] = useState<"later" | "never" | null>(null);
  if (after) {
    return (
      <p className="text-[12.5px] text-ink-2" role="status" data-guide-invite-after={after}>
        {t(after === "later" ? "guide.invite.laterNote" : "guide.invite.neverNote")}
      </p>
    );
  }
  if (!shown) return null;
  return (
    <section className="infobar !items-start" data-infobar="aa-guide-invite" aria-labelledby="guide-invite-title">
      <span className="flex shrink-0 pt-0.5 text-signal" aria-hidden>
        <FlaskConical size={20} strokeWidth={1.6} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <h2 id="guide-invite-title" className="text-[14px] font-semibold">
          {t("guide.invite.title")}
        </h2>
        <p className="max-w-[80ch] text-[13px] text-ink-2">{t("guide.invite.body", { firmware: kb.firmware ?? "3.01" })}</p>
        <div className="mt-2 flex flex-wrap gap-2">
          <button type="button" className="btn-primary" onClick={openGuide} data-guide-start>
            {t("guide.invite.start")}
          </button>
          <button type="button" className="btn-secondary" onClick={() => setAfter(inviteLater() ? "never" : "later")} data-guide-later>
            {t("guide.invite.later")}
          </button>
          <button
            type="button"
            className="btn-subtle"
            onClick={() => {
              inviteNever();
              setAfter("never");
            }}
            data-guide-never
          >
            {t("guide.invite.never")}
          </button>
        </div>
      </div>
    </section>
  );
}

// ------------------------------------------------------- Connection page

/** On the Connection page of an "AA" keyboard on the cable: the guide's
 *  state and the button that opens it, whatever the invitation got. */
export function GuideEntry({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const progress = useGuide((s) => s.progress);
  if (kb.protocol !== "ajazz-aa" || kb.connection !== "usb") return null;
  const state = guideState(progress);
  const done = answered(progress);
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2" data-guide-entry={state}>
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="text-[14px] font-medium">{t("guide.title")}</span>
        <span className="text-[13px] text-ink-2">{t(STATE[state], { done, count: GUIDE_STEPS.length })}</span>
      </div>
      <button type="button" className="btn-secondary" onClick={openGuide} data-guide-open>
        <FlaskConical size={14} aria-hidden />
        {t(state === "done" ? "guide.share" : "guide.open")}
      </button>
    </div>
  );
}

// ------------------------------------------------------------- the panel

/** The guide beside the page, while it is open, and the note after Cancel. */
export function GuidePanel() {
  const open = useGuide((s) => s.open);
  return (
    <>
      <AnimatePresence>{open && <Panel key="guide" />}</AnimatePresence>
      <ClosedNotice />
    </>
  );
}

function Panel() {
  const t = useT();
  const at = useGuide((s) => s.progress.at);
  const attention = useGuide((s) => s.attention);
  const step = GUIDE_STEPS[at];
  const box = useRef<HTMLElement>(null);
  const [flash, setFlash] = useState(false);
  // "Open the guide" while it is open: its heading takes the focus and the
  // panel lights up for a moment.
  useEffect(() => {
    if (attention === 0) return;
    box.current?.querySelector<HTMLElement>("[data-guide-step]")?.focus();
    setFlash(true);
    const id = window.setTimeout(() => setFlash(false), 900);
    return () => window.clearTimeout(id);
  }, [attention]);
  return (
    <motion.aside
      ref={box}
      className={`guide-pane flex h-full ${GUIDE_WIDTH} shrink-0 flex-col transition-shadow ${flash ? "shadow-[inset_0_0_0_2px_var(--signal)]" : ""}`}
      aria-label={t("guide.title")}
      initial={{ opacity: 0, x: 16 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: 16 }}
      transition={{ duration: 0.18 }}
      data-guide-panel={step ?? "summary"}
    >
      <header className="flex items-center gap-2 px-5 pt-5 pb-2">
        <FlaskConical size={16} className="text-signal" aria-hidden />
        <span className="text-[13px] font-semibold text-ink-2">{t("guide.title")}</span>
      </header>
      <ReadOnlyNote />
      {step ? <StepView key={at} step={step} at={at} /> : <Summary />}
    </motion.aside>
  );
}

/** The keyboard takes no writes now: the steps that save can't work. Says
 *  so plainly, and offers a report. */
function ReadOnlyNote() {
  const t = useT();
  const kb = useActiveKeyboard();
  const openReport = useReport((s) => s.openReport);
  const ro = kb ? aaStatusOf(kb)?.readOnly : null;
  if (!kb || !ro) return null;
  const reasons = ro.kind === "not-sane" ? ro.reasons : [ro.error];
  return (
    <div className="mx-5 mb-2 flex flex-col gap-2 rounded-lg bg-warn/12 p-3 text-[13px]" role="status" data-guide-read-only>
      <span className="flex items-start gap-2">
        <TriangleAlert size={15} className="mt-0.5 shrink-0 text-warn" aria-hidden />
        {t("guide.readOnly")}
      </span>
      <button
        type="button"
        className="btn-secondary btn-sm self-start"
        onClick={() =>
          openReport({
            kind: "bug",
            title: `[Problem] ${kb.modelName} firmware ${kb.firmware ?? "?"}: the test guide can't save`,
            context: `Keymotion writes nothing to the keyboard (${ro.kind}):\n${reasons.map((r) => `- ${r}`).join("\n") || "(no reason given)"}`,
          })
        }
        data-guide-report
      >
        <MessageSquareWarning size={14} aria-hidden />
        {t("report.settings.problem")}
      </button>
    </div>
  );
}

function StepView({ step, at }: { step: GuideStep; at: number }) {
  const t = useT();
  const progress = useGuide((s) => s.progress);
  const kb = useActiveKeyboard();
  // Keyed by the step: its error starts empty.
  const [error, setError] = useState(false);
  const title = useRef<HTMLHeadingElement>(null);
  // Each step's title takes the focus (read out).
  useEffect(() => {
    title.current?.focus({ preventScroll: true });
  }, []);
  const result = progress.results[step] ?? null;
  const go = GUIDE_GO[step];
  const checkKey = GUIDE_CHECK[step] !== null ? CHECKED[step] : undefined;
  const ticked = checkKey !== undefined && checkShown(progress, step, checkFacts(kb));
  const last = at === GUIDE_STEPS.length - 1;
  const next = () => {
    const to = nextAt(progress);
    if (to === null) setError(true);
    else showStep(to);
  };
  return (
    <>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4">
        <ol className="mb-3 flex gap-1" aria-hidden>
          {GUIDE_STEPS.map((s, i) => (
            <li
              key={s}
              className={`h-1 flex-1 rounded-full ${progress.results[s] ? "bg-ok" : i === at ? "bg-signal" : "bg-[var(--line-strong)]"}`}
            />
          ))}
        </ol>
        <p className="text-[12px] text-ink-3">{t("guide.step", { n: at + 1, count: GUIDE_STEPS.length })}</p>
        <h2 ref={title} tabIndex={-1} className="mt-1 text-[16px] font-semibold outline-none" data-guide-step={step}>
          {t(TITLE[step])}
        </h2>
        <p className="mt-1.5 text-[13.5px] leading-relaxed text-ink-2">{t(BODY[step])}</p>
        {ticked && checkKey && (
          <p className="mt-2.5 flex items-start gap-1.5 text-[13px] text-ok" data-guide-check={step}>
            <Check size={15} strokeWidth={2.5} className="mt-0.5 shrink-0" aria-hidden />
            <span>{t("guide.checked", { what: t(checkKey) })}</span>
          </p>
        )}
        {go && (
          <div className="mt-3.5 flex flex-wrap items-center gap-x-3 gap-y-1">
            <button type="button" className="btn-secondary btn-sm" onClick={() => goThere(go)} data-guide-go>
              {t("guide.go")}
              <ArrowRight size={14} aria-hidden />
            </button>
            <span className="text-[12px] text-ink-3">{goLabel(t, go)}</span>
          </div>
        )}
        <p id="guide-result" className="mt-5 mb-1.5 text-[13px] text-ink-2">
          {t("guide.result")}
        </p>
        <div role="radiogroup" aria-labelledby="guide-result" className="seg">
          {RESULTS.map((r) => (
            <button
              key={r}
              type="button"
              role="radio"
              aria-checked={result === r}
              onClick={() => {
                answerStep(step, r);
                setError(false);
              }}
              data-guide-result={r}
            >
              {t(RESULT[r])}
            </button>
          ))}
        </div>
        <textarea
          rows={2}
          maxLength={NOTE_MAX}
          value={progress.notes[step] ?? ""}
          onChange={(e) => noteStep(step, e.target.value)}
          placeholder={t("guide.note.placeholder")}
          aria-label={t("guide.note")}
          className="mt-3 w-full resize-none rounded-lg border border-line bg-bg-raise px-3 py-2 text-[13px]"
          data-guide-note
        />
        {error && (
          <p role="alert" className="mt-1.5 text-[13px] text-danger" data-guide-error>
            {t("guide.pick")}
          </p>
        )}
      </div>
      <footer className="flex items-center justify-between gap-2 border-t border-line px-5 py-3">
        <button type="button" className="btn-subtle" onClick={() => closeGuide(true)} data-guide-cancel>
          {t("guide.cancel")}
        </button>
        <div className="flex gap-2">
          {at > 0 && (
            <button type="button" className="btn-secondary" onClick={() => showStep(at - 1)} data-guide-back>
              {t("guide.back")}
            </button>
          )}
          <button type="button" className="btn-primary" onClick={next} data-guide-next>
            {t(last ? "guide.finish" : "guide.next")}
          </button>
        </div>
      </footer>
    </>
  );
}

/** After the last step: the results, then the exact text to share. */
function Summary() {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const progress = useGuide((s) => s.progress);
  const kb = useActiveKeyboard();
  const [report, setReport] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  // Bumped by "Try again" after the text couldn't be put together.
  const [tries, setTries] = useState(0);
  const [done, setDone] = useState<"no" | "copied" | "opened" | "failed">("no");
  const title = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    title.current?.focus({ preventScroll: true });
  }, []);
  useEffect(() => {
    let gone = false;
    getDiagnostics(locale).then(
      (r) => {
        if (gone) return;
        setReport(r);
        setFailed(false);
      },
      (e) => {
        console.error(e);
        if (!gone) setFailed(true);
      },
    );
    return () => {
      gone = true;
    };
  }, [locale, tries]);

  const facts = checkFacts(kb);
  const checks = Object.fromEntries(GUIDE_STEPS.map((s) => [s, checkShown(progress, s, facts)]));
  // The keyboard the answers were collected on.
  const { model, firmware } = sharedKeyboard(progress, kb?.modelName ?? "AJAZZ AK980 PRO", kb?.firmware ?? null);
  const text = report === null ? null : sharedText({ model, firmware, progress, checks, report });
  const bad = failedSteps(progress);

  const share = async (then: "copied" | "opened") => {
    if (text === null) return;
    try {
      await copyText(text);
      markShared();
      if (then === "opened") {
        const issue = guideIssue(model, firmware);
        await openIssue("keyboard", issue.title, issue.model);
      }
      setDone(then);
    } catch (e) {
      console.error(e);
      setDone("failed");
    }
  };

  return (
    <>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4">
        <h2 ref={title} tabIndex={-1} className="text-[16px] font-semibold outline-none" data-guide-step="summary">
          {t("guide.done.title")}
        </h2>
        <p className="mt-1 text-[13.5px] text-ink-2">
          {bad > 0 ? t("guide.done.failed", { count: bad }) : t("guide.done.ok")}
        </p>
        <ol className="mt-3 flex flex-col text-[13px]" data-guide-results>
          {GUIDE_STEPS.map((s, i) => {
            const r = progress.results[s];
            return (
              <li key={s} className="flex items-center justify-between gap-3 border-t border-line py-1.5 first:border-t-0">
                <span className="min-w-0">
                  {i + 1}. {t(TITLE[s])}
                </span>
                <span className={`shrink-0 ${r === "failed" ? "text-danger" : r === "worked" ? "text-ok" : "text-ink-3"}`}>
                  {r ? t(RESULT[r]) : "—"}
                </span>
              </li>
            );
          })}
        </ol>
        <h3 className="mt-5 text-[14px] font-semibold">{t("guide.text.title")}</h3>
        <p className="mt-1 text-[12.5px] text-ink-2">{t("guide.text.body")}</p>
        {text !== null ? (
          <pre
            tabIndex={0}
            aria-label={t("guide.text.title")}
            className="num mt-2 max-h-[240px] overflow-auto rounded-lg border border-line bg-bg-raise p-3 text-[11.5px] leading-relaxed whitespace-pre-wrap text-ink-2 [overflow-wrap:anywhere]"
            data-guide-text
          >
            {text}
          </pre>
        ) : (
          <div className="mt-2 flex min-h-[100px] items-center justify-center rounded-lg border border-line bg-bg-raise text-[12.5px] text-ink-3">
            {failed ? (
              <span className="flex flex-col items-center gap-2 text-center" role="alert" data-guide-text-failed>
                <span className="text-danger">{t("guide.text.failed")}</span>
                <button type="button" className="btn-secondary btn-sm" onClick={() => setTries((n) => n + 1)}>
                  {t("guide.text.retry")}
                </button>
              </span>
            ) : (
              <Loader2 size={16} className="animate-spin" aria-label={t("report.loading")} />
            )}
          </div>
        )}
        <div className="mt-3 flex flex-wrap gap-2">
          <button type="button" className="btn-primary" disabled={text === null} onClick={() => share("opened")} data-guide-send>
            {t("guide.send")}
            <ArrowUpRight size={14} aria-hidden />
          </button>
          <button type="button" className="btn-secondary" disabled={text === null} onClick={() => share("copied")} data-guide-copy>
            {done === "copied" ? <Check size={14} aria-hidden /> : <Copy size={14} aria-hidden />}
            {t("guide.copy")}
          </button>
        </div>
        <p className="mt-3 text-[12.5px] text-ink-2" data-guide-form>
          {t("guide.form", { question: FORM_QUESTION, answer: formAnswer(progress), box: FORM_DETAILS })}
        </p>
        <p className="mt-2 text-[12.5px] text-ink-2" role="status" data-guide-sent={done}>
          {done === "opened"
            ? t("guide.sent.opened")
            : done === "copied"
              ? t("guide.sent.copied")
              : done === "failed"
                ? t("report.failed")
                : t("report.account")}
        </p>
      </div>
      <footer className="flex items-center justify-between gap-2 border-t border-line px-5 py-3">
        <button type="button" className="btn-secondary" onClick={() => showStep(SUMMARY - 1)} data-guide-back>
          {t("guide.back")}
        </button>
        <button type="button" className="btn-primary" onClick={() => closeGuide(false)} data-guide-close>
          {t("report.close")}
        </button>
      </footer>
    </>
  );
}

/** "The guide is closed; your progress is kept", for a few seconds after
 *  Cancel: where to carry on. */
function ClosedNotice() {
  const t = useT();
  const shown = useGuide((s) => s.closedNotice);
  useEffect(() => {
    if (!shown) return;
    const id = window.setTimeout(() => useGuide.setState({ closedNotice: false }), 8000);
    return () => window.clearTimeout(id);
  }, [shown]);
  return (
    <AnimatePresence>
      {shown && (
        <motion.div
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 12 }}
          className="flyout fixed right-6 bottom-6 z-40 flex w-[340px] items-start gap-2 p-4"
          role="status"
          data-guide-closed
        >
          <FlaskConical size={14} className="mt-0.5 shrink-0 text-signal" aria-hidden />
          <p className="flex-1 text-[13px]">{t("guide.closed")}</p>
          <button
            type="button"
            onClick={() => useGuide.setState({ closedNotice: false })}
            aria-label={t("report.close")}
            className="btn-subtle btn-sm !px-2"
          >
            <X size={14} />
          </button>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
