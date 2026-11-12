import {
  BellRing,
  Cable,
  CalendarDays,
  Gauge,
  Keyboard,
  Layers,
  LayoutGrid,
  Lock,
  Monitor,
  Settings2,
  Sparkles,
  Timer,
  Workflow,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState, type SyntheticEvent } from "react";
import type { Feature } from "../bindings/Feature";
import { connectionKey, percent, useT, type MessageKey } from "../lib/i18n";
import { fmtClock, phaseColor, remainingNow, usePomodoro } from "../lib/pomodoro";
import { useActiveKeyboard, useApp, useNarrowWindow, type PageId } from "../lib/store";
import { useUnsaved } from "../lib/unsaved";

/** The Profiles page's entry (it works without the keyboard; loading needs the cable). */
export const PROFILES_ENABLED = true;

interface NavItem {
  id: PageId;
  label: MessageKey;
  icon: LucideIcon;
  /** Page needs this feature, which only the USB cable gives. */
  requires?: Feature;
}

interface NavGroup {
  title: MessageKey;
  items: NavItem[];
}

const GROUPS: NavGroup[] = [
  {
    title: "nav.section.device",
    items: [
      { id: "overview", label: "nav.overview", icon: LayoutGrid },
      { id: "lighting", label: "nav.lighting", icon: Sparkles, requires: "lighting" },
      { id: "keymap", label: "nav.keymap", icon: Keyboard, requires: "keymap" },
      { id: "macros", label: "nav.macros", icon: Workflow, requires: "macros" },
      // The screen library works without the keyboard; only uploads need it.
      { id: "screen", label: "nav.screen", icon: Monitor },
      ...(PROFILES_ENABLED ? [{ id: "profiles", label: "nav.profiles", icon: Layers } as NavItem] : []),
      { id: "connection", label: "nav.connection", icon: Cable },
    ],
  },
  {
    // Tools that use the whole keyboard (lights and screen) for a task;
    // they also work without it.
    title: "nav.section.tools",
    items: [
      { id: "focus", label: "nav.focus", icon: Timer },
      { id: "gauges", label: "nav.gauges", icon: Gauge },
      { id: "reminders", label: "nav.reminders", icon: BellRing },
      { id: "calendar", label: "nav.calendar", icon: CalendarDays },
    ],
  },
  { title: "nav.section.app", items: [{ id: "settings", label: "nav.settings", icon: Settings2 }] },
];

type ShowTip = (text: string) => (e: SyntheticEvent<HTMLElement>) => void;

