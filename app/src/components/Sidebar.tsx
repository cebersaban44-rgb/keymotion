import { BellRing, Cable, Gauge, Keyboard, LayoutGrid, Lock, Monitor, Settings2, Sparkles, Timer, Workflow, type LucideIcon } from "lucide-react";
import { useEffect, useState } from "react";
import type { Feature } from "../bindings/Feature";
import { connectionKey, useT, type MessageKey } from "../lib/i18n";
import { fmtClock, phaseColor, remainingNow, usePomodoro } from "../lib/pomodoro";
import { useActiveKeyboard, useApp, type PageId } from "../lib/store";
import { useUnsaved } from "../lib/unsaved";

interface NavItem {
  id: PageId;
  label: MessageKey;
  icon: LucideIcon;
  /** Page needs this feature in the current connection mode. */
  requires?: Feature;
}

const DEVICE_ITEMS: NavItem[] = [
  { id: "overview", label: "nav.overview", icon: LayoutGrid },
  { id: "lighting", label: "nav.lighting", icon: Sparkles, requires: "lighting" },
  { id: "keymap", label: "nav.keymap", icon: Keyboard, requires: "keymap" },
  { id: "macros", label: "nav.macros", icon: Workflow, requires: "macros" },
  { id: "screen", label: "nav.screen", icon: Monitor, requires: "screen" },
  { id: "connection", label: "nav.connection", icon: Cable },
];

/** Tools that use the whole keyboard (lights and screen) for a task. */
const TOOL_ITEMS: NavItem[] = [
  { id: "focus", label: "nav.focus", icon: Timer },
  { id: "gauges", label: "nav.gauges", icon: Gauge },
  { id: "reminders", label: "nav.reminders", icon: BellRing },
];

const APP_ITEMS: NavItem[] = [{ id: "settings", label: "nav.settings", icon: Settings2 }];

export function Sidebar() {
  const t = useT();
  const page = useApp((s) => s.page);
  const setPage = useApp((s) => s.setPage);
  const kb = useActiveKeyboard();
  const unsavedPages = new Set(useUnsaved().map((i) => i.page));

  const item = (it: NavItem) => {
    const active = page === it.id;
    const locked = !!it.requires && !!kb && !kb.features.includes(it.requires);
    const Icon = it.icon;
    return (
      <li key={it.id}>
        <button
          type="button"
          onClick={() => setPage(it.id)}
          aria-current={active ? "page" : undefined}
          className={`group relative flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-[13.5px] transition-colors ${
            active ? "bg-surface text-ink shadow-[inset_0_0_0_1px_var(--line)]" : "text-ink-2 hover:bg-surface/60 hover:text-ink"
          }`}
        >
          {active && <span className="absolute top-2.5 bottom-2.5 left-0 w-[3px] rounded-full bg-signal" />}
          <Icon size={17} strokeWidth={1.75} className={active ? "text-signal" : ""} />
          <span className="flex-1">{t(it.label)}</span>
          {unsavedPages.has(it.id) && (
            <span
              role="img"
              aria-label={t("unsaved.dot")}
              title={t("unsaved.dot")}
              className="h-2 w-2 rounded-full bg-warn"
            />
          )}
          {locked && <Lock size={12} className="text-ink-3" aria-label={t("soon.needsUsb")} />}
        </button>
      </li>
    );
  };

  return (
    <aside className="flex h-full w-[248px] shrink-0 flex-col gap-6 border-r border-line bg-bg-raise/70 px-4 pt-6 pb-4">
      <div className="flex items-center gap-2.5 px-2">
        <img src="/app-icon.svg" alt="" className="h-8 w-8" />
        <div className="leading-tight">
          <div className="font-display text-[17px] font-semibold">{t("app.name")}</div>
        </div>
      </div>

      <nav className="flex flex-1 flex-col gap-5" aria-label={t("app.name")}>
        <div>
          <div className="nav-group mb-1.5 px-3">{t("nav.section.device")}</div>
          <ul className="flex flex-col gap-0.5">{DEVICE_ITEMS.map(item)}</ul>
        </div>
        <div>
          <div className="nav-group mb-1.5 px-3">{t("nav.section.tools")}</div>
          <ul className="flex flex-col gap-0.5">{TOOL_ITEMS.map(item)}</ul>
        </div>
        <div>
          <div className="nav-group mb-1.5 px-3">{t("nav.section.app")}</div>
          <ul className="flex flex-col gap-0.5">{APP_ITEMS.map(item)}</ul>
        </div>
      </nav>

      <PomodoroChip />

      <div className="card flex items-center gap-3 rounded-lg px-3.5 py-3">
        <span
          className={`h-2 w-2 shrink-0 rounded-full ${kb ? "bg-ok" : "bg-ink-3"}`}
          aria-hidden
        />
        <div className="min-w-0 flex-1 leading-tight">
          <div className="truncate text-[13px] font-medium">{kb ? kb.modelName : t("device.none.title")}</div>
          {kb && <div className="num truncate text-[11px] text-ink-3">{t(connectionKey(kb.connection))}</div>}
        </div>
        {kb?.battery && <span className="num text-[12px] text-ink-2">%{kb.battery.percent}</span>}
      </div>
    </aside>
  );
}

/** The running Pomodoro, on every page; opens its section. */
function PomodoroChip() {
  const t = useT();
  const p = usePomodoro((x) => x.state);
  const setPage = useApp((s) => s.setPage);
  const onPage = useApp((s) => s.page === "focus");
  const [, tick] = useState(0);
  const running = p?.status === "running";
  useEffect(() => {
    if (!running) return;
    const id = window.setInterval(() => tick((n) => n + 1), 500);
    return () => window.clearInterval(id);
  }, [running]);
  // Not needed on the timer's own page.
  if (!p || p.status === "idle" || onPage) return null;
  const color = phaseColor(p.settings, p.phase);
  const phase =
    p.phase === "focus" ? t("pomo.phase.focus") : p.phase === "shortBreak" ? t("pomo.phase.shortBreak") : t("pomo.phase.longBreak");
  return (
    <button
      type="button"
      title={t("pomo.go")}
      onClick={() => setPage("focus")}
      className="flex items-center gap-3 rounded-lg border border-line px-3.5 py-2.5 text-left hover:bg-surface/60"
      data-pomo-chip
    >
      <Timer size={16} style={{ color }} />
      <span className="min-w-0 flex-1 leading-tight">
        <span className="block text-[11px] text-ink-3">
          {phase} · {t("pomo.round")} {p.round}/{p.settings.rounds}
        </span>
        <span className="num block text-[15px] font-semibold">
          {fmtClock(remainingNow(p))}
          {p.status === "paused" && <span className="ml-1.5 text-[11px] font-normal text-ink-3">{t("pomo.paused")}</span>}
        </span>
      </span>
      <span className="h-2 w-2 rounded-full" style={{ background: color, boxShadow: `0 0 8px ${color}` }} aria-hidden />
    </button>
  );
}
