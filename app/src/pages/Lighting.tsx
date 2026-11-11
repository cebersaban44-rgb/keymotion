import {
  AlertTriangle,
  BoxSelect,
  Check,
  Eraser,
  Loader2,
  MousePointer2,
  PaintBucket,
  Paintbrush,
  RotateCcw,
  Usb,
  Gauge,
  X,
} from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { EffectInfo } from "../bindings/EffectInfo";
import type { EffectMotion } from "../bindings/EffectMotion";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { ColorPicker, EffectSwatch, Field, Segmented, Steps, Toggle, type SwatchVariant } from "../components/controls";
import { KeyboardView, type KeyAction } from "../components/KeyboardView";
import { getLightingCatalog, setLiveFrame } from "../lib/ipc";
import { useT, type MessageKey } from "../lib/i18n";
import {
  REACTIVE_EFFECTS,
  sameColors,
  toLedColors,
  useLighting,
  type ColorMap,
  type PaintTool,
} from "../lib/lighting";
import { useGauges } from "../lib/gauges";
import { useMirror } from "../lib/mirror";
import { useApp } from "../lib/store";
import { friendlyError } from "../lib/errors";

const VARIANT: Record<number, SwatchVariant> = {
  0: "off",
  1: "static",
  2: "ripple",
  3: "ripple",
  4: "sparkle",
  5: "sparkle",
  6: "spectrum",
  7: "breath",
  8: "spectrum",
  9: "wave",
  10: "wave",
  11: "wave",
  12: "wave",
  13: "ripple",
  14: "ripple",
  15: "ripple",
  16: "wave",
  17: "breath",
  18: "wave",
  19: "wave",
};

type Status = { kind: "idle" } | { kind: "busy" } | { kind: "ok"; ms: number } | { kind: "error"; message: string };

/**
 * Labels for direction codes 0 and 1, by how the effect moves. Measured on
 * the AK980 PRO (fw 0x0106) with `kb direction-probe`; codes 2 and 3 repeat
 * 1 and 0, so only two are offered.
 */
const DIRECTION_LABELS: Record<EffectMotion, [MessageKey, MessageKey]> = {
  vertical: ["light.dir.down", "light.dir.up"],
  horizontal: ["light.dir.right", "light.dir.left"],
  rotation: ["light.dir.ccw", "light.dir.cw"],
  path: ["light.dir.forward", "light.dir.reverse"],
  diagonal: ["light.dir.upRight", "light.dir.downLeft"],
};

/** Direction code → 0 or 1 (2 behaves like 1, 3 like 0). */
const directionGroup = (d: number) => (d === 1 || d === 2 ? 1 : 0);

