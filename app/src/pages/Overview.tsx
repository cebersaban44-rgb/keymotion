import { Check, Lock, Usb } from "lucide-react";
import { motion } from "motion/react";
import type { Feature } from "../bindings/Feature";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { BatteryGauge } from "../components/BatteryGauge";
import { ClockCard } from "../components/ClockCard";
import { KeyboardView } from "../components/KeyboardView";
import { ModeSwitch } from "../components/ModeSwitch";
import { connectionKey, featureKey, useAgo, useT } from "../lib/i18n";

const ALL_FEATURES: Feature[] = ["lighting", "per-key-rgb", "keymap", "macros", "options", "screen", "clock", "battery"];

const rise = (i: number) => ({
  initial: { opacity: 0, y: 14 },
  animate: { opacity: 1, y: 0 },
  transition: { delay: 0.08 * i, duration: 0.5, ease: [0.2, 0.7, 0.2, 1] as const },
});

export function Overview({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const ago = useAgo();
  const full = kb.connection === "usb";
  const bleChannel = kb.bleName?.match(/-(\d)$/)?.[1];
  const usbFeatures = kb.modes.find((m) => m.connection === "usb")?.features ?? [];

  return (
    <div className="flex flex-col gap-6">
      {/* Hero */}
      <motion.header {...rise(0)} className="flex items-end justify-between gap-6">
        <div>
          <div className="eyebrow mb-2">{t("device.detected")}</div>
          <h1 className="font-display text-[44px] leading-[0.95] font-semibold tracking-[-0.035em]">
            <span className="text-ink-3">{kb.vendor} </span>
            {kb.modelName.replace(`${kb.vendor} `, "")}
          </h1>
          <div className="num mt-3 flex flex-wrap gap-x-5 gap-y-1 text-[12.5px] text-ink-2">
            <span>%{kb.layout}</span>
            <span>
              {kb.keyCount} {t("overview.specs.keys")}
            </span>
            {kb.screen && (
              <span>
                {kb.screen.width}×{kb.screen.height} {t("overview.specs.screen")}
              </span>
            )}
            {kb.hasKnob && <span>{t("overview.specs.knob")}</span>}
          </div>
        </div>
        <div className="flex items-center gap-2 rounded-full border border-line bg-surface px-3.5 py-1.5">
          <span className="h-1.5 w-1.5 rounded-full bg-ok shadow-[0_0_8px_var(--ok)]" />
          <span className="text-[12.5px]">
            {t("device.connected")} · {t(connectionKey(kb.connection))}
          </span>
        </div>
      </motion.header>

      {/* Keyboard plate */}
      <motion.section
        {...rise(1)}
        className="relative overflow-hidden rounded-[26px] border border-line p-7 pb-5"
        style={{
          background:
            "radial-gradient(120% 90% at 50% 120%, color-mix(in oklch, var(--signal) 10%, transparent), transparent 60%), linear-gradient(180deg, var(--plate), color-mix(in oklch, var(--plate) 80%, black))",
        }}
      >
        <div
          className="pointer-events-none absolute inset-0 opacity-[0.35]"
          style={{
            backgroundImage:
              "linear-gradient(rgb(255 255 255 / 0.035) 1px, transparent 1px), linear-gradient(90deg, rgb(255 255 255 / 0.035) 1px, transparent 1px)",
            backgroundSize: "28px 28px",
            maskImage: "radial-gradient(80% 80% at 50% 50%, black, transparent)",
          }}
        />
        {kb.geometry && (
          <div className="relative">
            <KeyboardView geometry={kb.geometry} battery={kb.battery?.percent ?? null} connection={kb.connection} />
          </div>
        )}
        <div className="relative mt-5 flex items-center justify-between text-[11.5px] text-[#8b8d92]">
          <span className="num">{t("overview.preview")}</span>
          {kb.geometry && (
            <span className="num rounded-full border border-white/10 px-2.5 py-0.5">
              {kb.geometry.keys.length} {t("overview.specs.keys")} · {t("overview.layoutMeasured")}
            </span>
          )}
        </div>
      </motion.section>

      {/* Instruments */}
      <div className="grid grid-cols-[1.15fr_0.85fr_1.2fr] gap-5">
        <motion.section {...rise(2)} className="card flex flex-col gap-5 p-5">
          <div>
            <div className="eyebrow">{t("overview.mode.title")}</div>
            <div className="mt-1 text-[12.5px] text-ink-3">{t("overview.mode.hint")}</div>
          </div>
          <ModeSwitch value={kb.connection} />
          <dl className="num grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[12px]">
            {kb.bleName && (
              <>
                <dt className="text-ink-3">BLE</dt>
                <dd className="text-right">{kb.bleName}</dd>
              </>
            )}
            {kb.bleAddress && (
              <>
                <dt className="text-ink-3">MAC</dt>
                <dd className="text-right">{kb.bleAddress}</dd>
              </>
            )}
            {bleChannel && (
              <>
                <dt className="text-ink-3">{t("overview.mode.channel")}</dt>
                <dd className="text-right">{bleChannel} / 3</dd>
              </>
            )}
            {kb.product && (
              <>
                <dt className="text-ink-3">USB</dt>
                <dd className="text-right">{kb.product}</dd>
              </>
            )}
          </dl>
        </motion.section>

        <motion.section {...rise(3)} className="card flex flex-col items-center gap-2 p-5">
          <div className="eyebrow self-start">{t("overview.battery.title")}</div>
          <BatteryGauge percent={kb.battery?.percent ?? null} />
          <div className="num text-center text-[11.5px] text-ink-3">
            {kb.battery
              ? `${t("overview.battery.source")} · ${ago(kb.battery.readAtMs)} ${t("overview.battery.ago")}`
              : t("overview.battery.unknown")}
          </div>
        </motion.section>

        <motion.section {...rise(4)} className="card flex flex-col gap-3 p-5">
          <div className="eyebrow">{t("overview.features.title")}</div>
          <ul className="flex flex-col gap-1.5">
            {ALL_FEATURES.map((f) => {
              const on = kb.features.includes(f);
              return (
                <li key={f} className="flex items-center gap-2.5 text-[13px]">
                  <span
                    className={`flex h-5 w-5 items-center justify-center rounded-md ${
                      on ? "bg-ok/15 text-ok" : "bg-surface-2 text-ink-3"
                    }`}
                  >
                    {on ? <Check size={12} strokeWidth={2.5} /> : <Lock size={11} />}
                  </span>
                  <span className={on ? "text-ink" : "text-ink-3"}>{t(featureKey(f))}</span>
                  {!on && usbFeatures.includes(f) && (
                    <span className="num ml-auto text-[10.5px] text-ink-3">{t("overview.features.locked")}</span>
                  )}
                </li>
              );
            })}
          </ul>
        </motion.section>
      </div>

      {(kb.features.includes("clock") || usbFeatures.includes("clock")) && (
        <motion.section {...rise(5)}>
          <ClockCard usb={kb.features.includes("clock")} />
        </motion.section>
      )}

      {!full && (
        <motion.section
          {...rise(6)}
          className="relative flex gap-6 overflow-hidden rounded-[20px] border border-signal/30 p-6"
          style={{ background: "linear-gradient(100deg, color-mix(in oklch, var(--signal) 12%, var(--surface)), var(--surface) 55%)" }}
        >
          <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-signal text-signal-ink">
            <Usb size={22} strokeWidth={2} />
          </div>
          <div className="flex-1">
            <h2 className="font-display text-[20px] font-semibold tracking-[-0.02em]">{t("overview.unlock.title")}</h2>
            <p className="mt-1 max-w-[62ch] text-[13px] text-ink-2">{t("overview.unlock.body")}</p>
            <ol className="mt-4 grid grid-cols-3 gap-3">
              {(["overview.unlock.step1", "overview.unlock.step2", "overview.unlock.step3"] as const).map((k, i) => (
                <li key={k} className="flex items-start gap-2.5 rounded-xl border border-line bg-bg-raise/60 px-3 py-2.5 text-[12.5px]">
                  <span className="num mt-px text-signal">0{i + 1}</span>
                  <span className="text-ink-2">{t(k)}</span>
                </li>
              ))}
            </ol>
          </div>
        </motion.section>
      )}
    </div>
  );
}
