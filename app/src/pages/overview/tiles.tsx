// The Overview's rows under the keyboard ("Status"), one for each thing the
// app does: light, focus timer (its own file), calendar, reminders, system
// gauges, the keyboard's screen and profile. Each says what is on now, or
// plainly that there is nothing, and opens its page; reminders and gauges
// switch on and off right here, as on their pages.

import { BellRing, CalendarDays, Gauge, Layers, Monitor, Sparkles } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import type { EffectInfo } from "../../bindings/EffectInfo";
import type { GeometryInfo } from "../../bindings/GeometryInfo";
import { Row, Toggle } from "../../components/controls";
import { calendarColor, hhmm, useCalendar } from "../../lib/calendar";
import { useGauges, watchGauges } from "../../lib/gauges";
import { listOf, percent, useT, type MessageKey } from "../../lib/i18n";
import { getLightingCatalog } from "../../lib/ipc";
import { useLive } from "../../lib/live";
import { PALETTE, useLighting, type ColorMap } from "../../lib/lighting";
import { profileName, profileOnKeyboard, useProfiles } from "../../lib/profiles";
import { rgbCss, useReminders } from "../../lib/reminders";
import { useApp } from "../../lib/store";
import { ICON_OF, when } from "../Reminders";
import { useGifPage } from "../screen/common";
import { Swatch, useClock, useInText } from "./Tile";

/** "A · B · C", leaving out the parts that are empty. */
function dots(parts: (ReactNode | null | false | undefined)[]): ReactNode {
  const shown = parts.filter((p): p is ReactNode => p !== null && p !== false && p !== undefined && p !== "");
  return shown.map((p, i) => (
    <span key={i}>
      {i > 0 && " · "}
      {p}
    </span>
  ));
}

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
export function LightRow() {
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

  let colorText: string | null = null;
  let swatches: ReactNode = null;
  if (saved && design.length) {
    swatches = (
      <span className="flex flex-wrap items-center gap-1.5" data-light-colors={design.length}>
        {design.slice(0, MAX_SWATCHES).map((hex) => (
          <Swatch key={hex} color={hex} label={colorName(hex)} />
        ))}
        {design.length > MAX_SWATCHES && <span className="num text-[12px] text-ink-2">+{design.length - MAX_SWATCHES}</span>}
      </span>
    );
  } else if (applied && applied.mode !== 0 && info) {
    const rainbow = info.color && info.rainbow && applied.rainbow;
    colorText = info.color ? (rainbow ? t("light.rainbow") : (colorName(applied.color) ?? null)) : t("overview.light.ownColors");
    swatches = (
      <span data-light-colors={info.color ? (rainbow ? "rainbow" : "one") : "own"}>
        {info.color && !rainbow ? <Swatch color={applied.color} /> : <Swatch rainbow />}
      </span>
    );
  }

  return (
    <Row
      icon={Sparkles}
      title={t("overview.light")}
      desc={
        <>
          {name ? (
            dots([name, colorText, level != null && <span className="num">{t("overview.light.level", { level })}</span>])
          ) : (
            t("overview.light.unknown")
          )}
          {gauges && <span className="block">{t("overview.light.gauges")}</span>}
        </>
      }
      onOpen={() => setPage("lighting")}
      openLabel={t("overview.light.go")}
      data="light"
    >
      {swatches}
    </Row>
  );
}

// -------------------------------------------------------------- reminders

/** The next reminder and how many are on; reminders switch on and off here. */
export function RemindersRow() {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const setPage = useApp((s) => s.setPage);
  const s = useReminders((x) => x.state);
  const configure = useReminders((x) => x.configure);
  const now = useClock();
  const inText = useInText();
  const list = s?.reminders ?? [];
  const on = list.filter((i) => i.reminder.enabled).length;
  const next = list.filter((i) => i.nextMs != null).sort((a, b) => a.nextMs! - b.nextMs!)[0];
  // As the Reminders page says it: off, away, the next one, none ahead, none at all.
  const away = !!s && s.away && s.settings.awayWait && list.some((i) => i.reminder.enabled && i.reminder.when.kind === "every");
  const Icon = next ? ICON_OF[next.reminder.icon] : null;
  const enabled = !!s?.settings.enabled;

  let desc: ReactNode = null;
  if (s) {
    if (!enabled) desc = t("reminder.status.off");
    else if (next && Icon && !away)
      desc = (
        <span className="flex min-w-0 items-center gap-1.5" data-overview-reminder>
          <Icon size={13} style={{ color: rgbCss(next.reminder.color) }} className="shrink-0" aria-hidden />
          <span className="num truncate">
            {[next.reminder.title, when(next.nextMs!, t, locale), inText(next.nextMs!, now)].filter(Boolean).join(" · ")}
          </span>
        </span>
      );
    else desc = t(away ? "reminder.status.away" : list.length ? "reminder.status.none" : "reminder.status.empty");
  }

  return (
    <Row
      icon={BellRing}
      title={t("nav.reminders")}
      desc={desc}
      onOpen={() => setPage("reminders")}
      openLabel={t("overview.rem.go")}
      data="reminders"
    >
      {s && (
        <span className="flex items-center gap-3 text-[14px]" data-tile-badge={enabled ? "on" : "off"}>
          <span className="num" aria-hidden>
            {enabled && on > 0 ? t("overview.rem.on", { count: on }) : t(enabled ? "overview.on" : "overview.off")}
          </span>
          <Toggle checked={enabled} onChange={(v) => configure({ enabled: v }).catch(console.error)} label={t("reminder.on")} />
        </span>
      )}
    </Row>
  );
}

