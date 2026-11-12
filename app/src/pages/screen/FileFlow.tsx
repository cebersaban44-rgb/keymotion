// Source "Dosyadan": an image, animated image or video, framed and timed
// on the live canvas.

import { Crosshair, FileImage, Film, Image as ImageIcon, Loader2, Upload } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type WheelEvent as ReactWheelEvent,
} from "react";
import { Field, Segmented, Slider, Toggle } from "../../components/controls";
import { percent, useT } from "../../lib/i18n";
import {
  DEFAULT_FRAMING,
  H,
  MAX_FRAMES,
  PROVEN_FRAMES,
  W,
  openSource,
  outputCanvas,
  plan,
  prepare,
  renderAt,
  slack,
  toRgb565,
  toRgba,
  type Fit,
  type Framing,
  type Prepared,
  type Source,
  type Timing,
} from "../../lib/screenImport";
import { EditFooter, Hint, PreviewPanel, StudioFrame, errorKey, fmtSeconds, type Done, type Step } from "./common";

const ACCEPT = "image/*,video/mp4,video/webm,.gif,.webp,.apng,.png,.jpg,.jpeg,.bmp,.avif,.mp4,.webm,.m4v,.mov";

/** Step 1 tile: pick or drop a file. Opening (decoding) happens here so a
 *  file that cannot be read never leaves step 1. */
export function FileTile({ onOpened }: { onOpened: (s: Source) => void }) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const [opening, setOpening] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const open = async (file: File) => {
    setError(null);
    setOpening(0);
    try {
      const s = await openSource(file, (p) => setOpening(p));
      setOpening(null);
      onOpened(s);
    } catch (e) {
      console.error(e);
      setOpening(null);
      setError(t(errorKey(e)));
    }
  };

  return (
    <div
      data-tile="file"
      className={`flex min-h-[260px] flex-col items-center justify-center gap-4 rounded-lg border-2 border-dashed p-6 text-center transition-colors ${
        over ? "border-signal bg-signal/5" : "border-line"
      }`}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const f = e.dataTransfer.files[0];
        if (f) void open(f);
      }}
    >
      <div className="flex gap-3 text-ink-3">
        <ImageIcon size={24} />
        <FileImage size={24} />
        <Film size={24} />
      </div>
      <div>
        <h3 className="font-display text-[19px] font-semibold">{t("sc.src.file")}</h3>
        <p className="mt-1 max-w-[40ch] text-[13px] text-ink-2">{t("sc.src.fileBody")}</p>
      </div>
      <button
        type="button"
        disabled={opening !== null}
        onClick={() => input.current?.click()}
        className="flex items-center gap-2 rounded-lg bg-signal px-4 py-2 text-[13px] font-semibold text-signal-ink disabled:opacity-50"
      >
        {opening !== null ? <Loader2 size={15} className="animate-spin" /> : <Upload size={15} />}
        {opening !== null ? t("sc.drop.opening", { percent: percent(Math.round(opening * 100)) }) : t("sc.drop.choose")}
      </button>
      <input
        ref={input}
        type="file"
        hidden
        accept={ACCEPT}
        onChange={(e) => {
          const f = e.target.files?.[0];
          e.target.value = "";
          if (f) void open(f);
        }}
      />
      {error && <Hint tone="error">{error}</Hint>}
      <p className="num text-[11px] text-ink-3">{t("sc.drop.formats")}</p>
    </div>
  );
}

const FPS_VIDEO = [5, 8, 10, 12, 15, 20, 25];
const FPS_ANIMATED = [0, 5, 10, 15, 20, 25];

