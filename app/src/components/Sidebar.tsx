// The navigation pane (Windows Settings' pane): at the top the connected
// keyboard, like the account block in Windows Settings (the app's logo, "Connected
// keyboard", its name and how it is connected); a search box that narrows
// the list; the pages in groups; Settings at the bottom. Below 1200 px the
// pane shrinks to its icons.

import {
  AudioLines,
  BellRing,
  Bot,
  Cable,
  CalendarDays,
  Gamepad2,
  Gauge,
  Keyboard,
  Layers,
  LayoutGrid,
  Lock,
  Monitor,
  Search,
  Settings2,
  Sparkles,
  Timer,
  Workflow,
  X,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type SyntheticEvent } from "react";
import type { Feature } from "../bindings/Feature";
import { connectionKey, modelTitle, percent, useT, type MessageKey } from "../lib/i18n";
import { fmtClock, phaseColor, remainingNow, usePomodoro } from "../lib/pomodoro";
import { SUPPORTED_MODEL, SUPPORTED_MODEL_ID } from "../lib/offline";
import { isNewAk980Firmware, useUnsupportedKeyboards } from "../lib/report";
import { useActiveKeyboard, useApp, useNarrowWindow, type PageId } from "../lib/store";
import { useUnsaved } from "../lib/unsaved";
import { useUpdates } from "../lib/updates";
import { UpdateCard } from "./Updates";

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
      { id: "music", label: "nav.music", icon: AudioLines },
      { id: "reminders", label: "nav.reminders", icon: BellRing },
      { id: "calendar", label: "nav.calendar", icon: CalendarDays },
      { id: "games", label: "nav.games", icon: Gamepad2 },
      { id: "agents", label: "nav.agents", icon: Bot },
    ],
  },
];

/** Settings, at the bottom of the pane as in Windows. */
const SETTINGS: NavItem = { id: "settings", label: "nav.settings", icon: Settings2 };

type ShowTip = (text: string) => (e: SyntheticEvent<HTMLElement>) => void;