export function Sidebar() {
  const t = useT();
  const page = useApp((s) => s.page);
  const setPage = useApp((s) => s.setPage);
  const kb = useActiveKeyboard();
  const unsavedPages = new Set(useUnsaved().filter((i) => !i.background).map((i) => i.page));
  const collapsed = useNarrowWindow();
  // Collapsed, the names show as tooltips beside the icons. They are drawn
  // fixed, so the scrolling list does not clip them.
  const [tip, setTip] = useState<{ text: string; y: number } | null>(null);
  const showTip: ShowTip = (text) => (e) => {
    if (!collapsed) return;
    const r = e.currentTarget.getBoundingClientRect();
    setTip({ text, y: r.top + r.height / 2 });
  };
  const hideTip = () => setTip(null);
  useEffect(() => setTip(null), [collapsed, page]);

  const item = (it: NavItem) => {
    const active = page === it.id;
    const locked = !!it.requires && !kb?.features.includes(it.requires);
    const unsaved = unsavedPages.has(it.id);
    const Icon = it.icon;
    const label = t(it.label);
    const tipText = locked ? t("nav.locked", { page: label }) : label;
    return (
      <li key={it.id}>
        <button
          type="button"
          onClick={() => setPage(it.id)}
          aria-current={active ? "page" : undefined}
          aria-label={collapsed ? tipText : undefined}
          onMouseEnter={showTip(tipText)}
          onFocus={showTip(tipText)}
          onMouseLeave={hideTip}
          onBlur={hideTip}
          className={`group relative flex h-9 w-full items-center gap-3 rounded-lg text-left text-[13.5px] transition-colors ${
            collapsed ? "justify-center px-0" : "px-3"
          } ${active ? "bg-surface text-ink shadow-[inset_0_0_0_1px_var(--line)]" : "text-ink-2 hover:bg-surface/60 hover:text-ink"}`}
          data-nav={it.id}
        >
          {active && <span className="absolute top-2 bottom-2 left-0 w-[3px] rounded-full bg-signal" />}
          <Icon size={17} strokeWidth={1.75} className={`shrink-0 ${active ? "text-signal" : locked ? "text-ink-3" : ""}`} aria-hidden />
          {!collapsed && <span className="min-w-0 flex-1 truncate">{label}</span>}
          {unsaved && (
            <span
              role="img"
              aria-label={t("unsaved.dot")}
              title={t("unsaved.dot")}
              className={`h-2 w-2 shrink-0 rounded-full bg-warn ${collapsed ? "absolute top-1.5 right-2.5" : ""}`}
            />
          )}
          {locked && !collapsed && <Lock size={12} className="shrink-0 text-ink-3" aria-label={t("soon.needsUsb")} />}
        </button>
      </li>
    );
  };

  const deviceTitle = kb ? kb.modelName : t("device.none.title");
  const deviceSub = kb ? t(connectionKey(kb.connection)) : t("device.none.sub");

  return (
    <aside
      className={`flex h-full shrink-0 flex-col border-r border-line bg-bg-raise/70 pb-4 ${
        collapsed ? "w-16 gap-3 px-2 pt-4" : "w-[232px] gap-5 px-3.5 pt-5"
      }`}
      data-sidebar={collapsed ? "collapsed" : "full"}
    >
      <div className={`flex h-8 items-center gap-2.5 ${collapsed ? "justify-center" : "px-1.5"}`}>
        <img src="/app-icon.svg" alt={collapsed ? t("app.name") : ""} className="h-8 w-8" />
        {!collapsed && <div className="truncate font-display text-[16px] font-semibold">{t("app.name")}</div>}
      </div>

      <nav
        className={`-mx-1 flex min-h-0 flex-1 flex-col overflow-y-auto px-1 ${collapsed ? "gap-2 [scrollbar-width:none]" : "gap-4"}`}
        aria-label={t("app.name")}
      >
        {GROUPS.map((g, i) => (
          <div key={g.title} className={collapsed && i > 0 ? "border-t border-line pt-2" : ""}>
            {!collapsed && <div className="nav-group mb-1 px-3">{t(g.title)}</div>}
            <ul className="flex flex-col gap-0.5" aria-label={collapsed ? t(g.title) : undefined}>
              {g.items.map(item)}
            </ul>
          </div>
        ))}
      </nav>

      <PomodoroChip collapsed={collapsed} showTip={showTip} hideTip={hideTip} />

      <div
        className={`card flex items-center rounded-lg ${collapsed ? "justify-center p-2.5" : "gap-3 px-3.5 py-3"}`}
        role={collapsed ? "img" : undefined}
        aria-label={collapsed ? `${deviceTitle}, ${deviceSub}` : undefined}
        tabIndex={collapsed ? 0 : undefined}
        onMouseEnter={showTip(`${deviceTitle}, ${deviceSub}`)}
        onFocus={showTip(`${deviceTitle}, ${deviceSub}`)}
        onMouseLeave={hideTip}
        onBlur={hideTip}
        data-device-status={kb ? kb.connection : "none"}
      >
        <span className={`h-2 w-2 shrink-0 rounded-full ${kb ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
        {!collapsed && (
          <>
            <div className="min-w-0 flex-1 leading-tight">
              <div className="truncate text-[13px] font-medium">{deviceTitle}</div>
              <div className="truncate text-[12px] text-ink-3">{deviceSub}</div>
            </div>
            {kb?.battery && <span className="num text-[12px] text-ink-2">{percent(kb.battery.percent)}</span>}
          </>
        )}
      </div>

      {collapsed && tip && (
        <div
          aria-hidden
          className="pointer-events-none fixed left-[70px] z-50 -translate-y-1/2 rounded-md border border-line bg-surface px-2.5 py-1 text-[12px] whitespace-nowrap text-ink shadow-lg"
          style={{ top: tip.y }}
          data-nav-tip
        >
          {tip.text}
        </div>
      )}
    </aside>
  );
}

/** The running focus timer, on every page; opens its page. */
function PomodoroChip({ collapsed, showTip, hideTip }: { collapsed: boolean; showTip: ShowTip; hideTip: () => void }) {
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
  const phase = p.phase === "focus" ? t("pomo.phase.focus") : t("pomo.phase.break");
  const left = fmtClock(remainingNow(p));
  const sub = t("pomo.chip.sub", { phase, round: p.block, rounds: p.settings.blocks.length });
  const summary = t("pomo.chip", { phase, time: left });
  return (
    <button
      type="button"
      title={collapsed ? undefined : t("pomo.go")}
      aria-label={collapsed ? summary : undefined}
      onClick={() => setPage("focus")}
      onMouseEnter={showTip(summary)}
      onFocus={showTip(summary)}
      onMouseLeave={hideTip}
      onBlur={hideTip}
      className={`flex items-center rounded-lg border border-line text-left hover:bg-surface/60 ${
        collapsed ? "h-10 justify-center" : "gap-3 px-3.5 py-2.5"
      }`}
      data-pomo-chip
    >
      <Timer size={16} style={{ color }} aria-hidden />
      {!collapsed && (
        <>
          <span className="min-w-0 flex-1 leading-tight">
            <span className="block truncate text-[12px] text-ink-3">{sub}</span>
            <span className="num block text-[15px] font-semibold">
              {left}
              {p.status === "paused" && <span className="ml-1.5 text-[12px] font-normal text-ink-3">{t("pomo.paused")}</span>}
            </span>
          </span>
          <span className="h-2 w-2 rounded-full" style={{ background: color }} aria-hidden />
        </>
      )}
    </button>
  );
}
