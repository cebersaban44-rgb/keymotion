// "Sistem göstergeleri" page (Araçlar): processor, graphics and memory load
// as bars on key rows, the volume on the F row for a moment when it
// changes, a red key while the microphone is muted. The backend reads the
// PC and lights the keys (app/src-tauri/src/gauges.rs); this page sets it
// up and shows what the keys show. The focus timer goes over the bars; the
// volume and microphone go over the timer (owner's decision, 2026-11-10).

import { Activity, Gauge, Layers, Mic, MicOff, Palette, Usb, Volume2, VolumeX, type LucideIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { GaugeBase } from "../bindings/GaugeBase";
import type { GaugeStyle } from "../bindings/GaugeStyle";
import type { GaugesSettings } from "../bindings/GaugesSettings";
import type { Connection } from "../bindings/Connection";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { ColorPicker, Segmented, Slider, Toggle } from "../components/controls";
import { KeyboardView } from "../components/KeyboardView";
import { OWN_COLORS, heatRgb, rgbCss, useGauges, watchGauges } from "../lib/gauges";
import { percent, useT, type MessageKey } from "../lib/i18n";
import { fromHex, toHex } from "../lib/pomodoroLights";
import { usePomodoro } from "../lib/pomodoro";

export function Gauges({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const usb = kb.connection === "usb" && kb.hasCommandChannel;
  const s = useGauges((x) => x.state);
  const configure = useGauges((x) => x.configure);
  const timer = usePomodoro((x) => x.state);
  const [picking, setPicking] = useState(false);
  // Colour drags fire many changes: show them at once, save the latest
  // shortly after.
  const [draft, setDraft] = useState<Partial<GaugesSettings>>({});
  const configureRef = useRef(configure);
  configureRef.current = configure;
  useEffect(() => {
    if (!Object.keys(draft).length) return;
    const id = window.setTimeout(() => {
      configureRef.current(draft).catch(console.error);
      setDraft({});
    }, 150);
    return () => window.clearTimeout(id);
  }, [draft]);
  useEffect(() => watchGauges(kb.geometry ?? null), [kb.geometry]);
  const geometry = kb.geometry;
  const colors = useMemo(() => {
    if (!s || !geometry) return null;
    const out: Record<number, string> = {};
    for (const c of s.frame) out[c.led] = `rgb(${c.r},${c.g},${c.b})`;
    return out;
  }, [s, geometry]);
  if (!s) return null;

  const shown = { ...s.settings, ...draft };
  const set = (patch: Partial<GaugesSettings>) => configure(patch).catch(console.error);
  const r = s.readings;
  const timerFront = !!timer && (timer.trying || (timer.status !== "idle" && timer.settings.lights));
  const status: MessageKey = !shown.enabled
    ? "gauges.status.off"
    : !usb
      ? "gauges.status.noUsb"
      : timerFront
        ? "gauges.status.timer"
        : s.frame.length === 0
          ? "gauges.status.empty"
          : "gauges.status.on";
  const pct = (v: number | null) => (v == null ? t("gauges.r.none") : percent(Math.round(v)));
  const micKey = geometry?.keys.find((k) => k.id === shown.micKey);
  const micLed = micKey?.led;
  const bars = shown.cpu || shown.gpu || shown.ram;

  return (
    <div className="flex flex-col gap-6" data-gauges>
      <header className="flex flex-col gap-2">
        <h1 className="page-title">{t("nav.gauges")}</h1>
        <p className="page-desc">{t("gauges.body")}</p>
      </header>
      {!usb && (
        <div className="flex items-center gap-3 rounded-lg border border-signal/30 bg-signal/10 px-4 py-3 text-[13px]">
          <Usb size={16} className="text-signal" />
          {t("gauges.noUsb")}
        </div>
      )}

      <section className="card flex flex-col">
        {/* On or off, what the keys show, and the readings. */}
        <div className="flex flex-col gap-5 p-5">
          <div className="flex items-start justify-between gap-4">
            <span className="flex flex-col gap-1">
              <span className="text-[14px] font-semibold">{t("gauges.on")}</span>
              <span className="max-w-[80ch] text-[12px] text-ink-3">{t("gauges.on.body")}</span>
            </span>
            <Toggle checked={shown.enabled} onChange={(enabled) => set({ enabled })} label={t("gauges.on")} />
          </div>
          <div className="flex items-center gap-2 text-[12px]" data-gauges-status={status}>
            <span
              className={`h-2 w-2 shrink-0 rounded-full ${shown.enabled && usb ? "bg-ok" : "bg-ink-3"}`}
              aria-hidden
            />
            <span className={shown.enabled ? "text-ink-2" : "text-ink-3"}>{t(status)}</span>
          </div>
          {geometry && colors && (
            <div className="flex flex-col gap-2">
              <span className="text-[12px] text-ink-3">{picking ? t("gauges.mic.pick") : t("gauges.preview")}</span>
              <KeysPreview
                geometry={geometry}
                connection={kb.connection}
                colors={colors}
                ring={picking ? micLed : undefined}
                onPick={
                  picking
                    ? (id) => {
                        set({ micKey: id });
                        setPicking(false);
                      }
                    : undefined
                }
              />
            </div>
          )}
          <div className="grid grid-cols-[repeat(auto-fit,minmax(120px,1fr))] gap-x-6 gap-y-4" data-gauges-readings>
            <Reading icon={Activity} label={t("gauges.r.cpu")} value={pct(r.cpu)} share={r.cpu} />
            <Reading icon={Gauge} label={t("gauges.r.gpu")} value={pct(r.gpu)} share={r.gpu} />
            <Reading icon={Layers} label={t("gauges.r.ram")} value={pct(r.ram)} share={r.ram} />
            <Reading
              icon={r.muted ? VolumeX : Volume2}
              label={t("gauges.r.volume")}
              value={r.muted ? t("gauges.r.muted") : pct(r.volume)}
              share={r.muted ? 0 : r.volume}
              lit={s.volumeShowing}
            />
            <Reading
              icon={r.micMuted ? MicOff : Mic}
              label={t("gauges.r.mic")}
              value={r.micMuted == null ? t("gauges.r.none") : r.micMuted ? t("gauges.r.micMuted") : t("gauges.r.micOpen")}
              alert={!!r.micMuted}
            />
          </div>
        </div>

        {/* Settings. */}
        <div className="flex flex-col divide-y divide-line border-t border-line" data-gauges-settings>
          <Row icon={Activity} title={t("gauges.bars")} body={t("gauges.bars.body")}>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(210px,1fr))] gap-2">
              <GaugeSwitch label={t("gauges.r.cpu")} where={t("gauges.cpu.where")} on={shown.cpu} onChange={(cpu) => set({ cpu })} />
              <GaugeSwitch label={t("gauges.r.gpu")} where={t("gauges.gpu.where")} on={shown.gpu} onChange={(gpu) => set({ gpu })} />
              <GaugeSwitch label={t("gauges.r.ram")} where={t("gauges.ram.where")} on={shown.ram} onChange={(ram) => set({ ram })} />
            </div>
            <p className="text-[12px] text-ink-3">{t("gauges.gpu.note")}</p>
          </Row>
          <Row
            icon={Volume2}
            title={t("gauges.volume")}
            body={t("gauges.volume.body")}
            checked={shown.volume}
            onChange={(volume) => set({ volume })}
          />
          <Row icon={MicOff} title={t("gauges.mic")} body={t("gauges.mic.body")} checked={shown.mic} onChange={(mic) => set({ mic })}>
            {shown.mic && (
              <div className="flex flex-wrap items-center gap-3 text-[13px]">
                <span className="text-ink-2">{t("gauges.mic.key")}</span>
                <span className="num rounded-md border border-line bg-bg-raise px-2 py-0.5 font-medium" data-gauges-mic-key>
                  {micKey?.label || shown.micKey}
                </span>
                <button
                  type="button"
                  onClick={() => setPicking((p) => !p)}
                  className="rounded-lg border border-line px-3 py-1.5 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
                  data-gauges-pick
                >
                  {picking ? t("gauges.cancel") : t("gauges.mic.change")}
                </button>
              </div>
            )}
          </Row>
          <Row icon={Palette} title={t("gauges.look")} body={t("gauges.look.body")}>
            <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-6">
              <div className="flex min-w-0 flex-col gap-4">
                <div className="flex flex-col gap-2">
                  <span className="text-[13px] font-medium">{t("gauges.style")}</span>
                  <Segmented<GaugeStyle>
                    label={t("gauges.style")}
                    value={shown.style}
                    onChange={(style) => set({ style })}
                    options={[
                      { value: "heat", label: t("gauges.style.heat") },
                      { value: "own", label: t("gauges.style.own") },
                    ]}
                  />
                  <StyleLegend style={shown.style} note={t(shown.style === "heat" ? "gauges.style.heatNote" : "gauges.style.ownNote")} />
                </div>
                <div className="flex items-start justify-between gap-3">
                  <span className="flex flex-col">
                    <span className="text-[13px] font-medium">{t("gauges.track")}</span>
                    <span className="text-[12px] text-ink-3">{t("gauges.track.body")}</span>
                  </span>
                  <Toggle checked={shown.track} onChange={(track) => set({ track })} label={t("gauges.track")} />
                </div>
                <Slider
                  label={t("gauges.brightness")}
                  value={shown.brightness}
                  min={1}
                  max={5}
                  step={1}
                  format={(v) => `${v}/5`}
                  onChange={(brightness) => set({ brightness })}
                />
              </div>
              <div className="flex min-w-0 flex-col gap-2">
                <span className="text-[13px] font-medium">{t("gauges.base")}</span>
                <Segmented<GaugeBase>
                  label={t("gauges.base")}
                  value={shown.base}
                  onChange={(base) => set({ base })}
                  options={[
                    { value: "dark", label: t("gauges.base.dark") },
                    { value: "tint", label: t("gauges.base.tint") },
                  ]}
                />
                {shown.base === "tint" && (
                  <ColorPicker
                    label={t("gauges.base.tint")}
                    value={toHex(shown.tint)}
                    onChange={(hex) => setDraft((d) => ({ ...d, tint: fromHex(hex) }))}
                  />
                )}
              </div>
            </div>
          </Row>
        </div>

        <LayerList enabled={shown.enabled} bars={bars} timerFront={timerFront} alerts={s.volumeShowing || (shown.mic && !!r.micMuted)} lit={s.frame.length > 0} />
      </section>
    </div>
  );
}

