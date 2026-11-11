import { Check, Loader2, RotateCcw, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Calibration } from "../bindings/Calibration";
import type { CalibrationInfo } from "../bindings/CalibrationInfo";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import { useT, type MessageKey } from "../lib/i18n";
import { getCalibration, setCalibration, setLiveFrame } from "../lib/ipc";
import { SWATCHES, toLedColors, type ColorMap } from "../lib/lighting";
import { useActiveKeyboard } from "../lib/store";
import { KeyboardView } from "./KeyboardView";
import { Segmented, Slider, Toggle } from "./controls";
import { friendlyError } from "../lib/errors";

type Triple = [number, number, number];
/** Channel `i` (0–2) of a per-channel triple. */
const ch = (v: Triple, i: number): number => v[i as 0 | 1 | 2];
export type Pattern = "off" | "white" | "ramp" | "palette";

const PATTERNS: Pattern[] = ["off", "white", "ramp", "palette"];
const CHANNELS: { name: MessageKey; color: string }[] = [
  { name: "cal.red", color: "#ff4d42" },
  { name: "cal.green", color: "#35c76f" },
  { name: "cal.blue", color: "#4a7bff" },
];
/** Save once the sliders have been still this long. */
const PERSIST_AFTER_MS = 400;
const GAMMA_STEP = 0.05;
const RAMP_STEPS = 8;

/** The colour bands of a test pattern, left to right. */
export function patternBands(p: Pattern): string[] {
  if (p === "white") return ["#ffffff"];
  if (p === "palette") return SWATCHES;
  if (p === "ramp")
    return Array.from({ length: RAMP_STEPS }, (_, i) => {
      const h = Math.round(((i + 1) / RAMP_STEPS) * 255)
        .toString(16)
        .padStart(2, "0");
      return `#${h}${h}${h}`;
    });
  return [];
}

/** Test pattern colours by LED: bands spread left to right by key centre. */
export function patternColors(g: GeometryInfo, p: Pattern): ColorMap {
  const bands = patternBands(p);
  const map: ColorMap = {};
  if (!bands.length) return map;
  for (const k of g.keys) {
    const t = Math.min(0.9999, Math.max(0, (k.x + k.w / 2) / g.width));
    map[k.led] = bands[Math.floor(t * bands.length)] ?? "#ffffff";
  }
  return map;
}

const sameCalibration = (a: Calibration, b: Calibration) =>
  a.gamma.every((v, i) => Math.abs(v - ch(b.gamma, i)) < 1e-3) &&
  a.gain.every((v, i) => Math.abs(v - ch(b.gain, i)) < 1e-3);

const uniform = (v: Triple) => v.every((x) => Math.abs(x - v[0]) < 1e-3);

type SaveState = { kind: "idle" | "saving" | "saved" } | { kind: "error"; message: string };

/**
 * sRGB → LED calibration editor. Slider moves preview at once (backend
 * re-sends the live frame); values are saved by the backend once they
 * settle. A test pattern can be streamed to the keyboard (RAM only) to
 * compare it with the same pattern on screen.
 */