export function Sidebar() {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const page = useApp((s) => s.page);
  const setPage = useApp((s) => s.setPage);
  const kb = useActiveKeyboard();
  const unsupported = useUnsupportedKeyboards();
  const unsavedPages = new Set(useUnsaved().filter((i) => !i.background).map((i) => i.page));
  // A new version waits: a dot on Settings, where the update is.
  const updateKind = useUpdates((s) => s.state?.status.kind);
  const updateWaits = updateKind === "available" || updateKind === "ready";
  const collapsed = useNarrowWindow();
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
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
  useEffect(() => {
    if (collapsed) setQuery("");
  }, [collapsed]);

  // The search narrows the list to the pages whose name has the words typed.
  const words = query.trim().toLocaleLowerCase(locale).split(/\s+/).filter(Boolean);
  const matches = (it: NavItem) => {
    const name = t(it.label).toLocaleLowerCase(locale);
    return words.every((w) => name.includes(w));
  };
  const groups = GROUPS.map((g) => ({ ...g, items: words.length ? g.items.filter(matches) : g.items })).filter(
    (g) => g.items.length > 0,
  );
  const found = [...groups.flatMap((g) => g.items), ...(words.length && matches(SETTINGS) ? [SETTINGS] : [])];
  const onSearchKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape" && query) {
      e.preventDefault();
      setQuery("");
    } else if (e.key === "Enter" && words.length && found[0]) {
      e.preventDefault();
      setPage(found[0].id);
      setQuery("");
    }
  };

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
          onClick={() => {
            setPage(it.id);
            setQuery("");
          }}
          aria-current={active ? "page" : undefined}
          aria-label={collapsed ? tipText : undefined}
          onMouseEnter={showTip(tipText)}
          onFocus={showTip(tipText)}
          onMouseLeave={hideTip}
          onBlur={hideTip}
          className={`nav-item ${collapsed ? "justify-center !px-0" : ""}`}
          data-nav={it.id}
        >
          <Icon size={16} strokeWidth={1.6} className={`shrink-0 ${locked ? "text-ink-3" : ""}`} aria-hidden />
          {!collapsed && <span className="min-w-0 flex-1 truncate">{label}</span>}
          {unsaved && (
            <span
              role="img"
              aria-label={t("unsaved.dot")}
              title={t("unsaved.dot")}
              className={`h-2 w-2 shrink-0 rounded-full bg-warn ${collapsed ? "absolute top-1.5 right-2" : ""}`}
            />
          )}
          {it.id === "settings" && updateWaits && !unsaved && (
            <span
              role="img"
              aria-label={t("update.dot")}
              title={t("update.dot")}
              className={`h-2 w-2 shrink-0 rounded-full bg-signal ${collapsed ? "absolute top-1.5 right-2" : ""}`}
              data-update-dot
            />
          )}
          {locked && !collapsed && <Lock size={12} className="shrink-0 text-ink-3" aria-label={t("soon.needsUsb")} />}
        </button>
      </li>
    );
  };

  // An AK980 PRO on AJAZZ's firmware 769 is there, but Keymotion can't
  // drive it yet: say so rather than "Waiting for a keyboard".
  const newFirmware = !kb && unsupported.some(isNewAk980Firmware);
  const deviceTitle = kb
    ? modelTitle(t, kb.modelId, kb.modelName)
    : newFirmware
      ? modelTitle(t, SUPPORTED_MODEL_ID, SUPPORTED_MODEL)
      : t("device.none.title");
  const deviceSub = kb ? t(connectionKey(kb.connection)) : newFirmware ? t("fw769.sub") : t("device.none.sub");
  // Not green for the receiver alone: whether the keyboard is on behind it
  // is not known yet.
  const connected = !!kb && kb.connection !== "dongle";

  return (
    <aside
      className={`flex h-full shrink-0 flex-col pb-3 ${collapsed ? "w-[60px] items-stretch px-1.5 pt-3" : "w-[300px] px-2 pt-2"}`}
      data-sidebar={collapsed ? "collapsed" : "full"}
    >
      {/* The connected keyboard, like the account block in Windows Settings. */}
      <div
        className={`flex items-center ${collapsed ? "justify-center pb-3" : "mx-1 mt-1 mb-3.5 gap-3.5 py-1"}`}
        role={collapsed ? "img" : "group"}
        aria-label={`${kb ? `${t("nav.keyboard")}: ` : ""}${deviceTitle}, ${deviceSub}`}
        tabIndex={collapsed ? 0 : undefined}
        onMouseEnter={showTip(`${deviceTitle}, ${deviceSub}`)}
        onFocus={showTip(`${deviceTitle}, ${deviceSub}`)}
        onMouseLeave={hideTip}
        onBlur={hideTip}
        data-device-status={kb ? kb.connection : newFirmware ? "new-firmware" : "none"}
      >
        <img
          src="/app-icon.svg"
          alt=""
          className={`shrink-0 rounded-[10px] shadow-[0_0_0_1px_var(--line-strong)] ${collapsed ? "h-9 w-9" : "h-11 w-11"} ${
            kb ? "" : "opacity-50 grayscale"
          }`}
        />
        {!collapsed && (
          <div className="flex min-w-0 flex-col" aria-hidden>
            {kb && <span className="text-[12px] leading-4 text-ink-2">{t("nav.keyboard")}</span>}
            <span className="truncate text-[14px] leading-5 font-semibold">{deviceTitle}</span>
            <span className="flex items-center gap-1.5 text-[12px] leading-4 text-ink-2">
              <span
                className={`h-2 w-2 shrink-0 rounded-full ${connected ? "bg-ok" : "shadow-[inset_0_0_0_1.5px_var(--ink-3)]"}`}
              />
              <span className="num truncate">
                {deviceSub}
                {kb?.battery ? ` · ${percent(kb.battery.percent)}` : ""}
              </span>
            </span>
          </div>
        )}
      </div>

      {!collapsed && (
        <label className="relative mx-1 mb-1.5 flex h-8 items-center">
          <input
            ref={search}
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onSearchKey}
            placeholder={t("nav.search")}
            aria-label={t("nav.search")}
            spellCheck={false}
            className="nav-search h-8 w-full rounded-md border border-line bg-bg-raise pr-9 pl-3 text-[13px] outline-none placeholder:text-ink-3"
            data-nav-search
          />
          {query ? (
            <button
              type="button"
              onClick={() => {
                setQuery("");
                search.current?.focus();
              }}
              className="absolute right-1 grid h-6 w-7 place-items-center rounded-[3px] text-ink-2 hover:bg-[var(--subtle-hover)]"
              aria-label={t("nav.search.clear")}
            >
              <X size={14} aria-hidden />
            </button>
          ) : (
            <Search size={14} className="pointer-events-none absolute right-3 text-ink-2" aria-hidden />
          )}
        </label>
      )}

      <nav
        className={`flex min-h-0 flex-1 flex-col overflow-y-auto ${collapsed ? "gap-1 [scrollbar-width:none]" : "-mx-1 px-1"}`}
        aria-label={t("app.name")}
      >
        {groups.map((g, i) => (
          <div key={g.title} className={collapsed && i > 0 ? "mt-1 border-t border-line pt-1" : ""}>
            {!collapsed && <div className="nav-group px-4 pt-3 pb-1">{t(g.title)}</div>}
            <ul className="mx-1 flex flex-col gap-0.5" aria-label={collapsed ? t(g.title) : undefined}>
              {g.items.map(item)}
            </ul>
          </div>
        ))}
        {words.length > 0 && found.length === 0 && (
          <p role="status" className="px-4 pt-3 text-[13px] text-ink-2" data-nav-none>
            {t("nav.search.none")}
          </p>
        )}
      </nav>

      <div className={`flex flex-col gap-2 pt-2 ${collapsed ? "" : "mx-1"}`}>
        <UpdateCard collapsed={collapsed} onTip={showTip} onTipEnd={hideTip} />
        <PomodoroChip collapsed={collapsed} showTip={showTip} hideTip={hideTip} />
        <ul className={collapsed ? "" : "mx-0"}>{(!words.length || matches(SETTINGS)) && item(SETTINGS)}</ul>
      </div>

      {collapsed && tip && (
        <div
          aria-hidden
          className="flyout pointer-events-none fixed left-[64px] z-50 -translate-y-1/2 px-2.5 py-1 text-[12px] whitespace-nowrap text-ink"
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
      className={`flex items-center rounded-md border border-line bg-surface text-left transition-colors hover:bg-surface-2 ${
        collapsed ? "h-10 justify-center" : "gap-3 px-3.5 py-2.5"
      }`}
      data-pomo-chip
    >
      <Timer size={16} style={{ color }} aria-hidden />
      {!collapsed && (
        <>
          <span className="min-w-0 flex-1 leading-tight">
            <span className="block truncate text-[12px] text-ink-2">{sub}</span>
            <span className="num block text-[15px] font-semibold">
              {left}
              {p.status === "paused" && <span className="ml-1.5 text-[12px] font-normal text-ink-2">{t("pomo.paused")}</span>}
            </span>
          </span>
          <span className="h-2 w-2 rounded-full" style={{ background: color }} aria-hidden />
        </>
      )}
    </button>
  );
}
