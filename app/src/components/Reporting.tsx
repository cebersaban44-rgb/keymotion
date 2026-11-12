import { ArrowUpRight, Check, Copy, Loader2, MessageSquareWarning, TriangleAlert, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { CrashNote } from "../bindings/CrashNote";
import { copyText } from "../lib/diagnostics";
import { useT } from "../lib/i18n";
import { crashPending, crashSeen, getDiagnostics, openIssue, unsupportedReport } from "../lib/ipc";
import { crashTitle, problemTitle, useReport, type ReportRequest } from "../lib/report";
import { useApp } from "../lib/store";

/** The report dialog and the notices that offer it: an unexpected error,
 *  and a crash note from an earlier run. Top right, clear of the save bars
 *  at the bottom of the pages. */
export function Reporting() {
  const request = useReport((s) => s.request);
  const closeReport = useReport((s) => s.closeReport);
  return (
    <>
      <div className="pointer-events-none fixed top-5 right-6 z-40 flex w-[360px] flex-col gap-3">
        <LastRunNotice />
        <ProblemNotice />
      </div>
      <AnimatePresence>{request && <ReportDialog request={request} onClose={closeReport} />}</AnimatePresence>
    </>
  );
}

const noticeClass = "card pointer-events-auto flex flex-col gap-2.5 p-4 shadow-xl";

/** "Something unexpected went wrong", once per error; it goes after a while. */
function ProblemNotice() {
  const t = useT();
  const problem = useReport((s) => s.problem);
  const dismiss = useReport((s) => s.dismissProblem);
  const openReport = useReport((s) => s.openReport);
  const [held, setHeld] = useState(false);
  useEffect(() => {
    if (!problem || held) return;
    const id = window.setTimeout(dismiss, 20_000);
    return () => window.clearTimeout(id);
  }, [problem, held, dismiss]);
  return (
    <AnimatePresence>
      {problem && (
        <motion.div
          key="problem"
          initial={{ opacity: 0, y: -8 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -8 }}
          className={noticeClass}
          role="status"
          data-problem-notice
          onMouseEnter={() => setHeld(true)}
          onMouseLeave={() => setHeld(false)}
          onFocus={() => setHeld(true)}
        >
          <NoticeHead title={t("report.problem.title")} onClose={dismiss} />
          <p className="text-[12.5px] text-ink-2">{t("report.problem.body")}</p>
          <button
            type="button"
            onClick={() => openReport({ kind: "bug", title: problemTitle(problem), context: problem })}
            className="self-start text-[12.5px] font-medium text-signal hover:underline"
          >
            {t("report.problem.report")}
          </button>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/** A crash note from an earlier run, offered once at start. */
function LastRunNotice() {
  const t = useT();
  const openReport = useReport((s) => s.openReport);
  const [note, setNote] = useState<CrashNote | null>(null);
  useEffect(() => {
    crashPending().then(setNote, console.error);
  }, []);
  const done = () => {
    if (note) crashSeen(note.name).catch(console.error);
    setNote(null);
  };
  return (
    <AnimatePresence>
      {note && (
        <motion.div
          key="crash"
          initial={{ opacity: 0, y: -8 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -8 }}
          className={noticeClass}
          role="status"
          data-crash-notice
        >
          <NoticeHead title={t("report.crash.title")} onClose={done} />
          <p className="text-[12.5px] text-ink-2">{t("report.crash.body")}</p>
          <button
            type="button"
            onClick={() => {
              openReport({ kind: "bug", title: crashTitle(note.text), context: note.text });
              done();
            }}
            className="self-start text-[12.5px] font-medium text-signal hover:underline"
          >
            {t("report.crash.report")}
          </button>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function NoticeHead({ title, onClose }: { title: string; onClose: () => void }) {
  const t = useT();
  return (
    <div className="flex items-start justify-between gap-2">
      <span className="flex items-center gap-2 text-[13px] font-semibold">
        <TriangleAlert size={14} className="shrink-0 text-warn" aria-hidden />
        {title}
      </span>
      <button
        type="button"
        onClick={onClose}
        aria-label={t("report.close")}
        className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-ink-3 hover:bg-surface-2 hover:text-ink"
      >
        <X size={14} />
      </button>
    </div>
  );
}

/**
 * What will be sent, shown first. "Report on GitHub" copies it and opens the
 * form (the text is too long for the address, so it is pasted); "Copy" is
 * there for anyone who'd rather send it another way.
 */
export function ReportDialog({ request, onClose }: { request: ReportRequest; onClose: () => void }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const [text, setText] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [done, setDone] = useState<"no" | "copied" | "opened" | "failed">("no");
  const primary = useRef<HTMLButtonElement>(null);
  const keyboard = request.kind === "keyboard";

  useEffect(() => {
    let gone = false;
    const load = keyboard ? unsupportedReport(request.keyboardId ?? "") : getDiagnostics(locale, request.context);
    load.then(
      (v) => !gone && setText(v),
      (e) => {
        console.error(e);
        if (!gone) setFailed(true);
      },
    );
    return () => {
      gone = true;
    };
  }, [keyboard, request.keyboardId, request.context, locale]);
  useEffect(() => {
    if (text !== null) primary.current?.focus();
  }, [text]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const copy = async (then: "copied" | "opened") => {
    if (text === null) return;
    try {
      await copyText(text);
      if (then === "opened") await openIssue(request.kind, request.title, request.model);
      setDone(then);
    } catch (e) {
      console.error(e);
      setDone("failed");
    }
  };

  return (
    <motion.div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-6 backdrop-blur-[2px]"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        role="dialog"
        aria-modal="true"
        aria-labelledby="report-title"
        aria-describedby="report-body"
        className="card flex max-h-full w-full max-w-[560px] flex-col gap-4 p-6"
        initial={{ opacity: 0, y: 8, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 4, scale: 0.98 }}
        transition={{ duration: 0.18, ease: "easeOut" }}
        data-report-dialog
      >
        <div className="flex items-start gap-4">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-signal/15 text-signal">
            <MessageSquareWarning size={19} />
          </span>
          <div className="flex flex-col gap-1.5">
            <h2 id="report-title" className="font-display text-[20px] leading-tight font-semibold">
              {t(keyboard ? "report.keyboard.title" : "report.title")}
            </h2>
            <p id="report-body" className="text-[13px] leading-relaxed text-ink-2">
              {t(keyboard ? "report.keyboard.body" : "report.body")}
            </p>
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <span className="text-[12px] text-ink-3">{t("report.included")}</span>
          {text !== null ? (
            <pre
              tabIndex={0}
              aria-label={t("report.included")}
              className="num max-h-[260px] min-h-[120px] overflow-auto rounded-lg border border-line bg-bg-raise p-3 text-[11.5px] leading-relaxed whitespace-pre-wrap text-ink-2 [overflow-wrap:anywhere]"
              data-report-text
            >
              {text}
            </pre>
          ) : (
            <div className="flex min-h-[120px] items-center justify-center rounded-lg border border-line bg-bg-raise text-[12.5px] text-ink-3">
              {failed ? (
                <span className="text-danger">{t("report.loadFailed")}</span>
              ) : (
                <Loader2 size={16} className="animate-spin" aria-label={t("report.loading")} />
              )}
            </div>
          )}
        </div>

        <p className="text-[12.5px] text-ink-2" role="status">
          {done === "opened"
            ? t("report.opened")
            : done === "copied"
              ? t("report.copied")
              : done === "failed"
                ? t("report.failed")
                : t("report.account")}
        </p>

        <div className="flex flex-wrap items-center justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="h-9 rounded-lg px-3.5 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
          >
            {t("report.close")}
          </button>
          <button
            type="button"
            onClick={() => copy("copied")}
            disabled={text === null}
            className="flex h-9 items-center gap-2 rounded-lg border border-line bg-bg-raise px-3.5 text-[13px] text-ink transition-colors hover:border-line-strong disabled:opacity-50"
          >
            {done === "copied" ? <Check size={14} /> : <Copy size={14} />}
            {t("report.copy")}
          </button>
          <button
            ref={primary}
            type="button"
            onClick={() => copy("opened")}
            disabled={text === null}
            className="flex h-9 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink transition-transform active:scale-[0.98] disabled:opacity-50"
            data-report-send
          >
            {t("report.send")}
            <ArrowUpRight size={14} aria-hidden />
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
