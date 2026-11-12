import { MessageSquareWarning, RotateCw, TriangleAlert } from "lucide-react";
import { AnimatePresence } from "motion/react";
import { Component, useState, type ErrorInfo, type ReactNode } from "react";
import { useT } from "../lib/i18n";
import { logUiError } from "../lib/ipc";
import { problemTitle } from "../lib/report";
import { ReportDialog } from "./Reporting";

interface Props {
  children: ReactNode;
  /** Around the whole window (nothing else is left to use) or one page. */
  full?: boolean;
}

interface State {
  failed: boolean;
  /** The error and where it happened, for a report. */
  detail: string;
}

/**
 * A drawing error leaves a calm message instead of an empty window: what
 * happened, "Reload", and "Report the problem". The error and where it
 * happened go to the app's log, which the report carries.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false, detail: "" };

  static getDerivedStateFromError(): Partial<State> {
    return { failed: true };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    const what = error instanceof Error ? `${error.name}: ${error.message}` : String(error);
    const where = (info.componentStack ?? "")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean)
      .slice(0, 6)
      .join(" < ");
    console.error(error);
    void logUiError(`page crashed: ${what} [${where}]`);
    this.setState({ detail: `page crashed: ${what}\n${where}` });
  }

  render() {
    return this.state.failed ? <CrashNotice full={this.props.full} detail={this.state.detail} /> : this.props.children;
  }
}

function CrashNotice({ full, detail }: { full?: boolean; detail: string }) {
  const t = useT();
  // Its own dialog: when the whole window failed, nothing else is left.
  const [reporting, setReporting] = useState(false);
  return (
    <div className={`flex items-center justify-center p-6 ${full ? "h-full bg-bg" : "min-h-[60vh]"}`}>
      <div role="alert" className="card flex w-full max-w-[460px] flex-col gap-5 p-6">
        <div className="flex items-start gap-4">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-warn/15 text-warn">
            <TriangleAlert size={19} />
          </span>
          <div className="flex flex-col gap-1.5">
            <h1 className="font-display text-[20px] leading-tight font-semibold">{t("crash.title")}</h1>
            <p className="text-[13px] leading-relaxed text-ink-2">{t("crash.body")}</p>
          </div>
        </div>
        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={() => setReporting(true)}
            className="flex h-9 items-center gap-2 rounded-lg border border-line bg-bg-raise px-3.5 text-[13px] text-ink transition-colors hover:border-line-strong"
          >
            <MessageSquareWarning size={14} />
            {t("report.problem.report")}
          </button>
          <button
            type="button"
            autoFocus
            onClick={() => location.reload()}
            className="flex h-9 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink transition-transform active:scale-[0.98]"
          >
            <RotateCw size={14} />
            {t("crash.reload")}
          </button>
        </div>
      </div>
      <AnimatePresence>
        {reporting && (
          <ReportDialog
            request={{ kind: "bug", title: problemTitle(detail), context: detail }}
            onClose={() => setReporting(false)}
          />
        )}
      </AnimatePresence>
    </div>
  );
}
