// Source "Slayt": several pictures in one loop, from files and from the
// library (phase B2).

import { ArrowDown, ArrowUp, FilePlus2, Images, Library, Loader2, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import type { ScreenItem } from "../../bindings/ScreenItem";
import { Field, Segmented, Toggle } from "../../components/controls";
import { useT } from "../../lib/i18n";
import { useScreen } from "../../lib/screen";
import { MAX_FRAMES, PROVEN_FRAMES, openSource, type Prepared, type Source } from "../../lib/screenImport";
import {
  MAX_SLIDES,
  SLIDE_DEFAULT_S,
  SLIDE_MAX_S,
  SLIDE_MIN_S,
  assemble,
  bestMoment,
  budget,
  renderStill,
  type SlideFit,
  type Transition,
} from "../../lib/screenSlides";
import {
  EditFooter,
  FramePlayer,
  Hint,
  PreviewPanel,
  StudioFrame,
  Thumb,
  loadFrames,
  pickThumb,
  useItemName,
  type Done,
  type Step,
} from "./common";

const ACCEPT = "image/*,video/mp4,video/webm,.gif,.webp,.apng,.png,.jpg,.jpeg,.bmp,.avif,.mp4,.webm,.m4v,.mov";

type Slide =
  | { key: string; kind: "file"; name: string; source: Source; atMs: number; fit: SlideFit; seconds: number }
  | { key: string; kind: "library"; name: string; seconds: number };

/** Where a slide's panel frame is cached: files per fit and dither. */
const stillKey = (s: Slide, dither: boolean) => (s.kind === "file" ? `${s.key}:${s.fit}:${dither ? 1 : 0}` : s.key);

// ------------------------------------------------------------ step 1 tile

export function SlideTile({ onStart }: { onStart: () => void }) {
  const t = useT();
  return (
    <div
      data-tile="slides"
      className="flex min-h-[260px] flex-col items-center justify-center gap-4 rounded-lg border border-line bg-surface-2/30 p-6 text-center"
    >
      <SlideStackArt />
      <div>
        <h3 className="font-display text-[19px] font-semibold">{t("sc.src.slides")}</h3>
        <p className="mt-1 max-w-[40ch] text-[13px] text-ink-2">{t("sc.src.slidesBody")}</p>
      </div>
      <button
        type="button"
        onClick={onStart}
        className="flex items-center gap-2 rounded-lg border border-signal/50 px-4 py-2 text-[13px] font-semibold text-signal hover:bg-signal/10"
      >
        <Images size={15} /> {t("sc.src.slidesGo")}
      </button>
    </div>
  );
}

/** Three library pictures fanned out, or plain panels when it is empty. */
const NO_ITEMS: ScreenItem[] = [];

function SlideStackArt() {
  // The store value itself (lesson 33): a fresh [] each call loops forever.
  const items = useScreen((s) => s.state?.items) ?? NO_ITEMS;
  const picks = items.slice(0, 3);
  return (
    <div className="relative h-[90px] w-[200px]">
      {[0, 1, 2].map((k) => {
        // The front (middle) card gets the library's first picture.
        const it = picks[k === 1 ? 0 : k === 0 ? 1 : 2];
        return (
          <div
            key={k}
            className="absolute top-[11px] left-[40px] h-[68px] w-[120px] overflow-hidden rounded-md bg-black shadow-[0_0_0_3px_#0b0c0e,0_0_0_4px_var(--line)]"
            style={{ transform: `translateX(${(k - 1) * 34}px) rotate(${(k - 1) * 7}deg)`, zIndex: k === 1 ? 2 : 1 }}
          >
            {it ? (
              <Thumb item={it} />
            ) : (
              <div className="h-full w-full bg-gradient-to-br from-[#0d2a5a] to-[#071a3c]" style={{ opacity: 0.6 + k * 0.2 }} />
            )}
          </div>
        );
      })}
    </div>
  );
}

// ------------------------------------------------------------- the flow

export function SlideFlow({
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
  const itemName = useItemName();
  const [slides, setSlides] = useState<Slide[]>([]);
  const [transition, setTransition] = useState<Transition>("fade");
  const [dither, setDither] = useState(true);
  const [adding, setAdding] = useState<{ done: number; total: number } | null>(null);
  const [failed, setFailed] = useState<string[]>([]);
  const [picker, setPicker] = useState(false);
  const [over, setOver] = useState(false);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [busy, setBusy] = useState(false);
  const [name, setName] = useState("");
  const autoName = useRef("");
  const input = useRef<HTMLInputElement>(null);
  // Slide keys also key the still cache, so they must never repeat.
  const counter = useRef(0);
  const newKey = () => `s${++counter.current}`;
  const step: Step = busy ? 4 : prepared ? 3 : 2;

  // Panel frames per slide (and per fit/dither for files), kept between edits.
  // A still is stored once under its key and never replaced; a new one
  // re-renders (`setVersion`) so `ready` below sees it.
  const stills = useRef(new Map<string, Uint16Array>());
  const [, setVersion] = useState(0);

  // Decoded files hold bitmaps and decoders: free them with their slide.
  const sources = useRef(new Set<Source>());
  useEffect(
    () => () => {
      for (const s of sources.current) s.dispose();
      sources.current.clear();
    },
    [],
  );

  useEffect(() => {
    let live = true;
    void (async () => {
      for (const s of slides) {
        if (s.kind !== "file") continue;
        const k = stillKey(s, dither);
        if (stills.current.has(k)) continue;
        const px = await renderStill(s.source, s.fit, s.atMs, dither);
        if (!live) return;
        stills.current.set(k, px);
        setVersion((v) => v + 1);
      }
    })().catch(console.error);
    return () => {
      live = false;
    };
  }, [slides, dither]);

  const seconds = useMemo(() => slides.map((s) => s.seconds), [slides]);
  const plan = budget(seconds, transition);
  const ready = slides.length > 0 && slides.every((s) => stills.current.has(stillKey(s, dither)));
  // Stills are only ever added, so once `ready` is true for these slides and
  // this dither, the frames are fixed: `ready` turning true is the signal.
  const show = useMemo(
    () => (ready ? assemble(slides.map((s) => stills.current.get(stillKey(s, dither))!), seconds, transition) : null),
    [ready, slides, dither, seconds, transition],
  );
  const full = slides.length >= MAX_SLIDES;
  const fallbackName = `${t("sc.src.slides")} · ${t("sc.sl.images", { count: slides.length })}`;

  const addFiles = async (files: File[]) => {
    const room = MAX_SLIDES - slides.length;
    const list = files.slice(0, Math.max(0, room));
    if (list.length === 0) return;
    setFailed([]);
    setAdding({ done: 0, total: list.length });
    const bad: string[] = [];
    for (let i = 0; i < list.length; i++) {
      const f = list[i]!;
      try {
        const source = await openSource(f);
        sources.current.add(source);
        const atMs = await bestMoment(source, "fill", dither);
        const slide: Slide = { key: newKey(), kind: "file", name: source.name, source, atMs, fit: "fill", seconds: SLIDE_DEFAULT_S };
        setSlides((s) => [...s, slide]);
      } catch (e) {
        console.error(e);
        bad.push(f.name);
      }
      setAdding({ done: i + 1, total: list.length });
    }
    setAdding(null);
    setFailed(bad);
  };

  const addLibrary = async (item: ScreenItem) => {
    if (full) return;
    try {
      const d = await loadFrames(item.id, item.frames === 1);
      const still = pickThumb(d.frames);
      if (!still) return;
      const key = newKey();
      stills.current.set(key, still);
      setSlides((s) => [...s, { key, kind: "library", name: itemName(item), seconds: SLIDE_DEFAULT_S }]);
    } catch (e) {
      console.error(e);
      setFailed([itemName(item)]);
    }
  };

  const update = (key: string, patch: Partial<Pick<Slide, "seconds">> & { fit?: SlideFit }) =>
    setSlides((list) => list.map((s) => (s.key === key ? ({ ...s, ...patch } as Slide) : s)));

  const move = (i: number, d: -1 | 1) =>
    setSlides((list) => {
      const j = i + d;
      if (j < 0 || j >= list.length) return list;
      const next = [...list];
      [next[i], next[j]] = [next[j]!, next[i]!];
      return next;
    });

  const remove = (key: string) => {
    const s = slides.find((x) => x.key === key);
    if (s?.kind === "file") {
      s.source.dispose();
      sources.current.delete(s.source);
    }
    setSlides((list) => list.filter((x) => x.key !== key));
  };

  const toPreview = () => {
    if (!show) return;
    // Follow the slide count until the user names it themselves.
    if (!name.trim() || name === autoName.current) setName(fallbackName);
    autoName.current = fallbackName;
    setPrepared({ frames: show.frames, delaysMs: show.delaysMs, name: fallbackName, kind: "animation" });
  };

  return (
    <StudioFrame
      step={step}
      locked={busy}
      onCancel={onCancel}
      title={t("sc.src.slides")}
      detail={t("sc.sl.images", { count: slides.length })}
    >
      <div
        className={prepared ? "hidden" : "flex flex-col gap-5"}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node)) setOver(false);
        }}
        onDrop={(e) => {
          e.preventDefault();
          setOver(false);
          void addFiles([...e.dataTransfer.files]);
        }}
      >
        <div className="grid grid-cols-1 gap-6 @min-[860px]:grid-cols-[480px_minmax(280px,1fr)]">
          <div className="flex flex-col gap-2">
            <div className="relative h-[270px] w-[480px] overflow-hidden rounded-lg bg-black">
              {show ? (
                <FramePlayer frames={show.frames} delaysMs={show.delaysMs} className="h-full w-full" />
              ) : (
                <div className="flex h-full items-center justify-center text-[13px] text-white/60">
                  {slides.length ? <Loader2 size={18} className="animate-spin" /> : t("sc.sl.noSlides")}
                </div>
              )}
              {over && (
                <div className="absolute inset-0 flex items-center justify-center border-2 border-dashed border-signal bg-signal/15 text-[13px] font-semibold text-white">
                  {t("sc.sl.dropHere")}
                </div>
              )}
            </div>
            <p className="text-[12px] text-ink-3">{t("sc.sl.liveNote")}</p>
            <BudgetMeter frames={plan.frames} />
          </div>

          <div className="flex min-w-0 flex-col gap-4">
            <div className="flex flex-wrap items-end justify-between gap-x-5 gap-y-3">
              <Field label={t("sc.sl.transition")}>
                <span className="flex items-center gap-3">
                  <Segmented<Transition>
                    label={t("sc.sl.transition")}
                    value={transition}
                    onChange={setTransition}
                    options={[
                      { value: "cut", label: t("sc.sl.cut") },
                      { value: "fade", label: t("sc.sl.fade") },
                    ]}
                  />
                  {transition === "fade" && <span className="num text-[11px] text-ink-3">{t("sc.sl.fadeHint")}</span>}
                </span>
              </Field>
              <span className="flex items-center gap-2.5 pb-1.5" title={t("sc.dither.hint")}>
                <span className="text-[13px] text-ink-2">{t("sc.dither")}</span>
                <Toggle checked={dither} onChange={setDither} label={t("sc.dither")} />
              </span>
            </div>

            {slides.length === 0 ? (
              <p className="rounded-lg border border-dashed border-line px-4 py-6 text-center text-[13px] text-ink-3">
                {t("sc.sl.empty")}
              </p>
            ) : (
              <ol className="flex max-h-[360px] flex-col gap-2 overflow-y-auto pr-1">
                {slides.map((s, i) => (
                  <SlideRow
                    key={s.key}
                    index={i}
                    count={slides.length}
                    slide={s}
                    still={stills.current.get(stillKey(s, dither)) ?? null}
                    onSeconds={(seconds) => update(s.key, { seconds })}
                    onFit={(fit) => update(s.key, { fit })}
                    onMove={(d) => move(i, d)}
                    onRemove={() => remove(s.key)}
                  />
                ))}
              </ol>
            )}

            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                disabled={full || !!adding}
                onClick={() => input.current?.click()}
                className="flex h-9 items-center gap-2 rounded-lg border border-line px-3 text-[13px] font-medium hover:bg-surface-2 disabled:opacity-40"
              >
                {adding ? <Loader2 size={14} className="animate-spin" /> : <FilePlus2 size={14} />}
                {adding ? t("sc.sl.adding", { done: adding.done, total: adding.total }) : t("sc.sl.addFiles")}
              </button>
              <button
                type="button"
                disabled={full}
                aria-expanded={picker}
                onClick={() => setPicker((v) => !v)}
                className={`flex h-9 items-center gap-2 rounded-lg border px-3 text-[13px] font-medium disabled:opacity-40 ${
                  picker ? "border-signal text-signal" : "border-line hover:bg-surface-2"
                }`}
              >
                <Library size={14} /> {t("sc.sl.addLibrary")}
              </button>
              <input
                ref={input}
                type="file"
                multiple
                hidden
                accept={ACCEPT}
                onChange={(e) => {
                  const files = [...(e.target.files ?? [])];
                  e.target.value = "";
                  void addFiles(files);
                }}
              />
            </div>
            {picker && <LibraryPicker onPick={addLibrary} onClose={() => setPicker(false)} disabled={full} />}
          </div>
        </div>

        <EditFooter
          frames={plan.frames}
          loopMs={plan.loopMs}
          preparing={null}
          disabled={!show || !plan.fits || slides.length < 2}
          onPreview={toPreview}
        />

        {slides.length === 1 && <Hint tone="note">{t("sc.sl.needTwo")}</Hint>}
        {!plan.fits && <Hint tone="warn">{t("sc.sl.over")}</Hint>}
        {plan.fits && plan.frames > PROVEN_FRAMES && <Hint tone="note">{t("sc.warn.unproven")}</Hint>}
        {full && <Hint tone="note">{t("sc.sl.full")}</Hint>}
        {failed.length > 0 && (
          <Hint tone="error">
            {t("sc.sl.failed", { files: failed.join(", ") })}
          </Hint>
        )}
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

