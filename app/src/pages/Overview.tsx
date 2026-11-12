// Genel Bakış: the connected keyboard at a glance. Its drawing shows the
// keyboard's real light (read back over USB about four times a second) and
// the keys held down while the app is in front, across the whole width so
// the keys' letters stay readable. Under it, the picture on the keyboard's
// screen and one tile for each thing the app does (light, focus timer,
// reminders, calendar, system gauges, profile), each saying what is on now
// and linking to its page. The connection details
// and the clock live on the Bağlantı page.

import { BatteryCharging, BatteryFull, BatteryMedium, Usb } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { KeyboardView } from "../components/KeyboardView";
import { hidFromCode } from "../lib/hidCodes";
import { connectionKey, percent, useAgo, useT } from "../lib/i18n";
import { useMirror } from "../lib/mirror";
import { useApp, useNarrowWindow } from "../lib/store";
import { FocusTile } from "./overview/FocusTile";
import { ScreenNow } from "./overview/ScreenNow";
import { CalendarTile, GaugesTile, LightTile, ProfileTile, RemindersTile } from "./overview/tiles";

/** LEDs of the keys held down while the window has focus. The Fn key and
 *  keys the system keeps (Win, Print Screen) send nothing here. */
function usePressedKeys(geometry: GeometryInfo | null): ReadonlySet<number> {
  const [down, setDown] = useState<ReadonlySet<number>>(new Set());
  useEffect(() => {
    if (!geometry) return;
    const byHid = new Map<number, number[]>();
    for (const k of geometry.keys) byHid.set(k.hid, [...(byHid.get(k.hid) ?? []), k.led]);
    const leds = (e: KeyboardEvent) => {
      const hid = hidFromCode(e.code);
      return hid == null ? [] : (byHid.get(hid) ?? []);
    };
    const press = (e: KeyboardEvent) => {
      if (e.repeat) return;
      const l = leds(e);
      if (l.length) setDown((s) => new Set([...s, ...l]));
    };
    const release = (e: KeyboardEvent) => {
      const l = new Set(leds(e));
      if (l.size) setDown((s) => new Set([...s].filter((x) => !l.has(x))));
    };
    const clear = () => setDown(new Set());
    window.addEventListener("keydown", press);
    window.addEventListener("keyup", release);
    window.addEventListener("blur", clear);
    return () => {
      window.removeEventListener("keydown", press);
      window.removeEventListener("keyup", release);
      window.removeEventListener("blur", clear);
    };
  }, [geometry]);
  return down;
}

const isRed = (c: string | undefined) => {
  if (!c) return false;
  const byte = (i: number) => parseInt(c.slice(i, i + 2), 16);
  return byte(1) > 180 && byte(3) < 60 && byte(5) < 60;
};

/** Readings in a row (about 2 s at four a second) before the charge state
 *  is shown, so a passing red in a moving effect does not count. */
const CHARGE_STEADY = 8;

/** Charging or fully charged, read from the Fn light over USB. The firmware
 *  lights Fn red while the battery charges and turns it off when it is full,
 *  whatever the effect (AK980 PRO: red while charging on 2026-11-10; off with
 *  a full battery on the keyboard's own screen, owner, 2026-11-11). There is
 *  no battery level over USB, but this tells the two apart. A red effect
 *  would light the keys next to Fn too, so those must not be red. */
function useChargeState(kb: KeyboardInfo, mirror: Record<number, string> | null): "charging" | "full" | null {
  const [state, setState] = useState<"charging" | "full" | null>(null);
  const run = useRef<{ red: boolean; n: number }>({ red: false, n: 0 });
  const led = (id: string) => kb.geometry?.keys.find((k) => k.id === id)?.led;
  const [fn, space, rctrl] = [led("fn"), led("space"), led("rctrl")];
  useEffect(() => {
    if (!mirror || fn == null || Object.keys(mirror).length === 0) {
      run.current = { red: false, n: 0 };
      setState(null);
      return;
    }
    const neighboursRed = [space, rctrl].every((l) => l != null && isRed(mirror[l]));
    const red = isRed(mirror[fn]) && !neighboursRed;
    run.current = run.current.red === red ? { red, n: run.current.n + 1 } : { red, n: 1 };
    if (run.current.n >= CHARGE_STEADY) setState(red ? "charging" : "full");
  }, [mirror, fn, space, rctrl]);
  return state;
}

/** The keyboard firmware version ("1.06"), when the keyboard reports it. */
function firmwareOf(kb: KeyboardInfo): string | null {
  return kb.firmware?.trim() || null;
}

/** "%98 düzen, 97 tuş, 240×135 ekran ve döner düğme." in the UI language. */
function useSpecs(kb: KeyboardInfo): string {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const layout = /^\d+$/.test(kb.layout) ? percent(Number(kb.layout)) : kb.layout;
  const parts = [
    t("overview.spec.layout", { layout }),
    t("overview.spec.keys", { count: kb.keyCount }),
    kb.screen ? t("overview.spec.screen", { size: `${kb.screen.width}×${kb.screen.height}` }) : null,
    kb.hasKnob ? t("overview.spec.knob") : null,
  ].filter((p): p is string => !!p);
  const list = new Intl.ListFormat(locale === "tr" ? "tr-TR" : "en-US", { type: "conjunction" }).format(parts);
  const firmware = firmwareOf(kb);
  return [t("overview.spec", { list }), firmware ? t("overview.firmware", { version: firmware }) : null].filter(Boolean).join(" ");
}

