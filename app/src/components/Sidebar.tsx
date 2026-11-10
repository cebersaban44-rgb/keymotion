import { Cable, Keyboard, LayoutGrid, Lock, Monitor, Settings2, Sparkles, Workflow, type LucideIcon } from "lucide-react";
import type { Feature } from "../bindings/Feature";
import { connectionKey, useT, type MessageKey } from "../lib/i18n";
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
          className={`group relative flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-[13.5px] transition-colors ${
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
              className="h-2 w-2 rounded-full bg-warn shadow-[0_0_8px_var(--warn)]"
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
          <div className="font-display text-[17px] font-semibold tracking-[-0.02em]">{t("app.name")}</div>
        </div>
      </div>

      <nav className="flex flex-1 flex-col gap-5" aria-label={t("app.name")}>
        <div>
          <div className="eyebrow mb-2 px-3">{t("nav.section.device")}</div>
          <ul className="flex flex-col gap-0.5">{DEVICE_ITEMS.map(item)}</ul>
        </div>
        <div>
          <div className="eyebrow mb-2 px-3">{t("nav.section.app")}</div>
          <ul className="flex flex-col gap-0.5">{APP_ITEMS.map(item)}</ul>
        </div>
      </nav>

      <div className="card flex items-center gap-3 rounded-2xl px-3.5 py-3">
        <span
          className={`h-2 w-2 shrink-0 rounded-full ${kb ? "bg-ok shadow-[0_0_8px_var(--ok)]" : "bg-ink-3"}`}
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
