import {
  AudioLines,
  BoxSelect,
  Check,
  Eraser,
  Keyboard,
  MousePointer2,
  PaintBucket,
  Paintbrush,
  RotateCcw,
  Sparkles,
  Usb,
  Gauge,
  X,
} from "lucide-react";
import { useEffect, useEffectEvent, useMemo, useRef, useState, type ReactNode } from "react";
import type { EffectInfo } from "../bindings/EffectInfo";
import type { EffectMotion } from "../bindings/EffectMotion";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { KeyGeo } from "../bindings/KeyGeo";
import { ActionBar } from "../components/ActionBar";
import { ColorPicker, InfoBar, Row, Rows, Segmented, Steps, Tabs, Toggle } from "../components/controls";
import { EffectThumb } from "../components/EffectThumb";
import { KeyboardView, type KeyAction } from "../components/KeyboardView";
import { getLightingCatalog, setLiveFrame } from "../lib/ipc";
import { useT, type MessageKey } from "../lib/i18n";
import {
  REACTIVE_EFFECTS,
  sameColors,
  sameEffect,
  toLedColors,
  useLighting,
  type ColorMap,
  type PaintTool,
} from "../lib/lighting";
import { useGauges } from "../lib/gauges";
import { useMusic } from "../lib/music";
import { useMirror } from "../lib/mirror";
import { useApp } from "../lib/store";

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
  // So does music mode while sound plays.
  const musicOn = useMusic((s) => !!s.state?.settings.enabled);
  const setPage = useApp((s) => s.setPage);

  return (
    <div className="@container flex flex-col gap-5">
      <header>
        <h1 className="page-title">{t("nav.lighting")}</h1>
        <p className="page-desc">{t("light.desc")}</p>
        <Tabs
          className="mt-3.5"
          label={t("nav.lighting")}
          value={tab}
          onChange={setTab}
          options={[
            { value: "effects", label: t("light.tab.effects"), icon: Sparkles },
            { value: "per-key", label: t("light.tab.perKey"), icon: Keyboard },
          ]}
        />
      </header>

      {!usb && (
        <InfoBar tone="warn" icon={<Usb size={20} strokeWidth={1.6} />} title={t("light.needsUsb")} data="usb" />
      )}
      {usb && gaugesOn && (
        <InfoBar
          icon={<Gauge size={20} strokeWidth={1.6} />}
          title={t("light.gaugesOn")}
          data="gauges"
          actions={
            <button type="button" onClick={() => setPage("gauges")} className="btn-secondary" data-light-gauges>
              {t("light.gaugesGo")}
            </button>
          }
        />
      )}
      {usb && musicOn && (
        <InfoBar
          icon={<AudioLines size={20} strokeWidth={1.6} />}
          title={t("light.musicOn")}
          data="music"
          actions={
            <button type="button" onClick={() => setPage("music")} className="btn-secondary" data-light-music>
              {t("light.musicGo")}
            </button>
          }
        />
      )}

      {tab === "effects" ? (
        <EffectsTab key="effects" kb={kb} usb={usb} mirror={mirror} />
      ) : (
        <PerKeyTab key="per-key" kb={kb} usb={usb} perKey={perKey} />
      )}
    </div>
  );
}

