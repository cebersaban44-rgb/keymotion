// "Canlı ekran": the live mode switch, its upload budget and the sources
// (phase C). The backend runs the live mode; this only shows and sets it.

import { AlertTriangle, Clock3, Loader2, Music2, Radio, Upload } from "lucide-react";
import type { LivePlan } from "../../bindings/LivePlan";
import type { LiveState } from "../../bindings/LiveState";
import { Segmented, Toggle } from "../../components/controls";
import { useT, type MessageKey } from "../../lib/i18n";
import { fmtWait, useLive } from "../../lib/live";
import { Hint, useNow } from "./common";

const PLANS: { id: LivePlan; label: MessageKey; years: MessageKey }[] = [
  { id: "saver", label: "sc.live.plan.saver", years: "sc.live.years.saver" },
  { id: "balanced", label: "sc.live.plan.balanced", years: "sc.live.years.balanced" },
  { id: "heavy", label: "sc.live.plan.heavy", years: "sc.live.years.heavy" },
];

export function LivePanel({ usb }: { usb: boolean }) {
  const t = useT();
  const s = useLive((x) => x.state);
  const drawError = useLive((x) => x.drawError);
  const configure = useLive((x) => x.configure);
  useNow(); // refresh the waiting times
  if (!s) return null;
  const plan = PLANS.find((p) => p.id === s.plan) ?? PLANS[1]!;
  const set = (patch: Parameters<typeof configure>[0]) => configure(patch).catch(console.error);

  return (
    <section className="card flex flex-col gap-5 p-5" data-live-panel>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="max-w-[72ch]">
          <div className="mb-1.5 flex items-center gap-2">
            <Radio size={14} className={s.enabled ? "text-ok" : "text-ink-3"} />
            <span className="eyebrow">{t("sc.live.eyebrow")}</span>
          </div>
          <h2 className="font-display text-[22px] font-semibold tracking-[-0.02em]">{t("sc.live.title")}</h2>
          <p className="mt-1 text-[12.5px] text-ink-2">{t("sc.live.body")}</p>
        </div>
        <label className="flex items-center gap-3 rounded-xl border border-line px-3 py-2">
          <span className="text-[13px] font-semibold">{t("sc.live.mode")}</span>
          <Toggle checked={s.enabled} onChange={(enabled) => set({ enabled })} label={t("sc.live.mode")} />
        </label>
      </div>

      <div className="grid grid-cols-[minmax(300px,1fr)_minmax(300px,1fr)] gap-6">
        <div className="flex flex-col gap-3">
          <span className="text-[12.5px] font-medium">{t("sc.live.plan")}</span>
          <Segmented<LivePlan>
            label={t("sc.live.plan")}
            value={s.plan}
            onChange={(p) => set({ plan: p })}
            options={PLANS.map((p) => ({ value: p.id, label: t(p.label) }))}
          />
          <BudgetLine s={s} />
          <p className="text-[11.5px] text-ink-3">{t(plan.years)}</p>
          {s.plan === "heavy" && <Hint tone="warn">{t("sc.live.heavyWarn")}</Hint>}
        </div>

        <div className="flex flex-col gap-3">
          <span className="text-[12.5px] font-medium">{t("sc.live.sources")}</span>
          <div className="flex flex-col gap-2 rounded-xl border border-line p-3">
            <div className="flex items-start justify-between gap-3">
              <span className="flex items-start gap-2.5">
                <Music2 size={16} className="mt-0.5 shrink-0 text-signal" />
                <span className="flex flex-col">
                  <span className="text-[13px] font-semibold">{t("sc.live.np")}</span>
                  <span className="text-[11.5px] text-ink-3">{t("sc.live.np.body")}</span>
                </span>
              </span>
              <Toggle checked={s.nowPlaying} onChange={(nowPlaying) => set({ nowPlaying })} label={t("sc.live.np")} />
            </div>
            {s.enabled && s.nowPlaying && <TrackLine s={s} />}
          </div>
        </div>
      </div>

      <StatusLine s={s} usb={usb} />
      {drawError && <Hint tone="error">{drawError}</Hint>}
      {s.enabled && (
        <div className="flex flex-col gap-1">
          <Hint tone="note">{t("sc.live.seeGif")}</Hint>
          <Hint tone="note">{t("sc.live.appOpen")}</Hint>
        </div>
      )}
    </section>
  );
}