function Pill({ children, data }: { children: ReactNode; data?: string }) {
  return (
    <span className="flex h-8 items-center gap-2 rounded-md border border-line bg-surface px-3 text-[13px] whitespace-nowrap" data-pill={data}>
      {children}
    </span>
  );
}

export function Overview({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const ago = useAgo();
  const usb = kb.connection === "usb";
  const live = usb && kb.hasCommandChannel && kb.features.includes("lighting");
  const mirror = useMirror(live);
  const pressed = usePressedKeys(kb.geometry ?? null);
  const pressedClass = useMemo(() => Object.fromEntries([...pressed].map((l) => [l, "is-pressed"])), [pressed]);
  // Off when nothing but the indicator keys (Num Lock, Caps Lock, Fn) is lit.
  const lit = mirror ? Object.values(mirror).filter((c) => c !== "#000000").length : 0;
  const lightsOff = !!mirror && Object.keys(mirror).length > 0 && lit <= 3;
  const charge = useChargeState(kb, live ? mirror : null);
  const specs = useSpecs(kb);
  const lastRead = kb.battery ? t("overview.bat.read", { level: percent(kb.battery.percent), ago: ago(kb.battery.readAtMs) }) : undefined;
  const narrow = useNarrowWindow();

  return (
    <div className="flex flex-col gap-5" data-overview>
      <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
        <div className="min-w-0">
          <h1 className="page-title">{kb.modelName}</h1>
          <p className="page-desc num" data-overview-specs>
            {specs}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Pill data="connection">
            <span className="h-2 w-2 rounded-full bg-ok" aria-hidden />
            {t(connectionKey(kb.connection))}
          </Pill>
          {usb && charge && (
            <span title={lastRead} data-overview-charge={charge}>
              <Pill>
                {charge === "full" ? (
                  <BatteryFull size={16} className="text-ok" aria-hidden />
                ) : (
                  <BatteryCharging size={16} className="text-ok" aria-hidden />
                )}
                {t(charge === "charging" ? "overview.bat.charging" : "overview.bat.full")}
              </Pill>
            </span>
          )}
          {!usb && kb.battery && (
            <span title={lastRead} data-overview-battery>
              <Pill>
                <BatteryMedium size={16} className="text-ok" aria-hidden />
                <span className="num">{t("overview.bat.level", { level: percent(kb.battery.percent) })}</span>
              </Pill>
            </span>
          )}
        </div>
      </header>

      {/* The keyboard, as it is lit right now: the whole width, so the
          keys' letters and numbers stay readable (K12, option A). */}
      <section className="card flex min-w-0 flex-col overflow-hidden p-0" data-overview-keyboard>
        <div className="p-6 pb-5" style={{ background: "var(--plate)" }}>
          {kb.geometry && (
            <KeyboardView
              geometry={kb.geometry}
              battery={kb.battery?.percent ?? null}
              connection={kb.connection}
              colors={live ? (mirror ?? {}) : {}}
              keyClass={pressedClass}
            />
          )}
        </div>
        <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 border-t border-line px-5 py-3 text-[13px]">
          <span className="flex items-center gap-2 text-ink-2" data-overview-live={live ? (lightsOff ? "off" : "on") : "unavailable"}>
            <span className={`h-2 w-2 rounded-full ${live && mirror ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
            {!live ? t("overview.kb.usbOnly") : lightsOff ? t("overview.kb.off") : t("overview.kb.live")}
          </span>
          <span className="text-ink-3">{t("overview.kb.press")}</span>
        </div>
      </section>

      {/* One tile for each thing the app does; the keyboard's screen first,
          two rows tall so its picture stays large. */}
      <div className={`grid gap-4 ${narrow ? "grid-cols-2" : "grid-cols-3"}`} data-overview-cards>
        {kb.screen && <ScreenNow className="row-span-2" />}
        <LightTile />
        <FocusTile />
        <RemindersTile />
        <CalendarTile />
        {/* Wide window with the screen tile: the gauges take two columns so
            the last row is full. */}
        <div className={`grid min-w-0 ${kb.screen && !narrow ? "col-span-2" : ""}`}>
          <GaugesTile geometry={kb.geometry ?? null} usb={usb && kb.hasCommandChannel} />
        </div>
        <ProfileTile modelId={kb.modelId} />
      </div>

      {!usb && (
        <section className="card flex gap-4 p-5" data-overview-unlock>
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-signal/15 text-signal">
            <Usb size={20} aria-hidden />
          </div>
          <div className="flex-1">
            <h2 className="eyebrow">{t("overview.unlock.title")}</h2>
            <p className="mt-0.5 max-w-[70ch] text-[13px] text-ink-2">{t("overview.unlock.body")}</p>
          </div>
        </section>
      )}
    </div>
  );
}