function BudgetMeter({ frames }: { frames: number }) {
  const t = useT();
  const pct = Math.min(100, (frames / MAX_FRAMES) * 100);
  const over = frames > MAX_FRAMES;
  return (
    <div className="mt-2 flex flex-col gap-1.5" aria-label={t("sc.sl.budget")}>
      <div className="flex items-baseline justify-between text-[12px]">
        <span className="font-medium">{t("sc.sl.budget")}</span>
        <span className={`num ${over ? "text-danger" : "text-ink-2"}`}>
          {t("sc.framesOfMax", { frames, max: MAX_FRAMES })}
        </span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
        <div className={`h-full rounded-full ${over ? "bg-danger" : "bg-signal"}`} style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

function SlideRow({
  index,
  count,
  slide,
  still,
  onSeconds,
  onFit,
  onMove,
  onRemove,
}: {
  index: number;
  count: number;
  slide: Slide;
  still: Uint16Array | null;
  onSeconds: (s: number) => void;
  onFit: (f: SlideFit) => void;
  onMove: (d: -1 | 1) => void;
  onRemove: () => void;
}) {
  const t = useT();
  const icon = "flex h-7 w-7 items-center justify-center rounded-md text-ink-3 hover:bg-surface hover:text-ink disabled:opacity-30";
  return (
    <li data-slide={index} className="flex gap-3 rounded-lg border border-line p-2">
      <div className="relative h-[54px] w-[96px] shrink-0 overflow-hidden rounded-md bg-black">
        {still && <FramePlayer frames={[still]} delaysMs={[100]} className="h-full w-full" />}
        <span className="num absolute top-1 left-1 rounded bg-black/70 px-1 text-[11px] text-white">{index + 1}</span>
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex items-center gap-2">
          <span className="truncate text-[13px] font-medium" title={slide.name}>
            {slide.name}
          </span>
          {slide.kind === "library" && (
            <span className="shrink-0 rounded-full bg-surface-2 px-1.5 py-px text-[11px] text-ink-3">{t("sc.library")}</span>
          )}
          <span className="ml-auto flex shrink-0">
            <button type="button" className={icon} disabled={index === 0} onClick={() => onMove(-1)} aria-label={t("sc.sl.up")} title={t("sc.sl.up")}>
              <ArrowUp size={13} />
            </button>
            <button
              type="button"
              className={icon}
              disabled={index === count - 1}
              onClick={() => onMove(1)}
              aria-label={t("sc.sl.down")}
              title={t("sc.sl.down")}
            >
              <ArrowDown size={13} />
            </button>
            <button
              type="button"
              className={`${icon} hover:text-danger`}
              onClick={onRemove}
              aria-label={t("sc.sl.remove")}
              title={t("sc.sl.remove")}
            >
              <Trash2 size={13} />
            </button>
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <label className="flex min-w-[150px] flex-1 items-center gap-2">
            <span className="text-[12px] text-ink-3">{t("sc.sl.duration")}</span>
            <input
              type="range"
              className="range flex-1"
              min={SLIDE_MIN_S}
              max={SLIDE_MAX_S}
              step={1}
              value={slide.seconds}
              aria-label={`${t("sc.sl.duration")} ${index + 1}`}
              onChange={(e) => onSeconds(Number(e.target.value))}
              style={
                {
                  "--pct": `${((slide.seconds - SLIDE_MIN_S) / (SLIDE_MAX_S - SLIDE_MIN_S)) * 100}%`,
                  "--fill": "var(--signal)",
                } as CSSProperties
              }
            />
            <span className="num w-12 text-right text-[12px] whitespace-nowrap text-ink-2">
              {slide.seconds} {t("sc.sl.s")}
            </span>
          </label>
          {slide.kind === "file" && (
            <Segmented<SlideFit>
              label={t("sc.fit")}
              value={slide.fit}
              onChange={onFit}
              options={[
                { value: "fill", label: t("sc.fit.fill") },
                { value: "fit", label: t("sc.fit.fit") },
              ]}
            />
          )}
        </div>
      </div>
    </li>
  );
}

function LibraryPicker({
  onPick,
  onClose,
  disabled,
}: {
  onPick: (item: ScreenItem) => void;
  onClose: () => void;
  disabled: boolean;
}) {
  const t = useT();
  const itemName = useItemName();
  // The store value itself (lesson 33): a fresh [] each call loops forever.
  const items = useScreen((s) => s.state?.items) ?? NO_ITEMS;
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-line bg-surface-2/40 p-3">
      <div className="flex items-center justify-between">
        <span className="text-[12px] font-medium">{t("sc.sl.pickTitle")}</span>
        <button
          type="button"
          onClick={onClose}
          aria-label={t("sc.close")}
          className="flex h-7 w-7 items-center justify-center rounded-md text-ink-3 hover:bg-surface hover:text-ink"
        >
          <X size={14} />
        </button>
      </div>
      {items.length === 0 ? (
        <p className="text-[12px] text-ink-3">{t("sc.sl.libraryEmpty")}</p>
      ) : (
        <ul className="grid max-h-[220px] grid-cols-[repeat(auto-fill,minmax(120px,1fr))] gap-2 overflow-y-auto">
          {items.map((it) => (
            <li key={it.id}>
              <button
                type="button"
                disabled={disabled}
                onClick={() => onPick(it)}
                title={itemName(it)}
                className="flex w-full flex-col items-start gap-1 rounded-lg p-1 text-left hover:bg-surface disabled:opacity-40"
              >
                <Thumb item={it} />
                <span className="w-full truncate text-[11px] text-ink-2">{itemName(it)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
