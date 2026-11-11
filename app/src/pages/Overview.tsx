// Genel Bakış: the connected keyboard at a glance. Its drawing shows the
// keyboard's real light (read back over USB about four times a second) and
// the keys held down while the app is in front; under it, quick cards for
// the light, the screen and the focus timer. The connection details and the
// clock live on the Bağlantı page.

import { BatteryCharging, BatteryFull, BatteryMedium, Play, Radio, Usb } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { KeyboardView } from "../components/KeyboardView";
import { useGauges } from "../lib/gauges";
import { hidFromCode } from "../lib/hidCodes";
import { connectionKey, percent, useAgo, useT, type MessageKey } from "../lib/i18n";
import { useLighting } from "../lib/lighting";
import { useLive } from "../lib/live";
import { useMirror } from "../lib/mirror";
import { DEFAULT_SETTINGS, fmtClock, phaseColor, remainingNow, usePomodoro } from "../lib/pomodoro";
import { useScreen } from "../lib/screen";
import { useApp } from "../lib/store";
import { Thumb } from "./screen/common";

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

      {/* The keyboard, as it is lit right now. */}
      <section className="card overflow-hidden p-0" data-overview-keyboard>
        <div className="p-6 pb-4" style={{ background: "var(--plate)" }}>
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
        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line px-5 py-3 text-[13px]">
          <span className="flex items-center gap-2 text-ink-2" data-overview-live={live ? (lightsOff ? "off" : "on") : "unavailable"}>
            <span className={`h-2 w-2 rounded-full ${live && mirror ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
            {!live ? t("overview.kb.usbOnly") : lightsOff ? t("overview.kb.off") : t("overview.kb.live")}
          </span>
          <span className="text-ink-3">{t("overview.kb.press")}</span>
        </div>
      </section>

      {/* Room for a fourth card (profiles) in the same row. */}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(210px,1fr))] gap-4" data-overview-cards>
        <LightCard />
        <ScreenCard />
        <FocusCard />
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

/** A small card in the row under the keyboard: a title, what is set now and
 *  a way to it. */
function QuickCard({
  title,
  data,
  children,
  link,
  onLink,
}: {
  title: MessageKey;
  data: string;
  children: ReactNode;
  link: MessageKey;
  onLink: () => void;
}) {
  const t = useT();
  return (
    <section className="card flex min-w-0 flex-col gap-2.5 p-4" data-quick={data}>
      <h2 className="text-[13px] font-semibold">{t(title)}</h2>
      {children}
      <button type="button" onClick={onLink} className="mt-auto self-start rounded-md pt-1 text-[13px] text-signal hover:underline">
        {t(link)}
      </button>
    </section>
  );
}

/** The light the app last stored on the keyboard. Only the Lighting page
 *  changes it, with its own save step. */
function LightCard() {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  const applied = useLighting((s) => s.appliedEffect);
  const saved = useLighting((s) => s.savedPerKey);
  const gauges = useGauges((s) => !!s.state?.settings.enabled);
  const name = saved ? t("light.tab.perKey") : applied ? t(`fx.${applied.mode}` as MessageKey) : null;
  const level = saved ? saved.brightness : applied && applied.mode !== 0 ? applied.brightness : null;
  return (
    <QuickCard title="overview.light" data="light" link="overview.light.go" onLink={() => setPage("lighting")}>
      <span className={`truncate text-[15px] ${name ? "" : "text-ink-3"}`}>{name ?? t("overview.light.unknown")}</span>
      {level != null && (
        <div className="flex items-center gap-3 text-[12px] text-ink-3">
          <span>{t("light.brightness")}</span>
          <span
            role="meter"
            aria-label={t("light.brightness")}
            aria-valuemin={0}
            aria-valuemax={5}
            aria-valuenow={level}
            className="flex flex-1 gap-1"
          >
            {[1, 2, 3, 4, 5].map((i) => (
              <span key={i} className={`h-1.5 flex-1 rounded-full ${i <= level ? "bg-signal" : "bg-surface-2"}`} />
            ))}
          </span>
        </div>
      )}
      {gauges && <span className="text-[12px] text-ink-3">{t("overview.light.gauges")}</span>}
    </QuickCard>
  );
}

/** What the keyboard's GIF page shows, as far as the app knows. */
function ScreenCard() {
  const t = useT();
  const openScreen = useApp((s) => s.openScreen);
  const state = useScreen((s) => s.state);
  const liveOn = useLive((s) => !!s.state?.enabled);
  const liveName = useLive((s) => s.state?.last?.name ?? null);
  const showsLive = state?.shown === "live";
  const shown = state?.items.find((i) => i.id === state.shown) ?? null;
  const factory = !!state?.items.some((i) => i.kind === "factory");
  return (
    <QuickCard title="nav.screen" data="screen" link="overview.screen.go" onLink={() => openScreen(liveOn ? "live" : "visuals")}>
      {showsLive ? (
        <div className="flex items-center gap-3">
          <span className="flex h-[68px] w-[68px] shrink-0 items-center justify-center rounded-md bg-surface-2 text-ok">
            <Radio size={20} aria-hidden />
          </span>
          <span className="flex min-w-0 flex-col">
            <span className="text-[13px] font-medium">{t("overview.screen.live")}</span>
            {liveName && <span className="truncate text-[12px] text-ink-3">{liveName}</span>}
          </span>
        </div>
      ) : shown ? (
        <div className="flex min-w-0 items-center gap-3">
          <Thumb item={shown} />
          <span className="min-w-0 truncate text-[13px]" title={shown.name}>
            {shown.name}
          </span>
        </div>
      ) : (
        <span className="text-[13px] text-ink-3">{t(factory ? "sc.page.gif.maybeFactory" : "overview.screen.unknown")}</span>
      )}
      {liveOn && !showsLive && <span className="text-[12px] text-ink-3">{t("overview.screen.liveOn")}</span>}
    </QuickCard>
  );
}

/** Start the focus timer from here; while it runs, the time left. */
function FocusCard() {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  const p = usePomodoro((x) => x.state);
  const control = usePomodoro((x) => x.control);
  const running = p?.status === "running";
  const [, tick] = useState(0);
  useEffect(() => {
    if (!running) return;
    const id = window.setInterval(() => tick((n) => n + 1), 500);
    return () => window.clearInterval(id);
  }, [running]);
  const s = p?.settings ?? DEFAULT_SETTINGS;
  const busy = !!p && p.status !== "idle";
  const phase = !p
    ? null
    : p.phase === "focus"
      ? t("pomo.phase.focus")
      : p.phase === "shortBreak"
        ? t("pomo.phase.shortBreak")
        : t("pomo.phase.longBreak");
  return (
    <QuickCard title="nav.focus" data="focus" link="overview.focus.go" onLink={() => setPage("focus")}>
      {busy && p ? (
        <span className="flex items-baseline gap-2">
          <span className="num text-[20px] font-semibold" style={{ color: phaseColor(p.settings, p.phase) }}>
            {fmtClock(remainingNow(p))}
          </span>
          <span className="text-[12px] text-ink-3">{p.status === "paused" ? t("pomo.paused") : phase}</span>
        </span>
      ) : (
        <>
          <span className="text-[15px]">{t("overview.focus.plan", { focus: s.focusMin, rest: s.shortMin })}</span>
          <button
            type="button"
            onClick={() => control("start").catch(console.error)}
            className="flex h-9 items-center justify-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink transition-transform active:scale-[0.98]"
            data-quick-start
          >
            <Play size={14} aria-hidden />
            {t("overview.focus.start")}
          </button>
        </>
      )}
    </QuickCard>
  );
}