/** Steps 2–4 for a file. */
export function FileFlow({
  source,
  usb,
  loaded,
  onCancel,
  onFinished,
}: {
  source: Source;
  usb: boolean;
  loaded: boolean;
  onCancel: () => void;
  onFinished: (d: Done) => void;
}) {
  const t = useT();
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [busy, setBusy] = useState(false);
  const [name, setName] = useState(source.name);
  const step: Step = busy ? 4 : prepared ? 3 : 2;

  const kind =
    source.kind === "image"
      ? t("sc.kind.image")
      : `${source.kind === "video" ? t("sc.kind.video") : t("sc.kind.animated")} · ${fmtSeconds(source.durationMs / 1000)}${
          source.kind === "animated" ? ` · ${t("sc.frames", { count: source.frameStarts.length })}` : ""
        }`;

  return (
    <StudioFrame step={step} locked={busy} onCancel={onCancel} title={source.name} detail={`${source.width}×${source.height} · ${kind}`}>
      <div className={prepared ? "hidden" : "flex flex-col gap-5"}>
        <FileEditor source={source} onPrepared={setPrepared} />
      </div>
      {prepared && (
        <PreviewPanel
          prepared={prepared}
          name={name}
          onName={setName}
          fallbackName={source.name}
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

function FileEditor({ source, onPrepared }: { source: Source; onPrepared: (p: Prepared) => void }) {
  const t = useT();
  const still = source.kind === "image";
  const [framing, setFraming] = useState<Framing>(DEFAULT_FRAMING);
  const [timing, setTiming] = useState<Timing>(() => {
    // Animated images keep their own frames when they fit; longer ones and
    // videos are sampled at 10 fps (14 s fit in 140 frames).
    const own = source.kind === "animated" && source.frameStarts.length <= MAX_FRAMES;
    const fps = own ? 0 : 10;
    const maxMs = fps ? (MAX_FRAMES / fps) * 1000 : source.durationMs;
    return { startMs: 0, endMs: Math.min(source.durationMs, maxMs), fps };
  });
  const [dither, setDither] = useState(source.kind !== "image");
  const [cursor, setCursor] = useState(0);
  const [preparing, setPreparing] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const abort = useRef<AbortController | null>(null);

  const p = useMemo(() => plan(source, timing), [source, timing]);
  const loopMs = p.frames.reduce((a, f) => a + Math.max(2, Math.round(f.delayMs / 2) * 2), 0);

  // A change while preparing stops the run.
  useEffect(() => {
    abort.current?.abort();
    setPreparing(null);
    setError(null);
  }, [framing, timing, dither]);

  // Keep the scrub cursor inside the range.
  useEffect(() => {
    setCursor((c) => Math.min(Math.max(c, timing.startMs), Math.max(timing.startMs, timing.endMs - 1)));
  }, [timing.startMs, timing.endMs]);

  // Stop preparing when the flow closes.
  useEffect(() => () => abort.current?.abort(), []);

  const runPreview = async () => {
    abort.current?.abort();
    const ctrl = new AbortController();
    abort.current = ctrl;
    setError(null);
    setPreparing({ done: 0, total: p.frames.length });
    try {
      const r = await prepare(source, p, framing, dither, source.name, (done, total) => setPreparing({ done, total }), ctrl.signal);
      if (!ctrl.signal.aborted) onPrepared(r);
    } catch (e) {
      if ((e as Error).name !== "AbortError") setError(t(errorKey(e)));
    } finally {
      if (abort.current === ctrl) setPreparing(null);
    }
  };

  return (
    <>
      <div className="grid grid-cols-1 gap-6 @min-[860px]:grid-cols-[480px_minmax(280px,1fr)]">
        <div className="flex flex-col gap-2">
          <CropCanvas source={source} framing={framing} onFraming={setFraming} atMs={cursor} dither={dither} />
          <p className="text-[12px] text-ink-3">{t("sc.preview.editNote")}</p>
          {!still && (
            <Slider
              label={t("sc.cursor")}
              value={cursor}
              min={timing.startMs}
              max={Math.max(timing.startMs + 1, timing.endMs - 1)}
              step={10}
              onChange={setCursor}
              format={(v) => fmtSeconds(v / 1000)}
            />
          )}
        </div>

        <div className="flex flex-col gap-4">
          <Field label={t("sc.fit")}>
            <Segmented<Fit>
              label={t("sc.fit")}
              value={framing.fit}
              onChange={(fit) => setFraming({ ...framing, fit, panX: 0, panY: 0 })}
              options={[
                { value: "fill", label: t("sc.fit.fill") },
                { value: "fit", label: t("sc.fit.fit") },
                { value: "stretch", label: t("sc.fit.stretch") },
              ]}
            />
          </Field>
          <div className="flex flex-col gap-1.5">
            <Slider
              label={t("sc.zoom")}
              value={framing.zoom}
              min={1}
              max={4}
              step={0.05}
              disabled={framing.fit === "stretch"}
              onChange={(zoom) => setFraming({ ...framing, zoom })}
              format={(v) => `${v.toFixed(2)}×`}
            />
            <button
              type="button"
              onClick={() => setFraming({ ...framing, zoom: 1, panX: 0, panY: 0 })}
              className="flex items-center gap-1.5 self-start text-[12px] text-ink-3 hover:text-ink"
            >
              <Crosshair size={12} /> {t("sc.center")}
            </button>
          </div>

          {!still && (
            <>
              <Field
                label={t("sc.fps")}
                hint={timing.fps ? `${Math.round(1000 / timing.fps)} ms / ${t("sc.frame")}` : t("sc.fps.sourceHint")}
              >
                <Segmented<string>
                  label={t("sc.fps")}
                  value={String(timing.fps)}
                  onChange={(v) => {
                    const fps = Number(v);
                    const maxMs = fps ? (MAX_FRAMES / fps) * 1000 : source.durationMs;
                    setTiming({ ...timing, fps, endMs: Math.min(source.durationMs, timing.startMs + maxMs) });
                  }}
                  options={(source.kind === "video" ? FPS_VIDEO : FPS_ANIMATED).map((f) => ({
                    value: String(f),
                    label: f === 0 ? t("sc.fps.source") : String(f),
                  }))}
                />
              </Field>
              <Slider
                label={t("sc.start")}
                value={timing.startMs}
                min={0}
                max={Math.max(1, source.durationMs - 10)}
                step={10}
                onChange={(startMs) => setTiming({ ...timing, startMs, endMs: Math.max(timing.endMs, startMs + 10) })}
                format={(v) => fmtSeconds(v / 1000)}
              />
              <Slider
                label={t("sc.end")}
                value={timing.endMs}
                min={10}
                max={source.durationMs}
                step={10}
                onChange={(endMs) => setTiming({ ...timing, endMs, startMs: Math.min(timing.startMs, endMs - 10) })}
                format={(v) => fmtSeconds(v / 1000)}
              />
            </>
          )}

          <div className="flex items-center justify-between gap-3">
            <span className="flex flex-col">
              <span className="text-[13px] font-medium">{t("sc.dither")}</span>
              <span className="text-[12px] text-ink-3">{t("sc.dither.hint")}</span>
            </span>
            <Toggle checked={dither} onChange={setDither} label={t("sc.dither")} />
          </div>
        </div>
      </div>

      <EditFooter frames={p.frames.length} loopMs={loopMs} preparing={preparing} onPreview={runPreview} />

      {p.clipped && <Hint tone="warn">{t("sc.warn.clipped")}</Hint>}
      {p.frames.length > PROVEN_FRAMES && <Hint tone="note">{t("sc.warn.unproven")}</Hint>}
      {error && <Hint tone="error">{error}</Hint>}
    </>
  );
}

/** The live framing editor: drag to move, wheel to zoom. Shows the exact
 *  panel colours (RGB565, with the chosen dithering) of one frame. */
function CropCanvas({
  source,
  framing,
  onFraming,
  atMs,
  dither,
}: {
  source: Source;
  framing: Framing;
  onFraming: (f: Framing) => void;
  atMs: number;
  dither: boolean;
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  const work = useRef(outputCanvas());
  const busy = useRef(false);
  const pending = useRef(false);
  const latest = useRef({ framing, atMs, dither });
  latest.current = { framing, atMs, dither };

  const render = useCallback(async () => {
    if (busy.current) {
      pending.current = true;
      return;
    }
    busy.current = true;
    try {
      do {
        pending.current = false;
        const { framing: f, atMs: at, dither: d } = latest.current;
        await renderAt(source, work.current.ctx, at, f);
        const px = toRgb565(work.current.ctx.getImageData(0, 0, W, H).data, d);
        const ctx = ref.current?.getContext("2d");
        if (ctx) ctx.putImageData(new ImageData(toRgba(px), W, H), 0, 0);
      } while (pending.current);
    } catch (e) {
      console.error(e);
    } finally {
      busy.current = false;
    }
  }, [source]);

  useEffect(() => {
    void render();
  }, [render, framing, atMs, dither]);

  const drag = useRef<{ x: number; y: number; f: Framing } | null>(null);
  const scale = 2; // CSS px per output px
  const onDown = (e: ReactPointerEvent) => {
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = { x: e.clientX, y: e.clientY, f: framing };
  };
  const onMove = (e: ReactPointerEvent) => {
    const d = drag.current;
    if (!d || d.f.fit === "stretch") return;
    const room = slack(source.width, source.height, d.f);
    const dx = (e.clientX - d.x) / scale;
    const dy = (e.clientY - d.y) / scale;
    const clamp = (v: number) => Math.max(-1, Math.min(1, v));
    onFraming({
      ...d.f,
      panX: room.x > 0.5 ? clamp(d.f.panX + dx / (room.x / 2)) : 0,
      panY: room.y > 0.5 ? clamp(d.f.panY + dy / (room.y / 2)) : 0,
    });
  };
  const onWheel = (e: ReactWheelEvent) => {
    if (framing.fit === "stretch") return;
    const zoom = Math.max(1, Math.min(4, framing.zoom * Math.pow(1.1, -e.deltaY / 100)));
    onFraming({ ...framing, zoom });
  };

  return (
    <canvas
      ref={ref}
      width={W}
      height={H}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={() => (drag.current = null)}
      onWheel={onWheel}
      className={`pixelated h-[270px] w-[480px] touch-none rounded-lg bg-black ${
        framing.fit === "stretch" ? "" : "cursor-grab active:cursor-grabbing"
      }`}
    />
  );
}
