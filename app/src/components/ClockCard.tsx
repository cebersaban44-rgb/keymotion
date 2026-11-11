import { Clock3, Loader2, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import type { ClockStatus } from "../bindings/ClockStatus";
import { getClockStatus, onClockChanged, setClockAuto, syncClock } from "../lib/ipc";
import { useAgo, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { Toggle } from "./controls";
import { friendlyError } from "../lib/errors";

type SyncState = { kind: "idle" | "busy" | "ok" } | { kind: "error"; message: string };

/**
 * Keyboard clock: when it was last set, a manual sync, and the automatic
 * sync switch. `usb` is false while the keyboard is connected some other
 * way; the card then explains how to enable it.
 */
export function ClockCard({ usb }: { usb: boolean }) {
  const t = useT();
  const ago = useAgo();
  const locale = useApp((s) => s.locale);
  const [status, setStatus] = useState<ClockStatus | null>(null);
  const [sync, setSync] = useState<SyncState>({ kind: "idle" });
  // Re-render every 30 s so "x min ago" stays current.
  const [, tick] = useState(0);

  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    getClockStatus()
      .then((s) => alive && setStatus(s))
      .catch(console.error);
    onClockChanged((s) => alive && setStatus(s)).then((u) => (off = u));
    const timer = window.setInterval(() => tick((n) => n + 1), 30_000);
    return () => {
      alive = false;
      off?.();
      window.clearInterval(timer);
    };
  }, []);

  const syncNow = async () => {
    setSync({ kind: "busy" });
    try {
      setStatus(await syncClock());
      setSync({ kind: "ok" });
    } catch (e) {
      setSync({ kind: "error", message: friendlyError(e, t) });
    }
  };

  const toggleAuto = async (on: boolean) => {
    try {
      setStatus(await setClockAuto(on));
    } catch (e) {
      setSync({ kind: "error", message: friendlyError(e, t) });
    }
  };

  const last = status?.lastSyncMs ?? null;
  const lastText = last === null ? t("clock.never") : t("clock.lastAt", { ago: ago(last) });
  const lastDate =
    last === null
      ? undefined
      : new Date(last).toLocaleString(locale === "tr" ? "tr-TR" : "en-US", { dateStyle: "medium", timeStyle: "short" });

  return (
    <div className="card flex flex-wrap items-center gap-x-5 gap-y-4 p-5" data-clock>
      <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg bg-surface-2 text-ink-2">
        <Clock3 size={20} strokeWidth={1.8} />
      </div>

      <div className="min-w-[220px] flex-1">
        <div className="eyebrow">{t("clock.title")}</div>
        <div className="num mt-1 text-[13px] text-ink-2" title={lastDate}>
          {status ? lastText : "…"}
        </div>
        <p className="mt-1 text-[12px] leading-relaxed text-ink-3">
          {usb ? t("clock.note") : t("clock.needsUsb")}
        </p>
        <div aria-live="polite" className="text-[12px]">
          {sync.kind === "ok" && <span className="text-ok">{t("clock.synced")}</span>}
          {sync.kind === "error" && <span className="text-danger">{sync.message}</span>}
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-3">
        <div className="text-right">
          <div className="text-[13px] font-medium">{t("clock.auto")}</div>
          <div className="max-w-[26ch] text-[11px] leading-snug text-ink-3">{t("clock.autoHint")}</div>
        </div>
        <Toggle
          label={t("clock.auto")}
          checked={status?.autoSync ?? true}
          disabled={!status}
          onChange={(on) => void toggleAuto(on)}
        />
      </div>

      <button
        type="button"
        onClick={() => void syncNow()}
        disabled={!usb || sync.kind === "busy"}
        className="flex h-9 shrink-0 items-center gap-2 rounded-lg border border-line px-3 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink disabled:cursor-not-allowed disabled:opacity-40"
      >
        {sync.kind === "busy" ? <Loader2 size={15} className="animate-spin" /> : <RefreshCw size={14} />}
        {sync.kind === "busy" ? t("clock.syncing") : t("clock.sync")}
      </button>
    </div>
  );
}
