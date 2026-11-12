// The Overview's tiles under the keyboard: light, reminders, calendar,
// system gauges and profile (the focus timer has its own file). Each says
// what is on now, or plainly that there is nothing, and links to its page.

import { BellRing, CalendarDays, Gauge, Layers, Sparkles } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import type { EffectInfo } from "../../bindings/EffectInfo";
import type { GeometryInfo } from "../../bindings/GeometryInfo";
import { calendarColor, hhmm, useCalendar } from "../../lib/calendar";
import { useGauges, watchGauges } from "../../lib/gauges";
import { useT, type MessageKey } from "../../lib/i18n";
import { getLightingCatalog } from "../../lib/ipc";
import { PALETTE, useLighting, type ColorMap } from "../../lib/lighting";
import { profileName, profileOnKeyboard, useProfiles } from "../../lib/profiles";
import { rgbCss, useReminders } from "../../lib/reminders";
import { useApp } from "../../lib/store";
import { ICON_OF, when } from "../Reminders";
import { Empty, LoadBar, Swatch, Tile, useClock, useInText } from "./Tile";

// ------------------------------------------------------------------ light

/** The firmware's effects (which of them take a colour), read once. */
let catalog: Promise<EffectInfo[]> | null = null;
function useEffectInfo(mode: number | null): EffectInfo | null {
  const [list, setList] = useState<EffectInfo[] | null>(null);
  useEffect(() => {
    let live = true;
    catalog ??= getLightingCatalog();
    catalog.then(
      (l) => live && setList(l),
      (e) => {
        catalog = null;
        console.error(e);
      },
    );
    return () => {
      live = false;
    };
  }, []);
  return mode == null ? null : (list?.find((e) => e.id === mode) ?? null);
}

/** The colours of a per-key design, most keys first (unlit keys left out). */
function designColors(colors: ColorMap): string[] {
  const count = new Map<string, number>();
  for (const c of Object.values(colors)) {
    const hex = c.toLowerCase();
    if (hex !== "#000000") count.set(hex, (count.get(hex) ?? 0) + 1);
  }
  return [...count].sort((a, b) => b[1] - a[1]).map(([hex]) => hex);
}

const MAX_SWATCHES = 8;

/**
 * The light the app last stored on the keyboard and its colours. Only the
 * Lighting page changes it, with its own save step, so the colours here
 * are shown, not picked.
 */
export function LightTile() {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  const applied = useLighting((s) => s.appliedEffect);
  const saved = useLighting((s) => s.savedPerKey);
  const gauges = useGauges((s) => !!s.state?.settings.enabled);
  const info = useEffectInfo(saved ? null : (applied?.mode ?? null));
  const name = saved ? t("light.tab.perKey") : applied ? t(`fx.${applied.mode}` as MessageKey) : null;
  const level = saved ? saved.brightness : applied && applied.mode !== 0 ? applied.brightness : null;
  const design = saved ? designColors(saved.colors) : [];
  const colorName = (hex: string) => {
    const p = PALETTE.find((c) => c.hex === hex.toLowerCase());
    return p ? t(p.name) : undefined;
  };

  let colors: ReactNode = null;
  if (saved && design.length) {
    colors = (
      <span className="flex flex-wrap items-center gap-1.5" data-light-colors={design.length}>
        {design.slice(0, MAX_SWATCHES).map((hex) => (
          <Swatch key={hex} color={hex} label={colorName(hex)} />
        ))}
        {design.length > MAX_SWATCHES && <span className="num text-[12px] text-ink-3">+{design.length - MAX_SWATCHES}</span>}
      </span>
    );
  } else if (applied && applied.mode !== 0 && info) {
    const rainbow = info.color && info.rainbow && applied.rainbow;
    colors = (
      <span className="flex items-center gap-2 text-[13px] text-ink-2" data-light-colors={info.color ? (rainbow ? "rainbow" : "one") : "own"}>
        {info.color ? (
          <>
            <Swatch color={applied.color} rainbow={rainbow} />
            {rainbow ? t("light.rainbow") : colorName(applied.color)}
          </>
        ) : (
          <>
            <Swatch rainbow />
            {t("overview.light.ownColors")}
          </>
        )}
      </span>
    );
  }

  return (
    <Tile icon={Sparkles} title={t("overview.light")} data="light" link={t("overview.light.go")} onLink={() => setPage("lighting")}>
      {name ? <span className="truncate text-[15px]">{name}</span> : <Empty>{t("overview.light.unknown")}</Empty>}
      {colors}
      {level != null && (
        <div className="flex items-center gap-3 text-[12px] text-ink-3">
          <span>{t("light.brightness")}</span>
          <span
            role="meter"
            aria-label={t("light.brightness")}
            aria-valuemin={0}
            aria-valuemax={5}
            aria-valuenow={level}
            className="flex max-w-[140px] flex-1 gap-1"
          >
            {[1, 2, 3, 4, 5].map((i) => (
              <span key={i} className={`h-1.5 flex-1 rounded-full ${i <= level ? "bg-signal" : "bg-line-strong"}`} />
            ))}
          </span>
        </div>
      )}
      {gauges && <span className="text-[12px] text-ink-3">{t("overview.light.gauges")}</span>}
    </Tile>
  );
}