function BudgetLine({ s }: { s: LiveState }) {
  const t = useT();
  const ready = Math.max(0, Math.floor(s.tokens + 1e-6));
  const pct = Math.max(0, Math.min(100, (s.tokens / s.burst) * 100));
  const units = { h: t("sc.live.h"), m: t("sc.live.m") };
  return (
    <div className="flex flex-col gap-1.5" data-budget>
      <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
        <div className="h-full rounded-full bg-ok transition-[width]" style={{ width: `${pct}%` }} />
      </div>
      <div className="num flex justify-between text-[11.5px] text-ink-2">
        <span>
          {ready} / {s.burst} {t("sc.live.ready")}
        </span>
        <span>
          {s.nextTokenMs != null
            ? `${t("sc.live.next")} ${fmtWait(s.nextTokenMs, units)}`
            : ready >= s.burst
              ? t("sc.live.full")
              : `${t("sc.live.refill")} ${fmtWait(3_600_000 / (s.perDay / 24), units)}`}
        </span>
      </div>
    </div>
  );
}

function TrackLine({ s }: { s: LiveState }) {
  const t = useT();
  const tr = s.track;
  if (!tr) return <p className="text-[12px] text-ink-3">{t("sc.live.np.none")}</p>;
  const state = !tr.playing ? t("sc.live.np.paused") : tr.settled ? t("sc.live.np.settled") : t("sc.live.np.settling");
  return (
    <div className="flex flex-col gap-0.5 rounded-lg bg-surface-2/60 px-2.5 py-2" data-track>
      <span className="truncate text-[12.5px] font-medium">
        {tr.artist ? `${tr.artist} — ${tr.title}` : tr.title}
      </span>
      <span className="text-[11px] text-ink-3">
        {tr.app ? `${tr.app} · ` : ""}
        {state}
      </span>
    </div>
  );
}

function StatusLine({ s, usb }: { s: LiveState; usb: boolean }) {
  const t = useT();
  const units = { h: t("sc.live.h"), m: t("sc.live.m") };
  const row = "flex items-center gap-2 text-[12.5px]";
  if (!s.enabled) return <p className="text-[12.5px] text-ink-3">{t("sc.live.off")}</p>;
  return (
    <div className="flex flex-col gap-1.5 border-t border-line pt-4" data-live-status>
      {s.uploading && (
        <p className={`${row} text-ink`}>
          <Loader2 size={14} className="animate-spin text-signal" />
          {t("sc.live.uploading")}: <span className="truncate font-medium">{s.uploading.name}</span>
        </p>
      )}
      {s.pending && (
        <p className={`${row} text-ink-2`}>
          <Clock3 size={14} className="text-warn" />
          {t("sc.live.pending")}: <span className="truncate font-medium">{s.pending.name}</span>
          <span className="text-ink-3">
            ·{" "}
            {!usb
              ? t("sc.live.waitCable")
              : s.nextTokenMs != null
                ? `${t("sc.live.waitBudget")} ${fmtWait(s.nextTokenMs, units)}`
                : t("sc.live.waitTurn")}
          </span>
        </p>
      )}
      {s.last ? (
        <p className={`${row} text-ink-2`}>
          <Upload size={14} className="text-ok" />
          {t("sc.live.last")}: <span className="truncate font-medium">{s.last.name}</span>
          <span className="num text-ink-3">
            · {new Date(s.last.atMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </span>
        </p>
      ) : (
        !s.pending && !s.uploading && <p className="text-[12.5px] text-ink-3">{t("sc.live.nothingYet")}</p>
      )}
      {s.lastError && !s.uploading && (
        <p className={`${row} text-warn`}>
          <AlertTriangle size={14} /> {t("sc.live.failed")}: {s.lastError}
        </p>
      )}
    </div>
  );
}