export function CalibrationCard() {
  const t = useT();
  const kb = useActiveKeyboard();
  const usb = kb?.connection === "usb" && kb.hasCommandChannel;
  const geometry = kb?.geometry ?? null;

  const [info, setInfo] = useState<CalibrationInfo | null>(null);
  const [cal, setCal] = useState<Calibration | null>(null);
  const [perChannel, setPerChannel] = useState(false);
  const [pattern, setPattern] = useState<Pattern>("off");
  const [save, setSave] = useState<SaveState>({ kind: "idle" });

  useEffect(() => {
    getCalibration()
      .then((i) => {
        setInfo(i);
        setCal(i.current);
        setPerChannel(!uniform(i.current.gamma));
      })
      .catch(console.error);
  }, []);

  // Debounced persistence; flushed if the page is left mid-change.
  const persistTimer = useRef<number | undefined>(undefined);
  const latest = useRef<Calibration | null>(null);
  const persist = (c: Calibration) => {
    window.clearTimeout(persistTimer.current);
    persistTimer.current = undefined;
    setSave({ kind: "saving" });
    setCalibration(c, true)
      .then(() => setSave({ kind: "saved" }))
      .catch((e) => setSave({ kind: "error", message: friendlyError(e, t) }));
  };
  useEffect(
    () => () => {
      if (persistTimer.current !== undefined && latest.current) {
        window.clearTimeout(persistTimer.current);
        setCalibration(latest.current, true).catch(console.error);
      }
    },
    [],
  );
  useEffect(() => {
    if (save.kind !== "saved") return;
    const id = window.setTimeout(() => setSave({ kind: "idle" }), 2000);
    return () => window.clearTimeout(id);
  }, [save]);

  const update = (next: Calibration) => {
    setCal(next);
    latest.current = next;
    setCalibration(next, false).catch(console.error);
    window.clearTimeout(persistTimer.current);
    persistTimer.current = window.setTimeout(() => persist(next), PERSIST_AFTER_MS);
  };

  // Stream the test pattern while one is chosen; stop when leaving.
  const colors = useMemo(() => (geometry ? patternColors(geometry, pattern) : {}), [geometry, pattern]);
  const live = usb && pattern !== "off";
  useEffect(() => {
    setLiveFrame(live ? toLedColors(colors) : null).catch(console.error);
  }, [live, colors]);
  useEffect(() => () => void setLiveFrame(null).catch(console.error), []);

  if (!cal || !info) return null;

  const setGamma = (channel: number | "all", v: number) =>
    update({ ...cal, gamma: cal.gamma.map((g, i) => (channel === "all" || i === channel ? v : g)) as Triple });
  const setGain = (channel: number, v: number) =>
    update({ ...cal, gain: cal.gain.map((g, i) => (i === channel ? v : g)) as Triple });
  const togglePerChannel = (on: boolean) => {
    setPerChannel(on);
    if (!on && !uniform(cal.gamma)) {
      const mean = cal.gamma.reduce((a, b) => a + b, 0) / 3;
      setGamma("all", Math.round(mean / GAMMA_STEP) * GAMMA_STEP);
    }
  };
  const reset = () => {
    setCal(info.factory);
    latest.current = info.factory;
    setPerChannel(!uniform(info.factory.gamma));
    persist(info.factory);
  };

  const gammaSlider = (channel: number | "all", label: string, accent?: string, hint?: string) => (
    <Slider
      key={String(channel)}
      label={label}
      hint={hint}
      value={ch(cal.gamma, channel === "all" ? 0 : channel)}
      min={info.gammaMin}
      max={info.gammaMax}
      step={GAMMA_STEP}
      accent={accent}
      format={(v) => v.toFixed(2)}
      onChange={(v) => setGamma(channel, v)}
    />
  );

  return (
    <section className="card flex flex-col gap-5 p-5">
      <div className="flex items-start justify-between gap-4">
        <div className="flex flex-col gap-1.5">
          <div className="eyebrow">{t("cal.title")}</div>
          <p className="max-w-[62ch] text-[13px] text-ink-2">{t("cal.intro")}</p>
        </div>
        <SaveBadge state={save} savedText={t("cal.saved")} />
      </div>

      <div className="grid grid-cols-[1fr_188px] items-start gap-7">
        <div className="flex flex-col gap-5">
          <div className="flex flex-col gap-3.5">
            {perChannel
              ? CHANNELS.map((c, i) => gammaSlider(i, `${t("cal.gamma")} · ${t(c.name)}`, c.color))
              : gammaSlider("all", t("cal.gamma"), undefined, t("cal.gammaHint"))}
            <div className="flex items-center gap-2.5">
              <Toggle label={t("cal.perChannel")} checked={perChannel} onChange={togglePerChannel} />
              <span className="text-[13px] text-ink-2">{t("cal.perChannel")}</span>
            </div>
          </div>
          <div className="flex flex-col gap-3.5 border-t border-line pt-4">
            <div className="text-[13px] font-medium">
              {t("cal.balance")}
              <span className="ml-2 font-normal text-ink-3">{t("cal.balanceHint")}</span>
            </div>
            {CHANNELS.map((c, i) => (
              <Slider
                key={c.name}
                label={t(c.name)}
                value={ch(cal.gain, i)}
                min={0}
                max={1}
                step={0.01}
                accent={c.color}
                format={(v) => `${Math.round(v * 100)}%`}
                onChange={(v) => setGain(i, v)}
              />
            ))}
          </div>
        </div>
        <CurvePlot cal={cal} label={t("cal.curve")} rawLabel={t("cal.raw")} />
      </div>

      <div className="flex flex-col gap-3 border-t border-line pt-4">
        <div className="flex items-center justify-between gap-4">
          <span className="text-[13.5px]">{t("cal.pattern")}</span>
          <Segmented<Pattern>
            label={t("cal.pattern")}
            value={pattern}
            onChange={setPattern}
            disabled={!usb}
            options={PATTERNS.map((p) => ({ value: p, label: t(`cal.pattern.${p}` as MessageKey) }))}
          />
        </div>
        <p className="text-[12px] text-ink-3">
          {usb ? t(`cal.pattern.hint.${pattern}` as MessageKey) : t("cal.usbOnly")}
        </p>
        {live && geometry && kb && (
          <div
            className="flex flex-col gap-3 rounded-lg border border-line p-5 pb-3.5"
            style={{ background: "linear-gradient(180deg, var(--plate), color-mix(in oklch, var(--plate) 80%, black))" }}
          >
            <div className="flex gap-1.5" aria-hidden>
              {patternBands(pattern).map((c, i) => (
                <div
                  key={`${c}-${i}`}
                  className="flex h-10 flex-1 items-end rounded-lg px-1.5 pb-1"
                  style={{ background: c, boxShadow: `0 6px 22px -10px ${c}` }}
                >
                  {pattern === "ramp" && (
                    <span className="num text-[11px]" style={{ color: i < RAMP_STEPS / 2 ? "#ffffffaa" : "#000000aa" }}>
                      {c.slice(1, 3).toUpperCase()}
                    </span>
                  )}
                </div>
              ))}
            </div>
            <KeyboardView geometry={geometry} connection={kb.connection} colors={colors} />
            <div className="flex items-center gap-2 text-[12px] text-[#8b8d92]">
              <span className="h-1.5 w-1.5 rounded-full bg-ok" />
              <span className="num">{t("cal.target")}</span>
            </div>
          </div>
        )}
      </div>

      <div className="flex items-end justify-between gap-6 border-t border-line pt-4">
        <p className="max-w-[64ch] text-[12px] leading-relaxed text-ink-3">{t("cal.note")}</p>
        <button
          type="button"
          onClick={reset}
          disabled={sameCalibration(cal, info.factory)}
          className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-line bg-bg-raise px-3 text-[13px] text-ink-2 transition-colors hover:border-line-strong hover:text-ink disabled:cursor-not-allowed disabled:opacity-40"
        >
          <RotateCcw size={13} />
          {t("cal.reset")}
        </button>
      </div>
    </section>
  );
}