// -------------------------------------------------------------- reminders

/** The next reminder and how many are on. */
export function RemindersTile() {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const setPage = useApp((s) => s.setPage);
  const s = useReminders((x) => x.state);
  const now = useClock();
  const inText = useInText();
  const list = s?.reminders ?? [];
  const on = list.filter((i) => i.reminder.enabled).length;
  const next = list.filter((i) => i.nextMs != null).sort((a, b) => a.nextMs! - b.nextMs!)[0];
  // As the Reminders page says it: off, away, the next one, none ahead, none at all.
  const away = !!s && s.away && s.settings.awayWait && list.some((i) => i.reminder.enabled && i.reminder.when.kind === "every");
  const Icon = next ? ICON_OF[next.reminder.icon] : null;
  // "0 açık" says nothing: the badge shows only how many are on, or that
  // reminders are off.
  const badge =
    s && list.length && (on > 0 || !s.settings.enabled)
      ? { on: s.settings.enabled, text: s.settings.enabled ? t("overview.rem.on", { count: on }) : t("overview.off") }
      : null;

  return (
    <Tile
      icon={BellRing}
      title={t("nav.reminders")}
      data="reminders"
      badge={badge}
      link={t("overview.rem.go")}
      onLink={() => setPage("reminders")}
    >
      {!s ? null : !s.settings.enabled ? (
        <Empty>{t("reminder.status.off")}</Empty>
      ) : next && Icon && !away ? (
        <div className="flex min-w-0 items-center gap-3" data-overview-reminder>
          <span
            className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full"
            style={{ background: `color-mix(in srgb, ${rgbCss(next.reminder.color)} 22%, transparent)`, color: rgbCss(next.reminder.color) }}
          >
            <Icon size={17} aria-hidden />
          </span>
          <span className="flex min-w-0 flex-col">
            <span className="truncate text-[15px]" title={next.reminder.title}>
              {next.reminder.title}
            </span>
            <span className="num text-[12px] text-ink-3">
              {[when(next.nextMs!, t, locale), inText(next.nextMs!, now)].filter(Boolean).join(" · ")}
            </span>
          </span>
        </div>
      ) : (
        <Empty>{t(away ? "reminder.status.away" : list.length ? "reminder.status.none" : "reminder.status.empty")}</Empty>
      )}
    </Tile>
  );
}

// --------------------------------------------------------------- calendar