export function Lighting({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const { tab, setTab } = useLighting();
  const usb = kb.connection === "usb" && kb.hasCommandChannel;
  const mirror = useMirror(usb && tab === "effects");
  const perKey = useLighting((s) => s.perKey);
  // The gauges draw the keys while on: the saved lighting does not show.
  const gaugesOn = useGauges((s) => !!s.state?.settings.enabled);
  const setPage = useApp((s) => s.setPage);

  return (
    <div className="flex flex-col gap-6">
      <header className="flex items-end justify-between gap-6">
        <div>
          <h1 className="page-title">{t("nav.lighting")}</h1>
        </div>
        <div role="tablist" className="inline-flex rounded-lg border border-line bg-bg-raise p-1">
          {(["effects", "per-key"] as const).map((id) => (
            <button
              key={id}
              role="tab"
              type="button"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
              className={`rounded-lg px-4 py-1.5 text-[13px] transition-colors ${
                tab === id ? "bg-surface-2 text-ink shadow-[inset_0_0_0_1px_var(--line)]" : "text-ink-2 hover:text-ink"
              }`}
            >
              {t(id === "effects" ? "light.tab.effects" : "light.tab.perKey")}
            </button>
          ))}
        </div>
      </header>

      {!usb && (
        <div className="flex items-center gap-3 rounded-lg border border-signal/30 bg-signal/10 px-4 py-3 text-[13px]">
          <Usb size={16} className="text-signal" />
          {t("light.needsUsb")}
        </div>
      )}
      {usb && gaugesOn && (
        <div className="flex items-center gap-3 rounded-lg border border-line bg-bg-raise/60 px-4 py-3 text-[13px]" data-light-gauges>
          <Gauge size={16} className="shrink-0 text-signal" />
          <span className="flex-1 text-ink-2">{t("light.gaugesOn")}</span>
          <button
            type="button"
            onClick={() => setPage("gauges")}
            className="shrink-0 rounded-lg border border-line px-3 py-1.5 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
          >
            {t("light.gaugesGo")}
          </button>
        </div>
      )}

      {tab === "effects" ? (
        <EffectsTab kb={kb} usb={usb} mirror={mirror} />
      ) : (
        <PerKeyTab kb={kb} usb={usb} perKey={perKey} />
      )}
    </div>
  );
}

function Plate({ kb, colors, caption, live, onKeyAction, selected, tool, onBackgroundPress }: {
  kb: KeyboardInfo;
  colors: ColorMap | null;
  caption: string;
  live: boolean;
  onKeyAction?: (led: number, action: KeyAction, additive: boolean) => void;
  selected?: ReadonlySet<number>;
  tool?: PaintTool;
  /** Pointer down on the plate outside any key (clears a selection). */
  onBackgroundPress?: () => void;
}) {
  return (
    <section
      onPointerDown={(e) => {
        if (!(e.target as HTMLElement).closest(".kb-key")) onBackgroundPress?.();
      }}
      className="relative overflow-hidden rounded-lg border border-line p-6 pb-4"
      style={{ background: "linear-gradient(180deg, var(--plate), color-mix(in oklch, var(--plate) 80%, black))" }}
    >
      {kb.geometry && (
        <KeyboardView
          geometry={kb.geometry}
          battery={kb.battery?.percent ?? null}
          connection={kb.connection}
          colors={colors}
          onKeyAction={onKeyAction}
          selected={selected}
          tool={tool}
        />
      )}
      <div className="mt-4 flex items-center gap-2 text-[12px] text-[#8b8d92]">
        <span className={`h-1.5 w-1.5 rounded-full ${live ? "bg-ok" : "bg-white/25"}`} />
        <span className="num">{caption}</span>
      </div>
    </section>
  );
}

function StatusLine({ status, okText }: { status: Status; okText: string }) {
  const t = useT();
  if (status.kind === "idle") return <span className="text-[12px] text-ink-3">{t("light.persistNote")}</span>;
  if (status.kind === "busy")
    return (
      <span className="flex items-center gap-2 text-[12px] text-ink-2">
        <Loader2 size={13} className="animate-spin" />
        {t("light.applying")}
      </span>
    );
  if (status.kind === "ok")
    return (
      <span className="flex items-center gap-2 text-[12px] text-ok">
        <Check size={13} />
        {okText} <span className="num text-ink-3">· {status.ms} ms</span>
      </span>
    );
  return (
    <span className="flex items-center gap-2 text-[12px] text-danger">
      <AlertTriangle size={13} />
      {status.message}
    </span>
  );
}

function EffectsTab({ kb, usb, mirror }: { kb: KeyboardInfo; usb: boolean; mirror: ColorMap | null }) {
  const t = useT();
  const { effect, setEffect, applyEffectDraft, appliedEffect } = useLighting();
  const [catalog, setCatalog] = useState<EffectInfo[]>([]);
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  useEffect(() => {
    getLightingCatalog().then(setCatalog).catch(console.error);
  }, []);

  const current = catalog.find((e) => e.id === effect.mode);
  const effects = catalog.filter((e) => e.id !== 0);
  const off = catalog.find((e) => e.id === 0);
  const dirty = JSON.stringify(effect) !== JSON.stringify(appliedEffect);

  const apply = async () => {
    setStatus({ kind: "busy" });
    const started = performance.now();
    try {
      await applyEffectDraft();
      setStatus({ kind: "ok", ms: Math.round(performance.now() - started) });
    } catch (e) {
      setStatus({ kind: "error", message: friendlyError(e, t) });
    }
  };

  const card = (fx: EffectInfo) => {
    const selected = fx.id === effect.mode;
    const isApplied = appliedEffect?.mode === fx.id;
    return (
      <button
        key={fx.id}
        type="button"
        onClick={() => setEffect({ mode: fx.id })}
        aria-pressed={selected}
        className={`group relative flex flex-col gap-3 rounded-lg border p-3 text-left transition-all ${
          selected
            ? "border-signal bg-surface-2 shadow-[0_0_0_3px_color-mix(in_oklch,var(--signal)_20%,transparent)]"
            : "border-line bg-surface hover:border-line-strong"
        }`}
      >
        <EffectSwatch
          variant={VARIANT[fx.id] ?? "wave"}
          color={effect.color}
          rainbow={fx.rainbow ? effect.rainbow || !fx.color : !fx.color}
        />
        <div className="flex items-center gap-1.5">
          <span className="truncate text-[13px] font-medium">{t(`fx.${fx.id}` as MessageKey)}</span>
          {isApplied && <span className="ml-auto h-1.5 w-1.5 shrink-0 rounded-full bg-ok" aria-hidden />}
        </div>
        <div className="num -mt-2 flex gap-1.5 text-[11px] text-ink-3">
          {REACTIVE_EFFECTS.has(fx.id) && <span>{t("light.reactive")}</span>}
          {fx.factoryDefault && <span>{t("light.factory")}</span>}
        </div>
      </button>
    );
  };

  return (
    <>
      <Plate
        kb={kb}
        colors={mirror}
        live={!!mirror}
        caption={mirror ? t("light.mirror.on") : t("light.mirror.off")}
      />

      <div className="grid grid-cols-[1fr_360px] items-start gap-5">
        <motion.section layout className="card p-5">
          <div className="eyebrow mb-4">{t("light.effects")}</div>
          <div className="grid grid-cols-4 gap-2.5">{effects.map(card)}</div>
          {off && <div className="mt-2.5 grid grid-cols-4 gap-2.5 border-t border-line pt-2.5">{card(off)}</div>}
        </motion.section>

        <section className="card sticky top-4 flex flex-col gap-5 p-5">
          <div className="flex items-baseline justify-between">
            <div className="eyebrow">{t("light.settings")}</div>
            <span className="font-display text-[15px] font-semibold">{current && t(`fx.${current.id}` as MessageKey)}</span>
          </div>

          {current && current.id === 0 && (
            <div className="flex gap-2.5 rounded-lg border border-warn/30 bg-warn/10 p-3 text-[12px] text-ink-2">
              <AlertTriangle size={15} className="mt-0.5 shrink-0 text-warn" />
              {t("light.offWarning")}
            </div>
          )}

          {current?.color && (
            <Field label={t("light.color")}>
              <ColorPicker
                label={t("light.color")}
                value={effect.color}
                onChange={(color) => setEffect({ color, rainbow: false })}
                disabled={current.rainbow && effect.rainbow}
              />
            </Field>
          )}
          {current?.rainbow && current.color && (
            <div className="flex items-center justify-between">
              <span className="text-[13px] font-medium">{t("light.rainbow")}</span>
              <Toggle label={t("light.rainbow")} checked={effect.rainbow} onChange={(rainbow) => setEffect({ rainbow })} />
            </div>
          )}
          {current?.brightness && (
            <Field label={t("light.brightness")} hint={`${effect.brightness} / 5`}>
              <Steps label={t("light.brightness")} value={effect.brightness} onChange={(brightness) => setEffect({ brightness })} />
            </Field>
          )}
          {current?.speed && (
            <Field label={t("light.speed")} hint={`${effect.speed} / 5`}>
              <Steps label={t("light.speed")} value={effect.speed} onChange={(speed) => setEffect({ speed })} />
            </Field>
          )}
          {current && current.directions > 0 && (
            <Field label={t("light.direction")}>
              <div role="radiogroup" className="grid grid-flow-col gap-1.5">
                {DIRECTION_LABELS[current.motion ?? "path"].map((label, v) => {
                  const on = directionGroup(effect.direction) === v;
                  return (
                    <button
                      key={v}
                      type="button"
                      role="radio"
                      aria-checked={on}
                      onClick={() => setEffect({ direction: v })}
                      className={`h-8 rounded-lg border text-[13px] transition-colors ${
                        on ? "border-signal bg-signal/15 text-ink" : "border-line bg-bg-raise text-ink-2 hover:text-ink"
                      }`}
                    >
                      {t(label)}
                    </button>
                  );
                })}
              </div>
            </Field>
          )}
          {current && current.id !== 0 && !current.brightness && !current.speed && !current.color && (
            <p className="text-[13px] text-ink-3">{t("light.noSettings")}</p>
          )}

          <div className="flex flex-col gap-2.5 border-t border-line pt-4">
            <button
              type="button"
              disabled={!usb || status.kind === "busy" || !current}
              onClick={apply}
              className="flex h-10 items-center justify-center gap-2 rounded-lg bg-signal text-[13.5px] font-semibold text-signal-ink transition-transform active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-40"
            >
              {status.kind === "busy" ? <Loader2 size={15} className="animate-spin" /> : null}
              {status.kind === "busy" ? t("light.applying") : t("light.apply")}
              {dirty && status.kind !== "busy" && <span className="h-1.5 w-1.5 rounded-full bg-signal-ink/70" />}
            </button>
            <StatusLine status={status} okText={t("light.applied")} />
            <button
              type="button"
              onClick={() => setEffect({ mode: 11, rainbow: true, brightness: 5, speed: 3, direction: 0 })}
              className="flex items-center gap-1.5 self-start text-[12px] text-ink-3 hover:text-ink"
            >
              <RotateCcw size={12} />
              {t("light.factoryReset")}
            </button>
          </div>
        </section>
      </div>
    </>
  );
}

function PerKeyTab({ kb, usb, perKey }: { kb: KeyboardInfo; usb: boolean; perKey: ColorMap }) {
  const t = useT();
  const {
    brush,
    setBrush,
    paint,
    setPerKey,
    perKeyBrightness,
    setPerKeyBrightness,
    livePreview,
    setLivePreview,
    savedPerKey,
    savePerKeyDraft,
    revertPerKey,
    tool,
    setTool,
  } = useLighting();
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const keys = kb.geometry?.keys ?? [];
  const live = usb && livePreview;

  // Select tool: keys stay selected and every colour pick recolours them.
  // The ref mirrors the state so rapid drags and colour picks never see a
  // stale selection.
  const [selection, setSelection] = useState<ReadonlySet<number>>(() => new Set());
  const selectionRef = useRef<ReadonlySet<number>>(selection);
  const select = (next: ReadonlySet<number>) => {
    selectionRef.current = next;
    setSelection(next);
    // Show the selection's colour in the picker when the keys share one.
    const design = useLighting.getState().perKey;
    const shared = new Set([...next].map((led) => design[led]?.toLowerCase()));
    const [only] = shared;
    if (shared.size === 1 && only) setBrush(only);
  };
  const switchTool = (next: PaintTool) => {
    setTool(next);
    select(new Set());
  };
  const onKeyAction = (led: number, action: KeyAction, additive: boolean) => {
    if (tool === "brush") {
      paint([led], currentBrush());
      return;
    }
    const prev = selectionRef.current;
    if (action === "drag") {
      if (!prev.has(led)) select(new Set([...prev, led]));
    } else if (additive) {
      const next = new Set(prev);
      if (next.has(led)) next.delete(led);
      else next.add(led);
      select(next);
    } else {
      select(new Set([led]));
    }
  };
  const pickColor = (color: string) => {
    setBrush(color);
    if (tool === "select" && selectionRef.current.size) paint([...selectionRef.current], color);
  };
  const eraseSelection = () => paint([...selectionRef.current], null);
  const selectAll = () => select(new Set(keys.map((k) => k.led)));

  // Select-tool shortcuts; ignored while typing in a field.
  useEffect(() => {
    if (tool !== "select") return;
    const onKey = (e: KeyboardEvent) => {
      const target = e.target instanceof Element ? e.target : null;
      if (target?.closest("input, textarea, [contenteditable]")) return;
      if (e.key === "Escape") {
        select(new Set());
      } else if ((e.key === "Delete" || e.key === "Backspace") && selectionRef.current.size) {
        e.preventDefault();
        eraseSelection();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        selectAll();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // `select`, `eraseSelection` and `selectAll` only use refs and store setters.
  }, [tool, keys]);

  // Stream the design to the keyboard's RAM while previewing; stop on leave.
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (!live) {
      window.clearTimeout(timer.current);
      setLiveFrame(null).catch(console.error);
      return;
    }
    window.clearTimeout(timer.current);
    // Brightness is applied by the backend after calibration, with the
    // firmware's own scale, so the preview matches the stored design.
    timer.current = window.setTimeout(() => {
      setLiveFrame(toLedColors(perKey), perKeyBrightness).catch(console.error);
    }, 40);
  }, [live, perKey, perKeyBrightness]);
  // A frame still waiting on the timer would restart the stream after the
  // stop and keep it running on another tab (seen when switching tabs
  // within 40 ms of opening this one).
  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
      void setLiveFrame(null).catch(console.error);
    },
    [],
  );

  const byId = useMemo(() => new Map(keys.map((k) => [k.id, k.led])), [keys]);
  const preset = (name: "wasd" | "rainbow" | "home") => {
    const map: ColorMap = {};
    if (name === "rainbow" && kb.geometry) {
      for (const k of keys) {
        const hue = ((k.x + k.w / 2) / kb.geometry.width) * 330;
        map[k.led] = hslHex(hue, 95, 55);
      }
    } else {
      const ids = name === "wasd" ? ["w", "a", "s", "d", "up", "left", "down", "right"] : ["a", "s", "d", "f", "j", "k", "l", "semicolon"];
      for (const id of ids) {
        const led = byId.get(id);
        if (led !== undefined) map[led] = currentBrush();
      }
    }
    setPerKey(map);
  };

  const save = async () => {
    setStatus({ kind: "busy" });
    const started = performance.now();
    try {
      await savePerKeyDraft();
      setStatus({ kind: "ok", ms: Math.round(performance.now() - started) });
    } catch (e) {
      setStatus({ kind: "error", message: friendlyError(e, t) });
    }
  };

  const painted = Object.keys(perKey).length;
  // Does the design on screen differ from the one stored on the keyboard?
  const matchesSaved =
    !!savedPerKey && savedPerKey.brightness === perKeyBrightness && sameColors(savedPerKey.colors, perKey);
  const saveState: MessageKey = !savedPerKey ? "light.state.none" : matchesSaved ? "light.state.clean" : "light.state.dirty";

  return (
    <>
      <Plate
        kb={kb}
        colors={perKey}
        live={live}
        caption={live ? t(matchesSaved ? "light.live.saved" : "light.live.on") : t("light.design")}
        onKeyAction={onKeyAction}
        selected={tool === "select" ? selection : undefined}
        tool={tool}
        onBackgroundPress={() => tool === "select" && selection.size > 0 && select(new Set())}
      />
      <p className="-mt-3 text-[12px] text-ink-3">
        {tool === "select" && selection.size > 0 ? (
          <span className="text-ink-2">
            <span className="num">{`${selection.size} ${t("light.keysSelected")}`}</span>
            <span className="mx-1.5 text-ink-3">·</span>
            {t("light.selectShortcuts")}
          </span>
        ) : (
          t(tool === "brush" ? "light.paintHint" : "light.selectEmptyHint")
        )}
        <span className="mx-1.5">·</span>
        <span className="num">
          {painted} {t("light.keysPainted")}
        </span>
      </p>

      <div className="grid grid-cols-[1fr_360px] items-start gap-5">
        <section className="card flex flex-col gap-5 p-5">
          <div className="flex flex-col gap-2.5 rounded-lg border border-line bg-bg-raise/60 p-3.5">
            <div className="flex items-center justify-between gap-4">
              <span className="text-[13px] font-medium">{t("light.tool")}</span>
              <Segmented<PaintTool>
                label={t("light.tool")}
                value={tool}
                onChange={switchTool}
                options={[
                  { value: "brush", label: t("light.tool.brush"), icon: <Paintbrush size={13} /> },
                  { value: "select", label: t("light.tool.select"), icon: <MousePointer2 size={13} /> },
                ]}
              />
            </div>
            <p className="text-[12px] leading-relaxed text-ink-2">
              {t(tool === "brush" ? "light.tool.brushHint" : "light.tool.selectHint")}
            </p>
          </div>
          <Field
            label={
              tool === "brush"
                ? t("light.brush")
                : selection.size
                  ? `${t("light.selectedColor")} · ${selection.size}`
                  : t("light.colorPickFirst")
            }
          >
            <ColorPicker label={t("light.brush")} value={brush} onChange={pickColor} />
          </Field>
          {tool === "select" && (
            <Field label={t("light.selection")}>
              <div className="flex flex-wrap gap-2">
                <ToolButton icon={<BoxSelect size={14} />} label={t("light.selectAll")} onClick={selectAll} />
                {selection.size > 0 && (
                  <>
                    <ToolButton icon={<Eraser size={14} />} label={t("light.eraseSelected")} onClick={eraseSelection} />
                    <ToolButton icon={<X size={14} />} label={t("light.deselect")} onClick={() => select(new Set())} />
                  </>
                )}
              </div>
            </Field>
          )}
          <Field label={t("light.tools")}>
            <div className="flex flex-wrap gap-2">
              <ToolButton
                icon={<PaintBucket size={14} />}
                label={t("light.fillAll")}
                onClick={() => paint(keys.map((k) => k.led), currentBrush())}
              />
              <ToolButton icon={<Eraser size={14} />} label={t("light.clear")} onClick={() => setPerKey({})} />
            </div>
          </Field>
          <Field label={t("light.presets")}>
            <div className="flex flex-wrap gap-2">
              <ToolButton label={t("light.preset.wasd")} onClick={() => preset("wasd")} />
              <ToolButton label={t("light.preset.home")} onClick={() => preset("home")} />
              <ToolButton label={t("light.preset.rainbow")} onClick={() => preset("rainbow")} />
            </div>
          </Field>
        </section>

        <section className="card sticky top-4 flex flex-col gap-5 p-5">
          <div className="flex items-center justify-between gap-4">
            <div>
              <div className="text-[13px] font-medium">{t("light.livePreview")}</div>
              <div className="text-[12px] text-ink-3">{t("light.livePreviewHint")}</div>
            </div>
            <Toggle label={t("light.livePreview")} checked={livePreview} onChange={setLivePreview} disabled={!usb} />
          </div>
          <Field label={t("light.brightness")} hint={`${perKeyBrightness} / 5`}>
            <Steps label={t("light.brightness")} value={perKeyBrightness} onChange={setPerKeyBrightness} />
          </Field>
          <div className="flex flex-col gap-2.5 border-t border-line pt-4">
            <div role="status" className="flex items-center gap-2 text-[13px]">
              <span
                className={`h-2 w-2 shrink-0 rounded-full ${
                  matchesSaved ? "bg-ok" : "bg-warn"
                }`}
              />
              <span className={matchesSaved ? "text-ink-2" : "text-ink"}>{t(saveState)}</span>
            </div>
            <button
              type="button"
              disabled={!usb || status.kind === "busy"}
              onClick={save}
              className="flex h-10 items-center justify-center gap-2 rounded-lg bg-signal text-[13.5px] font-semibold text-signal-ink transition-transform active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-40"
            >
              {status.kind === "busy" && <Loader2 size={15} className="animate-spin" />}
              {status.kind === "busy" ? t("light.saving") : t("light.save")}
            </button>
            {savedPerKey && !matchesSaved && (
              <button
                type="button"
                onClick={revertPerKey}
                disabled={status.kind === "busy"}
                className="flex h-9 items-center justify-center gap-1.5 rounded-lg border border-line bg-bg-raise text-[13px] text-ink-2 transition-colors hover:border-line-strong hover:text-ink disabled:opacity-40"
              >
                <RotateCcw size={13} />
                {t("light.revert")}
              </button>
            )}
            <StatusLine status={status} okText={t("light.saved")} />
            <p className="text-[12px] leading-relaxed text-ink-3">{t("light.indicatorNote")}</p>
          </div>
        </section>
      </div>
    </>
  );
}

/**
 * The brush colour at the moment of painting. Read from the store, not from
 * the render closure: a key pressed right after a colour change (before
 * React re-renders) must still get the new colour.
 */
const currentBrush = () => useLighting.getState().brush;

function ToolButton({ icon, label, onClick }: { icon?: React.ReactNode; label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex h-8 items-center gap-1.5 rounded-lg border border-line bg-bg-raise px-3 text-[13px] text-ink-2 transition-colors hover:border-line-strong hover:text-ink"
    >
      {icon}
      {label}
    </button>
  );
}

function hslHex(h: number, s: number, l: number): string {
  const a = (s / 100) * Math.min(l / 100, 1 - l / 100);
  const f = (n: number) => {
    const k = (n + h / 30) % 12;
    const c = l / 100 - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(c * 255)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}