// --------------------------------------------------------------- calendar

/** The next meeting, or that there is none (or no calendar). */
export function CalendarRow() {
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

  let desc: ReactNode = null;
  if (c) {
    if (c.sources.length === 0) desc = t("overview.cal.none");
    else if (next)
      desc = (
        <span className="flex min-w-0 items-center gap-1.5" data-overview-meeting>
          <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: calendarColor(next.color) }} aria-hidden />
          <span className="num truncate" title={next.title}>
            {[next.title, time, running ? t("overview.cal.now") : inText(next.startMs, now)].filter(Boolean).join(" · ")}
          </span>
        </span>
      );
    else desc = t("calendar.upcoming.none");
  }

  return (
    <Row
      icon={CalendarDays}
      title={t("nav.calendar")}
      desc={desc}
      onOpen={() => setPage("calendar")}
      openLabel={t("overview.cal.go")}
      data="calendar"
    />
  );
}

// ----------------------------------------------------------------- gauges

const LOADS = [
  ["cpu", "gauges.r.cpu"],
  ["ram", "gauges.r.ram"],
  ["gpu", "gauges.r.gpu"],
] as const;

/** On or off (switched here, as on its page); while on, the loads the keys show, read live. */
export function GaugesRow({ geometry, usb }: { geometry: GeometryInfo | null; usb: boolean }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const setPage = useApp((s) => s.setPage);
  const s = useGauges((x) => x.state);
  const configure = useGauges((x) => x.configure);
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
  const list = listOf(chosen, locale);

  let desc: ReactNode = null;
  if (s) {
    if (on && loads.length > 0)
      desc = (
        <span className="num" data-overview-loads>
          {loads
            .map(([id, label]) => `${t(label)} ${s.readings[id] == null ? t("gauges.r.none") : percent(Math.round(s.readings[id]!))}`)
            .join(" · ")}
        </span>
      );
    else if (on) desc = chosen.length ? t("overview.gauges.shown", { list }) : t("overview.gauges.nothing");
    else desc = chosen.length > 0 ? `${t("overview.gauges.off")} ${t("overview.gauges.chosen", { list })}` : t("overview.gauges.off");
  }

  return (
    <Row
      icon={Gauge}
      title={t("nav.gauges")}
      desc={
        <>
          {desc}
          {on && !usb && <span className="block">{t("gauges.status.noUsb")}</span>}
        </>
      }
      onOpen={() => setPage("gauges")}
      openLabel={t("light.gaugesGo")}
      data="gauges"
    >
      {s && (
        <span data-tile-badge={on ? "on" : "off"}>
          <Toggle state checked={on} onChange={(v) => configure({ enabled: v }).catch(console.error)} label={t("gauges.on")} />
        </span>
      )}
    </Row>
  );
}

// ----------------------------------------------------------------- screen

/** What the keyboard's screen shows (the picture itself is on the glass
 *  strip above), the live screen's state and a card that waits to go. */
export function ScreenRow() {
  const t = useT();
  const openScreen = useApp((s) => s.openScreen);
  const gif = useGifPage();
  const liveOn = useLive((s) => (s.state ? s.state.enabled : null));
  // Whole uploads the live screen may use now.
  const left = useLive((s) => (s.state ? Math.max(0, Math.floor(s.state.tokens + 1e-6)) : 0));
  return (
    <Row
      icon={Monitor}
      title={t("overview.screen.title")}
      desc={
        <>
          {dots([
            <span key="c" data-overview-screen-caption>
              {gif.caption}
            </span>,
            liveOn !== null && (
              <span key="l" className="num" data-overview-live-screen={liveOn ? "on" : "off"}>
                {liveOn ? t("overview.screen.liveLeft", { count: left }) : t("overview.screen.liveOff")}
              </span>
            ),
          ])}
          {gif.waiting && (
            <span className="block text-warn" data-overview-waiting>
              {t("overview.screen.waiting", { name: gif.waiting.name })}
            </span>
          )}
        </>
      }
      onOpen={() => openScreen(liveOn ? "live" : "visuals")}
      openLabel={t("overview.screen.go")}
      data="screen"
    />
  );
}

// ---------------------------------------------------------------- profile

/** The profile the keyboard has (all its parts match this computer's
 *  records), or that it matches none. */
export function ProfileRow({ modelId }: { modelId: string }) {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  const state = useProfiles((s) => (s.modelId === modelId ? s.state : null));
  useEffect(() => {
    useProfiles.getState().load(modelId).catch(console.error);
  }, [modelId]);
  const match = state ? profileOnKeyboard(state) : null;
  return (
    <Row
      icon={Layers}
      title={t("overview.profile")}
      desc={
        state &&
        (match ? (
          <span title={profileName(match, t)} data-overview-profile={match.id}>
            {profileName(match, t)}
          </span>
        ) : (
          <span data-overview-profile="none">{t("overview.profile.none")}</span>
        ))
      }
      onOpen={() => setPage("profiles")}
      openLabel={t("overview.profile.go")}
      data="profile"
    />
  );
}