function Plate({
  kb,
  colors,
  caption,
  onKeyAction,
  selected,
  tool,
  onBackgroundPress,
}: {
  kb: KeyboardInfo;
  colors: ColorMap | null;
  caption?: { text: string; live: boolean; aside?: string };
  onKeyAction?: (led: number, action: KeyAction, additive: boolean) => void;
  selected?: ReadonlySet<number>;
  tool?: PaintTool;
  /** Pointer down on the plate outside any key (clears a selection). */
  onBackgroundPress?: () => void;
}) {
  return (
    <section className="dev">
      <div
        onPointerDown={(e) => {
          if (!(e.target as HTMLElement).closest(".kb-key")) onBackgroundPress?.();
        }}
        className="dev-plate relative overflow-hidden"
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
      </div>
      {caption && (
        <div className="dev-live">
          <span className="flex items-center gap-2">
            <span className={`h-[7px] w-[7px] rounded-full ${caption.live ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
            <span className="num">{caption.text}</span>
          </span>
          {caption.aside && <span>{caption.aside}</span>}
        </div>
      )}
    </section>
  );
}

function EffectsTab({ kb, usb, mirror }: { kb: KeyboardInfo; usb: boolean; mirror: ColorMap | null }) {
  const t = useT();
  const { effect, setEffect, applyEffectDraft, appliedEffect, revertEffect } = useLighting();
  const [catalog, setCatalog] = useState<EffectInfo[]>([]);

  useEffect(() => {
    getLightingCatalog().then(setCatalog).catch(console.error);
  }, []);

  const current = catalog.find((e) => e.id === effect.mode);
  const effects = catalog.filter((e) => e.id !== 0);
  const off = catalog.find((e) => e.id === 0);
  // Unknown until the app stores an effect once; then whatever differs.
  const changes = !appliedEffect
    ? t("bar.notSaved")
    : sameEffect(effect, appliedEffect)
      ? null
      : t("bar.effectChanged");

  const card = (fx: EffectInfo) => {
    const selected = fx.id === effect.mode;
    const isApplied = appliedEffect?.mode === fx.id;
    const notes = [REACTIVE_EFFECTS.has(fx.id) && t("light.reactive"), fx.factoryDefault && t("light.factory")].filter(Boolean);
    return (
      <button
        key={fx.id}
        type="button"
        onClick={() => setEffect({ mode: fx.id })}
        aria-pressed={selected}
        className="gallery-item group flex flex-col gap-2 p-2 pb-2.5 text-left"
        data-effect={fx.id}
      >
        <EffectThumb id={fx.id} color={effect.color} rainbow={selected && fx.color && fx.rainbow && effect.rainbow} />
        <span className="flex min-w-0 flex-col px-0.5">
          <span className="flex items-center gap-1.5">
            <span className="truncate text-[13px] leading-[17px]">{t(`fx.${fx.id}` as MessageKey)}</span>
            {isApplied && <span className="ml-auto h-1.5 w-1.5 shrink-0 rounded-full bg-ok" aria-hidden />}
          </span>
          {notes.length > 0 && <span className="num truncate text-[11.5px] leading-4 text-ink-2">{notes.join(" · ")}</span>}
        </span>
        {selected && (
          <span className="gallery-check" aria-hidden>
            <Check size={13} strokeWidth={2.4} />
          </span>
        )}
      </button>
    );
  };
  const rainbowOn = !!current?.color && !!current.rainbow && effect.rainbow;

  return (
    <>
      <Plate
        kb={kb}
        colors={mirror}
        caption={{
          text: mirror ? t("light.mirror.on") : t("light.mirror.off"),
          live: !!mirror,
          aside: current ? [t(`fx.${current.id}` as MessageKey), rainbowOn ? t("light.rainbow") : null].filter(Boolean).join(" · ") : undefined,
        }}
      />

      <div className="grid grid-cols-1 items-start gap-4 @3xl:grid-cols-[minmax(0,1fr)_336px]">
        <section data-effects>
          <h2 className="section-title">
            <span>
              {t("light.effects")}
              <span className="count num">{catalog.length}</span>
            </span>
          </h2>
          <div className="grid grid-cols-2 gap-2 @2xl:grid-cols-4">
            {off && card(off)}
            {effects.map(card)}
          </div>
        </section>

        <section className="@3xl:sticky @3xl:top-4" data-effect-settings>
          <h2 className="section-title">
            <span>{t("light.settings")}</span>
            <span className="text-[13px] font-normal text-ink-2">{current && t(`fx.${current.id}` as MessageKey)}</span>
          </h2>
          <div className="rows">
            {current && current.id === 0 && <InfoBar tone="warn" title={t("light.offWarning")} />}
            {current?.color && (
              <div className="row row-tall">
                <span className="row-title">{t("light.color")}</span>
                {current.rainbow && <span className="row-desc">{t("light.color.rainbowNote")}</span>}
                <div className="mt-2">
                  <ColorPicker
                    label={t("light.color")}
                    value={effect.color}
                    onChange={(color) => setEffect({ color, rainbow: false })}
                    disabled={current.rainbow && effect.rainbow}
                  />
                </div>
              </div>
            )}
            {current?.rainbow && current.color && (
              <Row title={t("light.rainbow")} desc={t("light.rainbow.desc")}>
                <Toggle state label={t("light.rainbow")} checked={effect.rainbow} onChange={(rainbow) => setEffect({ rainbow })} />
              </Row>
            )}
            {current?.brightness && (
              <div className="row row-tall">
                <span className="flex items-baseline justify-between gap-3">
                  <span className="row-title">{t("light.brightness")}</span>
                  <span className="num text-[13px] text-ink-2">{effect.brightness} / 5</span>
                </span>
                <Steps label={t("light.brightness")} value={effect.brightness} onChange={(brightness) => setEffect({ brightness })} />
              </div>
            )}
            {current?.speed && (
              <div className="row row-tall">
                <span className="flex items-baseline justify-between gap-3">
                  <span className="row-title">{t("light.speed")}</span>
                  <span className="num text-[13px] text-ink-2">{effect.speed} / 5</span>
                </span>
                <Steps label={t("light.speed")} value={effect.speed} onChange={(speed) => setEffect({ speed })} />
              </div>
            )}
            {current && current.directions > 0 && (
              <Row title={t("light.direction")}>
                <select
                  value={directionGroup(effect.direction)}
                  onChange={(e) => setEffect({ direction: Number(e.target.value) })}
                  aria-label={t("light.direction")}
                  className="h-8 min-w-[132px] rounded-md border border-line bg-bg-raise px-2.5 text-[13px]"
                  data-direction
                >
                  {DIRECTION_LABELS[current.motion ?? "path"].map((label, v) => (
                    <option key={v} value={v}>
                      {t(label)}
                    </option>
                  ))}
                </select>
              </Row>
            )}
            {current && current.id !== 0 && !current.brightness && !current.speed && !current.color && (
              <Row title={t("light.noSettings")} />
            )}
          </div>
          <button
            type="button"
            onClick={() => setEffect({ mode: 11, rainbow: true, brightness: 5, speed: 3, direction: 0 })}
            className="btn-subtle btn-sm mt-2 text-ink-2"
          >
            <RotateCcw size={13} aria-hidden />
            {t("light.factoryReset")}
          </button>
        </section>
      </div>

      <ActionBar
        changes={changes}
        canSave={usb && !!current}
        onSave={applyEffectDraft}
        onRevert={appliedEffect ? revertEffect : undefined}
      />
    </>
  );
}

/** One empty list, so a keyboard without geometry keeps the memos below stable. */
const NO_KEYS: readonly KeyGeo[] = [];

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
  const keys = kb.geometry?.keys ?? NO_KEYS;
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

  // Select-tool shortcuts; ignored while typing in a field. An effect event,
  // so the listener always sees the current keys and handlers without being
  // added again on every render.
  const onShortcut = useEffectEvent((e: KeyboardEvent) => {
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
  });
  useEffect(() => {
    if (tool !== "select") return;
    const onKey = (e: KeyboardEvent) => onShortcut(e);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tool]);

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

  // What differs from the design stored on the keyboard (none stored: everything).
  const base = savedPerKey?.colors ?? {};
  const changedKeys = changedLeds(base, perKey);
  const brightnessChanged = !!savedPerKey && savedPerKey.brightness !== perKeyBrightness;
  const matchesSaved = !!savedPerKey && !brightnessChanged && sameColors(savedPerKey.colors, perKey);
  const changes = matchesSaved
    ? null
    : changedKeys > 0
      ? t("bar.keysChanged", { count: changedKeys })
      : brightnessChanged
        ? t("bar.brightnessChanged")
        : t("bar.notSaved");

  return (
    <>
      {/* Tool and colour, right above the keys they act on. */}
      <div className="flex flex-wrap items-center gap-x-5 gap-y-3">
        <Segmented<PaintTool>
          label={t("light.tool")}
          value={tool}
          onChange={switchTool}
          options={[
            { value: "brush", label: t("light.tool.brush"), icon: <Paintbrush size={13} /> },
            { value: "select", label: t("light.tool.select"), icon: <MousePointer2 size={13} /> },
          ]}
        />
        <ColorPicker inline label={t("light.brush")} value={brush} onChange={pickColor} />
      </div>

      <Plate
        kb={kb}
        colors={perKey}
        onKeyAction={onKeyAction}
        selected={tool === "select" ? selection : undefined}
        tool={tool}
        onBackgroundPress={() => tool === "select" && selection.size > 0 && select(new Set())}
      />

      <div className="-mt-3 flex min-h-8 flex-wrap items-center justify-between gap-x-6 gap-y-2 text-[13px]">
        {tool === "select" && selection.size > 0 ? (
          <span className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span className="num font-medium text-ink">{t("light.keysSelected", { count: selection.size })}</span>
            <span className="text-[12px] text-ink-3">{t("light.selectShortcuts")}</span>
          </span>
        ) : (
          <span className="max-w-[80ch] text-ink-2">{t(tool === "brush" ? "light.tool.brushHint" : "light.tool.selectHint")}</span>
        )}
        {tool === "select" && (
          <span className="flex flex-wrap gap-2">
            <ToolButton icon={<BoxSelect size={14} />} label={t("light.selectAll")} onClick={selectAll} />
            {selection.size > 0 && (
              <>
                <ToolButton icon={<Eraser size={14} />} label={t("light.eraseSelected")} onClick={eraseSelection} />
                <ToolButton icon={<X size={14} />} label={t("light.deselect")} onClick={() => select(new Set())} />
              </>
            )}
          </span>
        )}
      </div>

      <Rows>
        <Row title={t("light.brightness")} desc={<span className="num">{perKeyBrightness} / 5</span>}>
          <div className="w-[264px] max-w-full">
            <Steps label={t("light.brightness")} value={perKeyBrightness} onChange={setPerKeyBrightness} />
          </div>
        </Row>
        <Row title={t("light.presets")}>
          <div className="flex flex-wrap justify-end gap-2">
            <ToolButton label={t("light.preset.wasd")} onClick={() => preset("wasd")} />
            <ToolButton label={t("light.preset.home")} onClick={() => preset("home")} />
            <ToolButton label={t("light.preset.rainbow")} onClick={() => preset("rainbow")} />
          </div>
        </Row>
        <Row title={t("light.tools")} desc={`${t("light.paintHint")} ${t("light.indicatorNote")}`}>
          <div className="flex flex-wrap justify-end gap-2">
            <ToolButton
              icon={<PaintBucket size={14} />}
              label={t("light.fillAll")}
              onClick={() => paint(keys.map((k) => k.led), currentBrush())}
            />
            <ToolButton icon={<Eraser size={14} />} label={t("light.clear")} onClick={() => setPerKey({})} />
          </div>
        </Row>
      </Rows>

      <ActionBar
        left={
          <label className="flex items-center gap-2.5 text-[14px]" title={t("light.livePreviewHint")}>
            <span className={usb ? "text-ink" : "text-ink-3"}>{t("light.livePreview")}</span>
            <Toggle label={t("light.livePreview")} checked={livePreview} onChange={setLivePreview} disabled={!usb} />
          </label>
        }
        changes={changes}
        canSave={usb}
        onSave={savePerKeyDraft}
        onRevert={savedPerKey ? revertPerKey : undefined}
      />
    </>
  );
}

/** Keys whose colour differs between two designs (painted, recoloured or cleared). */
function changedLeds(a: ColorMap, b: ColorMap): number {
  const leds = new Set([...Object.keys(a), ...Object.keys(b)]);
  let n = 0;
  for (const led of leds) if (a[Number(led)]?.toLowerCase() !== b[Number(led)]?.toLowerCase()) n += 1;
  return n;
}

/**
 * The brush colour at the moment of painting. Read from the store, not from
 * the render closure: a key pressed right after a colour change (before
 * React re-renders) must still get the new colour.
 */
const currentBrush = () => useLighting.getState().brush;

function ToolButton({ icon, label, onClick }: { icon?: ReactNode; label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="btn-secondary btn-sm"
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