/** The keyboard as the gauges light it (the app's keyboard drawing, small);
 *  while picking the microphone key, keys can be clicked. */
function KeysPreview({
  geometry,
  connection,
  colors,
  ring,
  onPick,
}: {
  geometry: GeometryInfo;
  connection: Connection;
  colors: Record<number, string>;
  ring?: number;
  onPick?: (id: string) => void;
}) {
  const t = useT();
  const selected = useMemo(() => new Set(ring === undefined ? [] : [ring]), [ring]);
  const byLed = useMemo(() => new Map(geometry.keys.map((k) => [k.led, k.id])), [geometry]);
  return (
    <div className="w-full max-w-[760px]" data-gauges-preview>
      <KeyboardView
        geometry={geometry}
        connection={connection}
        colors={colors}
        compact
        label={t("gauges.preview")}
        selected={selected}
        tool="select"
        onKeyAction={
          onPick
            ? (led, action) => {
                const id = byLed.get(led);
                if (action === "press" && id) onPick(id);
              }
            : undefined
        }
      />
    </div>
  );
}

/** One reading: its value and a thin bar. */
function Reading({
  icon: Icon,
  label,
  value,
  share,
  lit,
  alert,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  share?: number | null;
  lit?: boolean;
  alert?: boolean;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className={`flex items-center gap-1.5 text-[12px] ${lit ? "text-signal" : "text-ink-3"}`}>
        <Icon size={13} className={alert ? "text-danger" : ""} />
        {label}
      </span>
      <span className={`num text-[18px] leading-none font-semibold ${alert ? "text-danger" : ""}`}>{value}</span>
      {share !== undefined && (
        <span className="h-1 overflow-hidden rounded-full bg-line" aria-hidden>
          <span className="block h-full rounded-full bg-signal transition-[width] duration-300" style={{ width: `${Math.max(0, Math.min(100, share ?? 0))}%` }} />
        </span>
      )}
    </div>
  );
}

