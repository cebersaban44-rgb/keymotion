// Genel Bakış: the connected keyboard at a glance (design A+E). At the top
// the keyboard's glass strip (its screen, the picture on the glass and the
// knob), under it one line with the keyboard's name and connection, then
// the drawing with the keyboard's real light (read back over USB about four
// times a second) and the keys held down while the app is in front, across
// the whole width so the keys' letters stay readable. Under it one row for
// each thing the app does (light, focus timer, calendar, reminders, system
// gauges, the screen, profile), each saying what is on now and opening its
// page. The connection details and the clock live on the Bağlantı page.

import { BatteryCharging, BatteryFull, BatteryMedium, Radio, Usb } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { KeyboardView } from "../components/KeyboardView";
import { BetaPill, KeyboardNotices } from "../components/KeyboardNotices";
import { GuideInvite } from "../components/TestGuide";
import { hidFromCode } from "../lib/hidCodes";
import { connectionKey, listOf, modelTitle, percent, useAgo, useT } from "../lib/i18n";
import { useMirror } from "../lib/mirror";
import { InfoBar, Rows } from "../components/controls";
import { DARK_UNTIL_PRESSED, useLighting } from "../lib/lighting";
import { useApp } from "../lib/store";
import { FocusRow } from "./overview/FocusRow";
import { GlassStrip } from "./overview/GlassStrip";
import { CalendarRow, GaugesRow, LightRow, ProfileRow, RemindersRow, ScreenRow } from "./overview/tiles";

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

/** Charging or fully charged over USB. A keyboard of the "04 xx" family
 *  shows it on its Fn light ([`useFnLightCharge`]); one that reports it
 *  (`KeyboardInfo.device`, the "AA" family) says so itself. */
function useChargeState(kb: KeyboardInfo, mirror: Record<number, string> | null): "charging" | "full" | null {
  const ff13 = kb.protocol === "sonix-ff13";
  const fromLight = useFnLightCharge(kb, ff13 ? mirror : null);
  return ff13 ? fromLight : (kb.device?.charge ?? null);
}

/** Charging or fully charged, read from the Fn light over USB. The firmware
 *  lights Fn red while the battery charges and turns it off when it is full,
 *  whatever the effect (AK980 PRO: red while charging on 2026-11-10; off with
 *  a full battery on the keyboard's own screen, owner, 2026-11-11). There is
 *  no battery level over USB, but this tells the two apart. A red effect
 *  would light the keys next to Fn too, so those must not be red. */
