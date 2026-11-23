// "Odak zamanlayıcısı" on the Overview: start it from here; while it runs,
// the block, its place in the sequence and the time left. Everything this
// row reads from the timer goes through `focusView`.

import { Play, Timer } from "lucide-react";
import { useEffect, useState } from "react";
import type { PomodoroPhase } from "../../bindings/PomodoroPhase";
import type { PomodoroState } from "../../bindings/PomodoroState";
import { useT, type MessageKey, type Translate } from "../../lib/i18n";
import { DEFAULT_SETTINGS, fmtClock, phaseColor, remainingNow, totalMinutes, usePomodoro } from "../../lib/pomodoro";
import { useApp } from "../../lib/store";
import { Row } from "../../components/controls";

/** What the row shows, whatever the timer's own model looks like. */
type FocusView =
  | { kind: "idle"; plan: string }
  | { kind: "busy"; paused: boolean; phase: string; color: string; leftMs: number; totalMs: number };

const PHASE: Record<PomodoroPhase, MessageKey> = {
  focus: "pomo.phase.focus",
  break: "pomo.phase.break",
};

function focusView(p: PomodoroState | null, t: Translate, now: number): FocusView {
  if (!p || p.status === "idle") {
    const s = p?.settings ?? DEFAULT_SETTINGS;
    return { kind: "idle", plan: t("overview.focus.plan", { blocks: s.blocks.length, min: totalMinutes(s) }) };
  }
  const blocks = p.settings.blocks.length;
  return {
    kind: "busy",
    paused: p.status === "paused",
    phase: blocks > 1 ? `${t(PHASE[p.phase])} · ${t("pomo.round")} ${p.block}/${blocks}` : t(PHASE[p.phase]),
    color: phaseColor(p.settings, p.phase),
    leftMs: remainingNow(p, now),
    totalMs: p.phaseMs,
  };
}

export function FocusRow() {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  const p = usePomodoro((x) => x.state);
  const control = usePomodoro((x) => x.control);
  const running = p?.status === "running";
  // Drawn again twice a second while it runs, so the time left counts down.
  const [, tick] = useState(0);
  useEffect(() => {
    if (!running) return;
    const id = window.setInterval(() => tick((n) => n + 1), 500);
    return () => window.clearInterval(id);
  }, [running]);
  const v = focusView(p, t, Date.now());

  return (
    <Row
      icon={Timer}
      title={t("nav.focus")}
      desc={
        v.kind === "busy" ? (
          <span className="flex flex-col gap-1.5 pt-0.5" data-focus-tile="busy">
            <span className="flex items-center gap-1.5">
              <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: v.color }} aria-hidden />
              {v.phase}
              {v.paused && <span>· {t("pomo.paused")}</span>}
            </span>
            <span className="h-1 max-w-[320px] overflow-hidden rounded-full bg-line-strong" aria-hidden>
              <span
                className="block h-full rounded-full transition-[width] duration-500"
                style={{ width: `${v.totalMs > 0 ? Math.min(100, (1 - v.leftMs / v.totalMs) * 100) : 0}%`, background: v.color }}
              />
            </span>
          </span>
        ) : (
          <span data-focus-tile="idle">{v.plan}</span>
        )
      }
      onOpen={() => setPage("focus")}
      openLabel={t("overview.focus.go")}
      data="focus"
    >
      {v.kind === "busy" ? (
        <span className="flex items-baseline gap-2">
          <span className="num text-[20px] leading-7 font-semibold">{fmtClock(v.leftMs)}</span>
          <span className="text-[12px] text-ink-2">{t("overview.focus.left")}</span>
        </span>
      ) : (
        <button type="button" onClick={() => control("start").catch(console.error)} className="btn-primary" data-quick-start>
          <Play size={12} fill="currentColor" aria-hidden />
          {t("overview.focus.start")}
        </button>
      )}
    </Row>
  );
}
