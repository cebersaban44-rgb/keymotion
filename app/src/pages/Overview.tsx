// Genel Bakış: the connected keyboard at a glance. Its drawing shows the
// keyboard's real light (read back over USB about four times a second) and
// the keys held down while the app is in front; below, the connection,
// the battery and what this connection allows.

import { BatteryCharging, Check, Lock, Usb } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { Feature } from "../bindings/Feature";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { BatteryGauge } from "../components/BatteryGauge";
import { ClockCard } from "../components/ClockCard";
import { KeyboardView } from "../components/KeyboardView";
import { ModeSwitch } from "../components/ModeSwitch";
import { hidFromCode } from "../lib/hidCodes";
import { connectionKey, featureKey, useAgo, useT } from "../lib/i18n";
import { useMirror } from "../lib/mirror";

const ALL_FEATURES: Feature[] = ["lighting", "per-key-rgb", "keymap", "macros", "options", "screen", "clock", "battery"];

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

export function Overview({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const ago = useAgo();
  const usb = kb.connection === "usb";
  const live = usb && kb.hasCommandChannel && kb.features.includes("lighting");
  const mirror = useMirror(live);
  const pressed = usePressedKeys(kb.geometry ?? null);
  const pressedClass = useMemo(() => Object.fromEntries([...pressed].map((l) => [l, "is-pressed"])), [pressed]);
  const lightsOff = !!mirror && Object.keys(mirror).length > 0 && Object.values(mirror).every((c) => c === "#000000");
  const bleChannel = kb.bleName?.match(/-(\d)$/)?.[1];
  const usbFeatures = kb.modes.find((m) => m.connection === "usb")?.features ?? [];
  const model = kb.modelName.replace(`${kb.vendor} `, "");

  const specs = [
    `${t("overview.spec.layout")} %${kb.layout}`,
    `${kb.keyCount} ${t("overview.specs.keys")}`,
    kb.screen ? `${kb.screen.width}×${kb.screen.height} ${t("overview.specs.screen")}` : null,
    kb.hasKnob ? t("overview.specs.knob") : null,
  ].filter(Boolean) as string[];

  return (
    <div className="flex flex-col gap-6" data-overview>
      <header className="flex items-start justify-between gap-6">
        <div>
          <h1 className="page-title">
            {kb.vendor} {model}
          </h1>
          <ul className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-[13px] text-ink-2" aria-label={t("overview.specs")}>
            {specs.map((s) => (
              <li key={s} className="num">
                {s}
              </li>
            ))}
          </ul>
        </div>
        <div className="flex items-center gap-2 rounded-md border border-line bg-surface px-3 py-1.5 text-[13px]">
          <span className="h-2 w-2 rounded-full bg-ok" aria-hidden />
          {t(connectionKey(kb.connection))}
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

      <div className="grid grid-cols-[repeat(auto-fit,minmax(260px,1fr))] gap-5">
        <section className="card flex flex-col gap-4 p-5">
          <div>
            <h2 className="eyebrow">{t("overview.mode.title")}</h2>
            <p className="mt-0.5 text-[13px] text-ink-3">{t("overview.mode.hint")}</p>
          </div>
          <ModeSwitch value={kb.connection} />
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[13px]">
            {kb.product && (
              <>
                <dt className="text-ink-3">{t("overview.mode.device")}</dt>
                <dd className="text-right">{kb.product}</dd>
              </>
            )}
            {kb.bleName && (
              <>
                <dt className="text-ink-3">{t("overview.mode.btName")}</dt>
                <dd className="text-right">{kb.bleName}</dd>
              </>
            )}
            {bleChannel && (
              <>
                <dt className="text-ink-3">{t("overview.mode.channel")}</dt>
                <dd className="num text-right">{bleChannel} / 3</dd>
              </>
            )}
            {kb.bleAddress && (
              <>
                <dt className="text-ink-3">{t("overview.mode.btAddress")}</dt>
                <dd className="mono text-right text-[12px]">{kb.bleAddress}</dd>
              </>
            )}
          </dl>
        </section>

        <section className="card flex flex-col gap-3 p-5" data-overview-battery>
          <h2 className="eyebrow">{t("overview.battery.title")}</h2>
          <div className="flex items-center justify-center">
            <BatteryGauge percent={kb.battery?.percent ?? null} />
          </div>
          {usb && (
            <p className="flex items-start gap-2 text-[13px] text-ink-2">
              <BatteryCharging size={16} className="mt-0.5 shrink-0 text-ok" aria-hidden />
              {t("overview.bat.cable")}
            </p>
          )}
          <p className="text-[13px] text-ink-3">
            {kb.battery ? t("overview.bat.read").replace("{ago}", ago(kb.battery.readAtMs)) : t("overview.bat.none")}
          </p>
        </section>

        <section className="card flex flex-col gap-3 p-5">
          <h2 className="eyebrow">{t("overview.features.title")}</h2>
          <ul className="flex flex-col gap-2">
            {ALL_FEATURES.map((f) => {
              const on = kb.features.includes(f);
              return (
                <li key={f} className="flex items-center gap-2.5 text-[13px]">
                  <span
                    className={`flex h-5 w-5 items-center justify-center rounded ${on ? "bg-ok/15 text-ok" : "bg-surface-2 text-ink-3"}`}
                    aria-hidden
                  >
                    {on ? <Check size={12} strokeWidth={2.5} /> : <Lock size={11} />}
                  </span>
                  <span className={on ? "text-ink" : "text-ink-3"}>{t(featureKey(f))}</span>
                  {!on && usbFeatures.includes(f) && <span className="ml-auto text-[12px] text-ink-3">{t("overview.features.locked")}</span>}
                </li>
              );
            })}
          </ul>
        </section>
      </div>

      {(kb.features.includes("clock") || usbFeatures.includes("clock")) && <ClockCard usb={kb.features.includes("clock")} />}

      {!usb && (
        <section className="card flex gap-4 p-5" data-overview-unlock>
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-signal/15 text-signal">
            <Usb size={20} aria-hidden />
          </div>
          <div className="flex-1">
            <h2 className="eyebrow">{t("overview.unlock.title")}</h2>
            <p className="mt-0.5 max-w-[70ch] text-[13px] text-ink-2">{t("overview.unlock.body")}</p>
            <ol className="mt-3 flex list-decimal flex-col gap-1 pl-5 text-[13px] text-ink-2">
              {(["overview.unlock.step1", "overview.unlock.step2", "overview.unlock.step3"] as const).map((k) => (
                <li key={k}>{t(k)}</li>
              ))}
            </ol>
          </div>
        </section>
      )}
    </div>
  );
}