function SaveBadge({ state, savedText }: { state: SaveState; savedText: string }) {
  if (state.kind === "idle") return <span className="h-5" aria-hidden />;
  if (state.kind === "error")
    return (
      <span role="alert" className="flex max-w-[30ch] items-center gap-1.5 text-[12px] text-danger">
        <TriangleAlert size={13} className="shrink-0" />
        {state.message}
      </span>
    );
  return (
    <span role="status" className="flex items-center gap-1.5 text-[12px] text-ink-2">
      {state.kind === "saving" ? <Loader2 size={13} className="animate-spin" /> : <Check size={13} className="text-ok" />}
      {state.kind === "saved" && savedText}
    </span>
  );
}

/** Transfer curve LED = gain · screen^gamma per channel, against raw (dashed). */
function CurvePlot({ cal, label, rawLabel }: { cal: Calibration; label: string; rawLabel: string }) {
  const curve = (gamma: number, gain: number) =>
    Array.from({ length: 41 }, (_, i) => {
      const x = i / 40;
      const y = gain * Math.pow(x, gamma);
      return `${i ? "L" : "M"}${(x * 100).toFixed(2)},${(100 - y * 100).toFixed(2)}`;
    }).join(" ");
  const single = uniform(cal.gamma) && uniform(cal.gain);
  return (
    <figure className="flex flex-col gap-2">
      <svg
        viewBox="-3 -3 106 106"
        className="aspect-square w-full rounded-lg border border-line bg-bg-raise"
        role="img"
        aria-label={label}
      >
        {[25, 50, 75].map((v) => (
          <g key={v} stroke="var(--line)" strokeWidth={1} vectorEffect="non-scaling-stroke">
            <line x1={v} y1={0} x2={v} y2={100} vectorEffect="non-scaling-stroke" />
            <line x1={0} y1={v} x2={100} y2={v} vectorEffect="non-scaling-stroke" />
          </g>
        ))}
        <line
          x1={0}
          y1={100}
          x2={100}
          y2={0}
          stroke="var(--ink-3)"
          strokeDasharray="3 3"
          strokeWidth={1}
          vectorEffect="non-scaling-stroke"
        />
        {single ? (
          <path
            d={curve(cal.gamma[0], cal.gain[0])}
            fill="none"
            stroke="var(--signal)"
            strokeWidth={2}
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
        ) : (
          CHANNELS.map((c, i) => (
            <path
              key={c.name}
              d={curve(ch(cal.gamma, i), ch(cal.gain, i))}
              fill="none"
              stroke={c.color}
              strokeWidth={1.75}
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
          ))
        )}
      </svg>
      <figcaption className="flex items-center justify-between text-[11px] text-ink-3">
        <span>{label}</span>
        <span className="flex items-center gap-1.5">
          <span className="w-4 border-t border-dashed border-ink-3" />
          {rawLabel}
        </span>
      </figcaption>
    </figure>
  );
}