function useFnLightCharge(kb: KeyboardInfo, mirror: Record<number, string> | null): "charging" | "full" | null {
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

/** "%98 düzen ve 97 tuş" in the UI language; the screen and the knob are
 *  drawn on the glass strip. */
function useSpecs(kb: KeyboardInfo): string {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const layout = /^\d+$/.test(kb.layout) ? percent(Number(kb.layout)) : kb.layout;
  return listOf([t("overview.spec.layout", { layout }), t("overview.spec.keys", { count: kb.keyCount })], locale);
}

/** Whether the light the app last saved is an effect that lights keys only
 *  as they are pressed (Launch, Ripples…), at a brightness above zero: the
 *  keys read back dark between presses, yet the light is on. */
function useLightOnPress(): boolean {
  const applied = useLighting((s) => s.appliedEffect);
  const saved = useLighting((s) => s.savedPerKey);
  return !saved && !!applied && DARK_UNTIL_PRESSED.has(applied.mode) && applied.brightness > 0;
}

/** The brightness the knob is at, as far as the app knows: the level it
 *  last stored with the light. */
function useKnobLevel(): number | null {
  const applied = useLighting((s) => s.appliedEffect);
  const saved = useLighting((s) => s.savedPerKey);
  return saved ? saved.brightness : applied ? applied.brightness : null;
}

export function Overview({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const ago = useAgo();
  const usb = kb.connection === "usb";
  // Only the receiver is seen: whether the keyboard is on behind it is not
  // known yet, so nothing here says it is connected.
  const receiver = kb.connection === "dongle";
  const live = usb && kb.hasCommandChannel && kb.features.includes("lighting");
  const mirror = useMirror(live, kb.protocol);
  const pressed = usePressedKeys(kb.geometry ?? null);
  const pressedClass = useMemo(() => Object.fromEntries([...pressed].map((l) => [l, "is-pressed"])), [pressed]);
  // Off when nothing but the indicator keys (Num Lock, Caps Lock, Fn) is lit,
  // unless the keys light only as they are pressed: then the line says so,
  // pressed or not.
  const lit = mirror ? Object.values(mirror).filter((c) => c !== "#000000").length : 0;
  const onPress = useLightOnPress();
  const lightsOff = !onPress && !!mirror && Object.keys(mirror).length > 0 && lit <= 3;
  const charge = useChargeState(kb, live ? mirror : null);
  const specs = useSpecs(kb);
  // On the cable, a keyboard that reports its battery ("AA") gives its own
  // reading; otherwise the level is the last Bluetooth one.
  const ownReading = usb && kb.batteryOverUsb;
  const lastRead = kb.battery
    ? ownReading
      ? t("overview.bat.own", { ago: ago(kb.battery.readAtMs) })
      : t("overview.bat.read", { level: percent(kb.battery.percent), ago: ago(kb.battery.readAtMs) })
    : undefined;
  const level = useKnobLevel();
  const firmware = firmwareOf(kb);

  return (
    <div className="flex flex-col" data-overview>
      <h1 className="page-title mb-5">{t("nav.overview")}</h1>

      {/* The glass strip, then the keyboard as it is lit right now: the
          whole width, so the keys' letters and numbers stay readable. The
          strip's screen and the drawing start at the same left edge. */}
      <section className="dev" data-overview-keyboard>
        {kb.screen && <GlassStrip level={level} knob={kb.hasKnob} />}
        <div className="dev-caption">
          <span className="text-[14px] font-semibold text-ink">{modelTitle(t, kb.modelId, kb.modelName)}</span>
          <span className="flex items-center gap-[7px] text-[13px] text-ink" data-pill="connection">
            <span className={`h-2 w-2 rounded-full ${receiver ? "bg-ink-3" : "bg-ok"}`} aria-hidden />
            {t(connectionKey(kb.connection))}
          </span>
          {firmware && <span>{t("overview.firmware.short", { version: firmware })}</span>}
          {kb.protocol === "ajazz-aa" && <BetaPill />}
          <span className="num" data-overview-specs>
            {specs}
          </span>
          <span className="ml-auto flex flex-wrap items-center gap-x-4 gap-y-1">
            {usb && charge && (
              <span title={lastRead} className="flex items-center gap-1.5 text-ink" data-overview-charge={charge}>
                {charge === "full" ? (
                  <BatteryFull size={15} className="text-ok" aria-hidden />
                ) : (
                  <BatteryCharging size={15} className="text-ok" aria-hidden />
                )}
                {t(charge === "charging" ? "overview.bat.charging" : "overview.bat.full")}
              </span>
            )}
            {(!usb || kb.batteryOverUsb) && kb.battery && (
              <span title={lastRead} className="flex items-center gap-1.5 text-ink" data-overview-battery>
                <BatteryMedium size={15} className="text-ok" aria-hidden />
                <span className="num">{t("overview.bat.level", { level: percent(kb.battery.percent) })}</span>
              </span>
            )}
            {kb.hasKnob && level != null && <span className="num">{t("overview.knob", { level })}</span>}
          </span>
        </div>
        <div className="dev-plate">
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
        <div className="dev-live">
          <span className="flex items-center gap-2" data-overview-live={live ? (onPress ? "on-press" : lightsOff ? "off" : "on") : "unavailable"}>
            <span className={`h-[7px] w-[7px] rounded-full ${live && mirror ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
            {!live ? t("overview.kb.usbOnly") : onPress ? t("overview.kb.onPress") : lightsOff ? t("overview.kb.off") : t("overview.kb.live")}
          </span>
          <span>{t("overview.kb.press")}</span>
        </div>
      </section>

      {kb.aaStatus && (
        <div className="mt-6 flex flex-col gap-3">
          <KeyboardNotices kb={kb} />
          <GuideInvite kb={kb} />
        </div>
      )}

      {!usb && (
        <InfoBar
          className="mt-6"
          icon={receiver ? <Radio size={20} strokeWidth={1.6} /> : <Usb size={20} strokeWidth={1.6} />}
          title={t(receiver ? "overview.receiver.title" : "overview.unlock.title")}
          data={receiver ? "receiver" : "usb"}
        >
          <span className="block max-w-[80ch]" data-overview-unlock={receiver ? "receiver" : "usb"}>
            {t(receiver ? "overview.receiver.body" : "overview.unlock.body")}
          </span>
        </InfoBar>
      )}

      {/* One row for each thing the app does. */}
      <div className="mt-6">
        <Rows title={t("overview.status")} data="overview">
          <LightRow />
          <FocusRow />
          <CalendarRow />
          <RemindersRow />
          <GaugesRow geometry={kb.geometry ?? null} usb={usb && kb.hasCommandChannel} />
          {kb.screen && <ScreenRow />}
          <ProfileRow modelId={kb.modelId} />
        </Rows>
      </div>
    </div>
  );
}