function GaugeSwitch({ label, where, on, onChange }: { label: string; where: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex items-center gap-3 py-1">
      <Toggle checked={on} onChange={onChange} label={label} />
      <span className="flex flex-col">
        <span className="text-[13px] font-medium">{label}</span>
        <span className="text-[12px] text-ink-3">{where}</span>
      </span>
    </div>
  );
}

/** Ten little keys in the chosen bar colours. */
function StyleLegend({ style, note }: { style: GaugeStyle; note: string }) {
  const rows =
    style === "heat"
      ? [Array.from({ length: 10 }, (_, i) => (i / 9 < 0.6 ? heatRgb.green : i / 9 < 0.8 ? heatRgb.amber : heatRgb.red))]
      : (["cpu", "gpu", "ram"] as const).map((g) => Array.from({ length: 10 }, () => OWN_COLORS[g]));
  return (
    <div className="flex flex-col gap-1.5">
      {rows.map((row, j) => (
        <span key={j} className="flex gap-1" aria-hidden>
          {row.map((c, i) => (
            <span key={i} className="h-3 w-3 rounded-[3px]" style={{ background: rgbCss(c) }} />
          ))}
        </span>
      ))}
      <span className="text-[12px] text-ink-3">{note}</span>
    </div>
  );
}

/** The light layers from the top down, and which are on now. */
function LayerList({
  enabled,
  bars,
  timerFront,
  alerts,
  lit,
}: {
  enabled: boolean;
  bars: boolean;
  timerFront: boolean;
  alerts: boolean;
  lit: boolean;
}) {
  const t = useT();
  const layers: { title: MessageKey; note: MessageKey; on: boolean }[] = [
    { title: "gauges.layer.preview", note: "gauges.layer.preview.note", on: false },
    { title: "gauges.layer.alerts", note: "gauges.layer.alerts.note", on: enabled && alerts },
    { title: "gauges.layer.timer", note: "gauges.layer.timer.note", on: timerFront },
    { title: "gauges.layer.bars", note: "gauges.layer.bars.note", on: enabled && bars && !timerFront },
    { title: "gauges.layer.saved", note: "gauges.layer.saved.note", on: !timerFront && !(enabled && lit) },
  ];
  return (
    <div className="flex flex-col gap-3 border-t border-line px-5 py-4" data-gauges-layers>
      <span className="flex flex-col">
        <span className="text-[13px] font-semibold">{t("gauges.layers")}</span>
        <span className="text-[12px] text-ink-3">{t("gauges.layers.body")}</span>
      </span>
      <ol className="flex flex-col gap-1">
        {layers.map((l, i) => (
          <li
            key={l.title}
            className={`flex items-center gap-3 rounded-lg px-3 py-2 text-[13px] ${l.on ? "bg-signal/10 text-ink" : "text-ink-3"}`}
            data-layer-on={l.on || undefined}
          >
            <span className="num w-4 text-right text-[11px]">{i + 1}</span>
            <span className="flex flex-1 flex-col">
              <span>{t(l.title)}</span>
              <span className="text-[12px] text-ink-3">{t(l.note)}</span>
            </span>
            {l.on && <span className="text-[11px] font-semibold text-signal">{t("gauges.layer.now")}</span>}
          </li>
        ))}
      </ol>
      <Note>{t("gauges.end")}</Note>
    </div>
  );
}

function Note({ children }: { children: ReactNode }) {
  return <p className="text-[12px] text-ink-3">{children}</p>;
}

/** One group of settings: a title and what it does, its switch, and what
 *  belongs to it, indented under it. */
function Row({
  icon: Icon,
  title,
  body,
  checked,
  onChange,
  children,
}: {
  icon: LucideIcon;
  title: string;
  body: string;
  checked?: boolean;
  onChange?: (v: boolean) => void;
  children?: ReactNode;
}) {
  const nested = Array.isArray(children) ? children.some(Boolean) : !!children;
  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <div className="flex items-start justify-between gap-3">
        <span className="flex items-start gap-2.5">
          <Icon size={16} className="mt-0.5 shrink-0 text-signal" />
          <span className="flex max-w-[80ch] flex-col">
            <span className="text-[13px] font-semibold">{title}</span>
            <span className="text-[12px] text-ink-3">{body}</span>
          </span>
        </span>
        {onChange && checked !== undefined && <Toggle checked={checked} onChange={onChange} label={title} />}
      </div>
      {nested && <div className="ml-[7px] flex flex-col gap-3 border-l-2 border-line pl-5">{children}</div>}
    </div>
  );
}