/** The next meeting, or that there is none (or no calendar). */
export function CalendarTile() {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const setPage = useApp((s) => s.setPage);
  const c = useCalendar((x) => x.state);
  const now = useClock();
  const inText = useInText();
  const next = c?.upcoming.find((e) => !e.allDay && e.endMs > now) ?? null;
  const running = !!next && next.startMs <= now;
  const tomorrow = !!next && new Date(next.startMs).toDateString() !== new Date(now).toDateString();
  const time = next
    ? `${tomorrow ? `${t("reminder.tomorrow").toLocaleLowerCase(locale)} ` : ""}${hhmm(next.startMs)}–${hhmm(next.endMs)}`
    : "";

  return (
    <Tile icon={CalendarDays} title={t("nav.calendar")} data="calendar" link={t("overview.cal.go")} onLink={() => setPage("calendar")}>
      {!c ? null : c.sources.length === 0 ? (
        <Empty>{t("overview.cal.none")}</Empty>
      ) : next ? (
        <div className="flex min-w-0 items-start gap-3" data-overview-meeting>
          <span className="mt-1.5 h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: calendarColor(next.color) }} aria-hidden />
          <span className="flex min-w-0 flex-col">
            <span className="truncate text-[15px]" title={next.title}>
              {next.title}
            </span>
            <span className="num text-[12px] text-ink-3">
              {[time, running ? t("overview.cal.now") : inText(next.startMs, now)].filter(Boolean).join(" · ")}
            </span>
          </span>
        </div>
      ) : (
        <Empty>{t("calendar.upcoming.none")}</Empty>
      )}
    </Tile>
  );
}

// ----------------------------------------------------------------- gauges

const LOADS = [
  ["cpu", "gauges.r.cpu"],
  ["ram", "gauges.r.ram"],
  ["gpu", "gauges.r.gpu"],
] as const;

/** On or off; while on, the loads the keys show, read live. */
export function GaugesTile({ geometry, usb }: { geometry: GeometryInfo | null; usb: boolean }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const setPage = useApp((s) => s.setPage);
  const s = useGauges((x) => x.state);
  const on = !!s?.settings.enabled;
  // The readings come only while someone watches; the gauges read this PC
  // anyway while they are on, so watching then costs nothing more.
  useEffect(() => (on ? watchGauges(geometry) : undefined), [on, geometry]);
  const set = s?.settings;
  const loads = set ? LOADS.filter(([id]) => set[id]) : [];
  const chosen = set
    ? [
        ...loads.map(([, label]) => t(label)),
        ...(set.volume ? [t("gauges.volume")] : []),
        ...(set.mic ? [t("gauges.mic")] : []),
      ]
    : [];
  const list = new Intl.ListFormat(locale === "tr" ? "tr-TR" : "en-US", { type: "conjunction" }).format(chosen);

  return (
    <Tile
      icon={Gauge}
      title={t("nav.gauges")}
      data="gauges"
      badge={s ? { on, text: t(on ? "overview.on" : "overview.off") } : null}
      link={t("light.gaugesGo")}
      onLink={() => setPage("gauges")}
    >
      {!s ? null : on && loads.length > 0 ? (
        <div className="grid grid-cols-[auto_1fr_auto] items-center gap-x-3 gap-y-2 text-[12px]" data-overview-loads>
          {loads.map(([id, label]) => (
            <LoadBar key={id} label={t(label)} value={s.readings[id]} none={t("gauges.r.none")} />
          ))}
        </div>
      ) : on ? (
        <Empty>{chosen.length ? t("overview.gauges.shown", { list }) : t("overview.gauges.nothing")}</Empty>
      ) : (
        <>
          <Empty>{t("overview.gauges.off")}</Empty>
          {chosen.length > 0 && <span className="text-[12px] text-ink-3">{t("overview.gauges.chosen", { list })}</span>}
        </>
      )}
      {on && !usb && <span className="text-[12px] text-ink-3">{t("gauges.status.noUsb")}</span>}
    </Tile>
  );
}

// ---------------------------------------------------------------- profile

/** The profile the keyboard has (all its parts match this computer's
 *  records), or that it matches none. */
export function ProfileTile({ modelId }: { modelId: string }) {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  const state = useProfiles((s) => (s.modelId === modelId ? s.state : null));
  useEffect(() => {
    useProfiles.getState().load(modelId).catch(console.error);
  }, [modelId]);
  const match = state ? profileOnKeyboard(state) : null;
  return (
    <Tile icon={Layers} title={t("overview.profile")} data="profile" link={t("overview.profile.go")} onLink={() => setPage("profiles")}>
      {state &&
        (match ? (
          <span className="truncate text-[15px]" title={profileName(match, t)} data-overview-profile={match.id}>
            {profileName(match, t)}
          </span>
        ) : (
          <span data-overview-profile="none">
            <Empty>{t("overview.profile.none")}</Empty>
          </span>
        ))}
    </Tile>
  );
}
