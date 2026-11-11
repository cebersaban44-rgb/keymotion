import { Check, Copy, RotateCw, TriangleAlert } from "lucide-react";
import { Component, useState, type ErrorInfo, type ReactNode } from "react";
import { copyDiagnostics } from "../lib/diagnostics";
import { useT } from "../lib/i18n";
import { logUiError } from "../lib/ipc";

interface Props {
  children: ReactNode;
  /** Around the whole window (nothing else is left to use) or one page. */
  full?: boolean;
}

interface State {
  failed: boolean;
}

/**
 * A drawing error leaves a calm message instead of an empty window: what
 * happened, "Reload", and "Copy diagnostics" for a report. The error and
 * where it happened go to the app's log, which the diagnostics carry.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
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
  }

  render() {
    return this.state.failed ? <CrashNotice full={this.props.full} /> : this.props.children;
  }
}

function CrashNotice({ full }: { full?: boolean }) {
  const t = useT();
  const [copied, setCopied] = useState<"no" | "yes" | "failed">("no");
  const copy = () =>
    copyDiagnostics().then(
      () => setCopied("yes"),
      (e) => {
        console.error(e);
        setCopied("failed");
      },
    );
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
        {copied === "yes" && <p className="text-[13px] text-ink-2">{t("data.diag.copied")}</p>}
        {copied === "failed" && <p className="text-[13px] text-danger">{t("data.diag.failed")}</p>}
        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={copy}
            className="flex h-9 items-center gap-2 rounded-lg border border-line bg-bg-raise px-3.5 text-[13px] text-ink transition-colors hover:border-line-strong"
          >
            {copied === "yes" ? <Check size={14} /> : <Copy size={14} />}
            {t("data.diag.copy")}
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
    </div>
  );
}
