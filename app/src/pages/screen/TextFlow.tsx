// Source "Yazı kartı": text and template cards drawn here, in the panel's
// own colours (phase B1).

import { AlignCenter, AlignLeft, AlignRight, Type } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Field, Segmented, Slider, Toggle } from "../../components/controls";
import { useT, type MessageKey } from "../../lib/i18n";
import { H, PROVEN_FRAMES, W, toRgba, type Prepared } from "../../lib/screenImport";
import {
  EMOJI_PICKS,
  PALETTES,
  SIZES,
  TEMPLATES,
  USES,
  cardFrame,
  defaultSpec,
  ensureFonts,
  layoutCard,
  makeLayers,
  prepareCard,
  type Align,
  type CardLayers,
  type CardLayout,
  type CardSpec,
  type FontId,
  type GradDir,
  type Palette,
  type TemplateId,
} from "../../lib/screenText";
import { useApp } from "../../lib/store";
import { EditFooter, Hint, PreviewPanel, StudioFrame, type Done, type Step } from "./common";
import { friendlyError } from "../../lib/errors";

type T = (k: MessageKey) => string;

const TEMPLATE_KEY: Record<TemplateId, MessageKey> = {
  name: "sc.tx.t.name",
  quote: "sc.tx.t.quote",
  marquee: "sc.tx.t.marquee",
  mountain: "sc.tx.t.mountain",
  badge: "sc.tx.t.badge",
};

function sample(t: T, id: TemplateId) {
  return {
    title: t(`sc.tx.sample.${id}.t` as MessageKey),
    subtitle: t(`sc.tx.sample.${id}.s` as MessageKey),
  };
}

/** The spec a template starts with: the user's own text once they typed. */
function withTemplate(spec: CardSpec, id: TemplateId, edited: boolean, t: T): CardSpec {
  return {
    ...spec,
    ...(edited ? {} : sample(t, id)),
    template: id,
    size: SIZES[id].def,
  };
}

/** The last card in this session, so "Yazı kartı" reopens where it was. */
let remembered: { spec: CardSpec; edited: boolean } | null = null;

/**
 * `useT()` returns a new function on every render; this one changes only
 * with the language, so the sample-text memos below recompute when the
 * language changes and not on every render.
 */
function useLocaleT(): T {
  const t = useT();
  const locale = useApp((s) => s.locale);
  // eslint-disable-next-line react-hooks/exhaustive-deps -- `t` is new every render but depends only on `locale`
  return useCallback((k: MessageKey) => t(k), [locale]);
}

// ---------------------------------------------------------------- canvas

/** Draws a card exactly as the panel will show it, animating if it moves. */
function CardCanvas({
  spec,
  className,
  animate = true,
  onLayout,
}: {
  spec: CardSpec;
  className: string;
  animate?: boolean;
  onLayout?: (l: CardLayout) => void;
}) {
  const locale = useApp((s) => s.locale);
  const ref = useRef<HTMLCanvasElement>(null);
  const layers = useRef<CardLayers | null>(null);
  const report = useRef(onLayout);
  report.current = onLayout;

  useEffect(() => {
    let live = true;
    let timer: number | undefined;
    void (async () => {
      await ensureFonts(spec);
      const ctx = ref.current?.getContext("2d");
      if (!live || !ctx) return;
      const ls = (layers.current ??= makeLayers());
      const layout = layoutCard(spec, ls.fg, locale);
      report.current?.(layout);
      const img = new ImageData(W, H);
      let i = 0;
      const tick = () => {
        if (!live) return;
        toRgba(cardFrame(spec, layout, ls, i), img.data);
        ctx.putImageData(img, 0, 0);
        if (!animate || layout.frames < 2) return;
        i = (i + 1) % layout.frames;
        timer = window.setTimeout(tick, layout.delayMs);
      };
      tick();
    })();
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [spec, locale, animate]);

  return <canvas ref={ref} width={W} height={H} className={`pixelated bg-black ${className}`} />;
}

// ------------------------------------------------------------ step 1 tile

export function TextTile({ onStart }: { onStart: () => void }) {
  const t = useT();
  const [k, setK] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => setK((v) => (v + 1) % TEMPLATES.length), 3200);
    return () => window.clearInterval(id);
  }, []);
  const id = TEMPLATES[k]!;
  const localeT = useLocaleT();
  const spec = useMemo(() => withTemplate(defaultSpec(id), id, false, localeT), [id, localeT]);
  return (
    <div
      data-tile="text"
      className="flex min-h-[260px] flex-col items-center justify-center gap-4 rounded-lg border border-line bg-surface-2/30 p-6 text-center"
    >
      <CardCanvas spec={spec} className="h-[90px] w-[160px] rounded-md shadow-[0_0_0_4px_#0b0c0e,0_0_0_5px_var(--line)]" />
      <div>
        <h3 className="font-display text-[19px] font-semibold">{t("sc.src.text")}</h3>
        <p className="mt-1 max-w-[40ch] text-[13px] text-ink-2">{t("sc.src.textBody")}</p>
      </div>
      <button
        type="button"
        onClick={onStart}
        className="flex items-center gap-2 rounded-lg border border-signal/50 px-4 py-2 text-[13px] font-semibold text-signal hover:bg-signal/10"
      >
        <Type size={15} /> {t("sc.src.textGo")}
      </button>
    </div>
  );
}

// ------------------------------------------------------------- the flow

export function TextFlow({
  usb,
  loaded,
  onCancel,
  onFinished,
}: {
  usb: boolean;
  loaded: boolean;
  onCancel: () => void;
  onFinished: (d: Done) => void;
}) {
  const t = useT();
  const localeT = useLocaleT();
  const locale = useApp((s) => s.locale);
  const [spec, setSpec] = useState<CardSpec>(() => remembered?.spec ?? withTemplate(defaultSpec(), "name", false, t));
  const [edited, setEdited] = useState(remembered?.edited ?? false);
  const [layout, setLayout] = useState<CardLayout | null>(null);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [preparing, setPreparing] = useState<{ done: number; total: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const autoName = useRef("");
  const abort = useRef<AbortController | null>(null);
  const step: Step = busy ? 4 : prepared ? 3 : 2;
  const uses = USES[spec.template];
  const fallbackName = spec.title.trim().slice(0, 60) || t(TEMPLATE_KEY[spec.template]);

  useEffect(() => {
    remembered = { spec, edited };
    abort.current?.abort();
    setPreparing(null);
    setError(null);
  }, [spec, edited]);
  useEffect(() => () => abort.current?.abort(), []);

  const set = (patch: Partial<CardSpec>) => setSpec((s) => ({ ...s, ...patch }));
  const setText = (patch: Partial<CardSpec>) => {
    setEdited(true);
    set(patch);
  };

  const runPreview = async () => {
    abort.current?.abort();
    const ctrl = new AbortController();
    abort.current = ctrl;
    setError(null);
    setPreparing({ done: 0, total: layout?.frames ?? 1 });
    try {
      const r = await prepareCard(spec, locale, fallbackName, (done, total) => setPreparing({ done, total }), ctrl.signal);
      if (!ctrl.signal.aborted) {
        // Follow the title until the user names it themselves.
        if (!name.trim() || name === autoName.current) setName(fallbackName);
        autoName.current = fallbackName;
        setPrepared(r);
      }
    } catch (e) {
      if ((e as Error).name !== "AbortError") setError(friendlyError(e, t));
    } finally {
      if (abort.current === ctrl) setPreparing(null);
    }
  };

  const frames = layout?.frames ?? 1;
  const loopMs = frames * (layout?.delayMs ?? 100);
  const contrast = Math.min(contrastRatio(spec.fg, spec.bg1), spec.gradient ? contrastRatio(spec.fg, spec.bg2) : 99);
  const raised = layout?.speed != null && layout.speed > spec.speed * 1.15;
  const thumbs = useMemo(() => TEMPLATES.map((id) => withTemplate(spec, id, edited, localeT)), [spec, edited, localeT]);

  return (
    <StudioFrame step={step} locked={busy} onCancel={onCancel} title={t("sc.src.text")} detail={t(TEMPLATE_KEY[spec.template])}>
      <div className={prepared ? "hidden" : "flex flex-col gap-5"}>
        <Field label={t("sc.tx.template")}>
          <div role="radiogroup" aria-label={t("sc.tx.template")} className="flex flex-wrap gap-2">
            {TEMPLATES.map((id, k) => (
              <TemplateThumb
                key={id}
                id={id}
                spec={thumbs[k]!}
                selected={spec.template === id}
                label={t(TEMPLATE_KEY[id])}
                onPick={() => setSpec((s) => withTemplate(s, id, edited, t))}
              />
            ))}
          </div>
        </Field>

        <div className="grid grid-cols-1 gap-6 @min-[860px]:grid-cols-[480px_minmax(280px,1fr)]">
          <div className="flex flex-col gap-2">
            <CardCanvas spec={spec} className="h-[270px] w-[480px] rounded-lg" onLayout={setLayout} />
            <p className="text-[12px] text-ink-3">{t("sc.tx.liveNote")}</p>
            <div className="mt-3">
              <Colors spec={spec} set={set} />
            </div>
          </div>

          <div className="flex flex-col gap-4">
            <TextFields spec={spec} setText={setText} />

            <Field label={t("sc.tx.font")}>
              <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                <Segmented<FontId>
                  label={t("sc.tx.font")}
                  value={spec.font}
                  onChange={(font) => set({ font })}
                  options={[
                    { value: "display", label: t("sc.tx.font.display") },
                    { value: "sans", label: t("sc.tx.font.sans") },
                    { value: "mono", label: t("sc.tx.font.mono") },
                  ]}
                />
                <span className="flex items-center gap-2.5">
                  <span className="text-[13px] text-ink-2">{t("sc.tx.upper")}</span>
                  <Toggle checked={spec.upper} onChange={(upper) => set({ upper })} label={t("sc.tx.upper")} />
                </span>
              </div>
            </Field>
            <Slider
              label={t("sc.tx.size")}
              hint={layout?.shrunkTo ? t("sc.tx.shrunk", { size: layout.shrunkTo }) : undefined}
              value={spec.size}
              min={SIZES[spec.template].min}
              max={SIZES[spec.template].max}
              step={1}
              onChange={(size) => set({ size })}
              format={(v) => `${v} px`}
            />
            {uses.align && (spec.template !== "marquee" || spec.subtitle.trim() !== "") && (
              <Field label={spec.template === "marquee" ? t("sc.tx.alignTop") : t("sc.tx.align")}>
                <Segmented<Align>
                  label={t("sc.tx.align")}
                  value={spec.align}
                  onChange={(align) => set({ align })}
                  options={[
                    { value: "left", label: t("sc.tx.align.left"), icon: <AlignLeft size={14} /> },
                    { value: "center", label: t("sc.tx.align.center"), icon: <AlignCenter size={14} /> },
                    { value: "right", label: t("sc.tx.align.right"), icon: <AlignRight size={14} /> },
                  ]}
                />
              </Field>
            )}
            {uses.motion && (
              <>
                <Slider
                  label={t("sc.tx.speed")}
                  value={spec.speed}
                  min={20}
                  max={200}
                  step={5}
                  onChange={(speed) => set({ speed })}
                  format={(v) => t("sc.tx.pxs", { speed: v })}
                />
                <Field label={t("sc.fps")} hint={`${Math.round(1000 / spec.fps)} ms / ${t("sc.frame")}`}>
                  <Segmented<string>
                    label={t("sc.fps")}
                    value={String(spec.fps)}
                    onChange={(v) => set({ fps: Number(v) })}
                    options={[10, 12, 15, 20, 25].map((f) => ({ value: String(f), label: String(f) }))}
                  />
                </Field>
              </>
            )}
          </div>
        </div>

        <EditFooter frames={frames} loopMs={loopMs} preparing={preparing} disabled={!layout} onPreview={runPreview} />

        {contrast < 2.2 && <Hint tone="warn">{t("sc.tx.lowContrast")}</Hint>}
        {raised && (
          <Hint tone="note">
            {t("sc.tx.speedRaised", { speed: t("sc.tx.pxs", { speed: layout!.speed! }) })}
          </Hint>
        )}
        {frames > PROVEN_FRAMES && <Hint tone="note">{t("sc.warn.unproven")}</Hint>}
        {error && <Hint tone="error">{error}</Hint>}
      </div>

      {prepared && (
        <PreviewPanel
          prepared={prepared}
          name={name}
          onName={setName}
          fallbackName={fallbackName}
          usb={usb}
          loaded={loaded}
          onBack={() => setPrepared(null)}
          onBusy={setBusy}
          onFinished={onFinished}
        />
      )}
    </StudioFrame>
  );
}

function TemplateThumb({
  id,
  spec,
  selected,
  label,
  onPick,
}: {
  id: TemplateId;
  spec: CardSpec;
  selected: boolean;
  label: string;
  onPick: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      data-template={id}
      onClick={onPick}
      className={`flex flex-col items-center gap-1.5 rounded-lg p-1.5 transition-colors ${
        selected ? "bg-signal/10 shadow-[inset_0_0_0_1.5px_var(--signal)]" : "hover:bg-surface-2"
      }`}
    >
      <CardCanvas spec={spec} animate={false} className="h-[68px] w-[120px] rounded-md" />
      <span className={`text-[12px] ${selected ? "font-semibold text-ink" : "text-ink-2"}`}>{label}</span>
    </button>
  );
}

const inputCls =
  "h-9 rounded-lg border border-line bg-bg-raise px-3 text-[13px] outline-none focus:border-signal placeholder:text-ink-3";

function TextFields({ spec, setText }: { spec: CardSpec; setText: (p: Partial<CardSpec>) => void }) {
  const t = useT();
  const id = spec.template;
  const titleLabel = id === "quote" ? t("sc.tx.quote") : id === "marquee" ? t("sc.tx.text") : t("sc.tx.title");
  const subLabel = id === "quote" ? t("sc.tx.author") : id === "marquee" ? t("sc.tx.topLine") : t("sc.tx.subtitle");
  return (
    <div className="flex flex-col gap-3">
      {USES[id].emoji && (
        <Field label={t("sc.tx.badge")} hint={t("sc.tx.badgeHint")}>
          <div className="flex flex-wrap items-center gap-1.5">
            <input
              value={spec.emoji}
              maxLength={8}
              aria-label={t("sc.tx.badge")}
              onChange={(e) => setText({ emoji: e.target.value })}
              className={`${inputCls} w-16 text-center text-[16px]`}
            />
            {EMOJI_PICKS.map((e) => (
              <button
                key={e}
                type="button"
                onClick={() => setText({ emoji: e })}
                aria-label={e}
                className={`flex h-8 w-8 items-center justify-center rounded-md text-[16px] hover:bg-surface-2 ${
                  spec.emoji === e ? "bg-surface-2 shadow-[inset_0_0_0_1px_var(--line-strong)]" : ""
                }`}
              >
                {e}
              </button>
            ))}
          </div>
        </Field>
      )}
      <Field label={titleLabel}>
        {id === "quote" ? (
          <textarea
            value={spec.title}
            maxLength={200}
            rows={3}
            onChange={(e) => setText({ title: e.target.value })}
            className="resize-none rounded-lg border border-line bg-bg-raise px-3 py-2 text-[13px] outline-none focus:border-signal"
          />
        ) : (
          <input
            value={spec.title}
            maxLength={id === "marquee" ? 120 : 60}
            onChange={(e) => setText({ title: e.target.value })}
            className={inputCls}
          />
        )}
      </Field>
      <Field label={subLabel} hint={t("sc.tx.optional")}>
        <input
          value={spec.subtitle}
          maxLength={80}
          onChange={(e) => setText({ subtitle: e.target.value })}
          className={inputCls}
        />
      </Field>
    </div>
  );
}

const PALETTE_KEY: Record<string, MessageKey> = {
  night: "sc.tx.pal.night",
  sunset: "sc.tx.pal.sunset",
  forest: "sc.tx.pal.forest",
  neon: "sc.tx.pal.neon",
  paper: "sc.tx.pal.paper",
  mono: "sc.tx.pal.mono",
};

function isPalette(spec: CardSpec, p: Palette) {
  const eq = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  return (
    spec.gradient === p.gradient &&
    eq(spec.bg1, p.bg1) &&
    (!p.gradient || (eq(spec.bg2, p.bg2) && spec.dir === p.dir)) &&
    eq(spec.fg, p.fg) &&
    eq(spec.accent, p.accent)
  );
}

function Colors({ spec, set }: { spec: CardSpec; set: (p: Partial<CardSpec>) => void }) {
  const t = useT();
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("sc.tx.colors")}>
        <div role="radiogroup" aria-label={t("sc.tx.palette")} className="flex flex-wrap gap-1.5">
          {PALETTES.map((p) => {
            const on = isPalette(spec, p);
            return (
              <button
                key={p.id}
                type="button"
                role="radio"
                aria-checked={on}
                title={t(PALETTE_KEY[p.id]!)}
                onClick={() => set({ gradient: p.gradient, bg1: p.bg1, bg2: p.bg2, dir: p.dir, fg: p.fg, accent: p.accent })}
                className={`flex items-center gap-2 rounded-lg border px-2 py-1 text-[12px] ${
                  on ? "border-signal text-ink" : "border-line text-ink-2 hover:border-line-strong"
                }`}
              >
                <span
                  className="flex h-5 w-8 items-center justify-center rounded-[4px] text-[11px] font-bold"
                  style={{
                    background: p.gradient ? `linear-gradient(${p.dir === "h" ? "90deg" : p.dir === "d" ? "135deg" : "180deg"}, ${p.bg1}, ${p.bg2})` : p.bg1,
                    color: p.fg,
                    boxShadow: `inset 0 -2px 0 ${p.accent}`,
                  }}
                >
                  Aa
                </span>
                {t(PALETTE_KEY[p.id]!)}
              </button>
            );
          })}
        </div>
      </Field>
      <div className="flex flex-wrap gap-2">
        <ColorChip label={t("sc.tx.fg")} value={spec.fg} onChange={(fg) => set({ fg })} />
        <ColorChip label={t("sc.tx.accent")} value={spec.accent} onChange={(accent) => set({ accent })} />
        <ColorChip label={spec.gradient ? t("sc.tx.bg1") : t("sc.tx.bg")} value={spec.bg1} onChange={(bg1) => set({ bg1 })} />
        {spec.gradient && <ColorChip label={t("sc.tx.bg2")} value={spec.bg2} onChange={(bg2) => set({ bg2 })} />}
      </div>
      <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
        <span className="flex items-center gap-2.5">
          <Toggle checked={spec.gradient} onChange={(gradient) => set({ gradient })} label={t("sc.tx.gradient")} />
          <span className="text-[13px] font-medium">{t("sc.tx.gradient")}</span>
        </span>
        {spec.gradient && (
          <Segmented<GradDir>
            label={t("sc.tx.dir")}
            value={spec.dir}
            onChange={(dir) => set({ dir })}
            options={[
              { value: "v", label: t("sc.tx.dir.v") },
              { value: "h", label: t("sc.tx.dir.h") },
              { value: "d", label: t("sc.tx.dir.d") },
            ]}
          />
        )}
      </div>
    </div>
  );
}

function ColorChip({ label, value, onChange }: { label: string; value: string; onChange: (hex: string) => void }) {
  return (
    <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-line bg-bg-raise py-1 pr-2.5 pl-1 hover:border-line-strong">
      <span className="relative h-7 w-7 shrink-0 overflow-hidden rounded-md border border-black/20" style={{ background: value }}>
        <input
          type="color"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          aria-label={label}
          className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
        />
      </span>
      <span className="flex flex-col leading-tight">
        <span className="text-[12px] text-ink-2">{label}</span>
        <span className="num text-[11px] text-ink-3">{value.toUpperCase()}</span>
      </span>
    </label>
  );
}

// ----------------------------------------------------------------- utils

function relLum(hex: string) {
  const v = parseInt(hex.slice(1), 16);
  const c = [(v >> 16) & 255, (v >> 8) & 255, v & 255].map((x) => {
    const s = x / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * c[0]! + 0.7152 * c[1]! + 0.0722 * c[2]!;
}

function contrastRatio(a: string, b: string) {
  const x = relLum(a);
  const y = relLum(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
