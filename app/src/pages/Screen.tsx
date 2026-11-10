import {
  AlertTriangle,
  Check,
  Crosshair,
  FileImage,
  Film,
  Image as ImageIcon,
  Info,
  Library,
  Loader2,
  Pencil,
  Play,
  Trash2,
  Upload,
  Usb,
  X,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type WheelEvent as ReactWheelEvent,
} from "react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { ScreenItem } from "../bindings/ScreenItem";
import { Field, Segmented, Slider, Toggle } from "../components/controls";
import { useT, type MessageKey } from "../lib/i18n";
import { useApp } from "../lib/store";
import { itemData, useScreen } from "../lib/screen";
import {
  DEFAULT_FRAMING,
  H,
  ImportError,
  MAX_FRAMES,
  PROVEN_FRAMES,
  W,
  fromBlob,
  openSource,
  outputCanvas,
  plan,
  pagesFor,
  prepare,
  renderAt,
  slack,
  toRgb565,
  toRgba,
  uploadSeconds,
  type Fit,
  type Framing,
  type Prepared,
  type Source,
  type Timing,
} from "../lib/screenImport";

// ---------------------------------------------------------------- helpers

function fmtSeconds(s: number) {
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)} sn`;
  const m = Math.floor(s / 60);
  return `${m} dk ${Math.round(s - m * 60)} sn`;
}

/** Frames cache for library previews (data never changes for an id). */
const framesCache = new Map<string, Promise<{ frames: Uint16Array[]; delaysMs: number[] }>>();
function loadFrames(id: string, first?: boolean) {
  const key = first ? `${id}:1` : id;
  let p = framesCache.get(key);
  if (!p) {
    p = itemData(id, first ? 1 : undefined).then(fromBlob);
    framesCache.set(key, p);
    p.catch(() => framesCache.delete(key));
  }
  return p;
}

/** Plays RGB565 frames on a 240×135 canvas, scaled with sharp pixels. */
function FramePlayer({
  frames,
  delaysMs,
  className = "",
  playing = true,
}: {
  frames: Uint16Array[];
  delaysMs: number[];
  className?: string;
  playing?: boolean;
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const ctx = ref.current?.getContext("2d");
    if (!ctx || frames.length === 0) return;
    const img = new ImageData(W, H);
    let i = 0;
    let timer: number | undefined;
    const draw = () => {
      toRgba(frames[i]!, img.data);
      ctx.putImageData(img, 0, 0);
      if (!playing || frames.length < 2) return;
      timer = window.setTimeout(
        () => {
          i = (i + 1) % frames.length;
          draw();
        },
        Math.max(16, delaysMs[i] ?? 100),
      );
    };
    draw();
    return () => window.clearTimeout(timer);
  }, [frames, delaysMs, playing]);
  return <canvas ref={ref} width={W} height={H} className={`pixelated ${className}`} />;
}

// -------------------------------------------------------------- the page

export function Screen({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const usb = kb.connection === "usb" && kb.hasCommandChannel && kb.features.includes("screen");
  const state = useScreen((s) => s.state);
  const modelId = useScreen((s) => s.modelId);
  const loaded = state !== null && modelId === kb.modelId;

  useEffect(() => {
    if (!loaded) useScreen.getState().load(kb.modelId).catch(console.error);
  }, [kb.modelId, loaded]);

  return (
    <div className="flex flex-col gap-6">
      <header className="flex items-end justify-between gap-6">
        <div>
          <div className="eyebrow mb-2">{t("sc.eyebrow")}</div>
          <h1 className="font-display text-[40px] leading-none font-semibold tracking-[-0.035em]">{t("sc.title")}</h1>
        </div>
        {loaded && (
          <div
            className="num flex flex-col items-end gap-0.5 text-[11.5px] text-ink-3"
            title={t("sc.wear.hint")}
          >
            <span>
              {t("sc.uploads.today")}: <span className="text-ink-2">{state.uploadsToday}</span> ·{" "}
              {t("sc.uploads.total")}: <span className="text-ink-2">{state.uploadsTotal}</span>
            </span>
            <span>{t("sc.wear.short")}</span>
          </div>
        )}
      </header>

      {!usb && (
        <div className="flex items-center gap-3 rounded-2xl border border-signal/30 bg-signal/10 px-4 py-3 text-[13px]">
          <Usb size={16} className="text-signal" />
          {t("sc.needsUsb")}
        </div>
      )}

      <KeyboardScreens />

      <Studio usb={usb} loaded={loaded} />
      <LibraryCard usb={usb} />
    </div>
  );
}

// ------------------------------------------------------ keyboard screens

function pad(n: number) {
  return String(n).padStart(2, "0");
}

function useNow() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(id);
  }, []);
  return now;
}

/** The firmware's home page (clock), as on the owner's keyboard photo. */
function ClockPage() {
  const now = useNow();
  const mono = "JetBrains Mono Variable, monospace";
  const chips = [
    { l: "WIN", c: "#5fe3ff", on: true },
    { l: "USB", c: "#8a6bff", on: true },
    { l: "2.4", c: "#7ee08a", on: false },
    { l: "BT1", c: "#3aa0ff", on: false },
    { l: "lock", c: "#e8d24a", on: false },
    { l: "A", c: "#e8d24a", on: false },
    { l: "1", c: "#e8d24a", on: false },
  ];
  return (
    <svg viewBox="0 0 240 135" className="block h-full w-full" role="img" aria-label="clock page">
      <defs>
        <linearGradient id="sc-home" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#0d2a5a" />
          <stop offset="1" stopColor="#071a3c" />
        </linearGradient>
      </defs>
      <rect width="240" height="135" fill="url(#sc-home)" />
      <text x="10" y="20" fill="#5fd4ff" fontFamily={mono} fontSize="11">
        {`${now.getFullYear()}/${pad(now.getMonth() + 1)}/${pad(now.getDate())}`}
      </text>
      <path d="M190 10 l-4 7 h3 l-2 6 l5 -8 h-3 l2 -5 z" fill="#c6f04a" />
      <rect x="199" y="10" width="26" height="12" rx="2" fill="none" stroke="#7ee08a" strokeWidth="1.4" />
      <rect x="225.5" y="13.5" width="2" height="5" fill="#7ee08a" />
      {[0, 1, 2, 3, 4].map((i) => (
        <rect key={i} x={201.5 + i * 4.6} y="12.5" width="3.4" height="7" fill="#7ee08a" />
      ))}
      <text
        x="120"
        y="76"
        textAnchor="middle"
        fill="#f4f8ff"
        fontFamily="'Times New Roman', Times, serif"
        fontSize="34"
      >
        {`${pad(now.getHours())}: ${pad(now.getMinutes())}: ${pad(now.getSeconds())}`}
      </text>
      {chips.map((c, i) => (
        <g key={c.l} transform={`translate(${9 + i * 32} 104)`}>
          <rect width="26" height="22" rx="4" fill={c.on ? c.c : "none"} fillOpacity={0.85} stroke={c.c} strokeWidth="1.5" />
          {c.l === "lock" ? (
            <g fill="none" stroke={c.c} strokeWidth="1.6">
              <path d="M9.5 10.5 V7.8 a3.5 3.5 0 0 1 7 0 V10.5" />
              <rect x="7.5" y="10.5" width="11" height="7.5" rx="1.2" />
            </g>
          ) : (
            <text
              x="13"
              y="15.5"
              textAnchor="middle"
              fill={c.on ? "#071426" : c.c}
              fontFamily={mono}
              fontSize={c.l.length > 2 ? 8 : 12}
              fontWeight="700"
            >
              {c.l}
            </text>
          )}
        </g>
      ))}
    </svg>
  );
}

function Panel({ label, sub, children }: { label: string; sub: string; children: ReactNode }) {
  return (
    <figure className="flex flex-col gap-2">
      <div className="relative aspect-[240/135] w-full overflow-hidden rounded-xl bg-black shadow-[0_0_0_6px_#0b0c0e,0_0_0_7px_var(--line)]">
        {children}
      </div>
      <figcaption className="flex items-baseline justify-between gap-2 px-1">
        <span className="text-[12.5px] font-medium">{label}</span>
        <span className="text-[11px] text-ink-3">{sub}</span>
      </figcaption>
    </figure>
  );
}

function KeyboardScreens() {
  const t = useT();
  const state = useScreen((s) => s.state);
  const shownId = state?.shown ?? null;
  const factory = state?.items.find((i) => i.kind === "factory") ?? null;
  const shown = state?.items.find((i) => i.id === shownId) ?? null;
  const item = shown ?? factory;
  const [frames, setFrames] = useState<{ frames: Uint16Array[]; delaysMs: number[] } | null>(null);

  useEffect(() => {
    setFrames(null);
    if (!item) return;
    let live = true;
    loadFrames(item.id).then((f) => live && setFrames(f), console.error);
    return () => {
      live = false;
    };
  }, [item?.id]);

  return (
    <section className="card grid grid-cols-[1fr_1fr_minmax(220px,0.8fr)] items-start gap-6 p-5">
      <Panel label={t("sc.page.home")} sub={t("sc.page.home.sub")}>
        <ClockPage />
      </Panel>
      <Panel
        label={t("sc.page.gif")}
        sub={shown ? shown.name : factory ? t("sc.page.gif.maybeFactory") : t("sc.page.gif.unknown")}
      >
        {frames ? (
          <FramePlayer frames={frames.frames} delaysMs={frames.delaysMs} className={`h-full w-full ${shown ? "" : "opacity-60"}`} />
        ) : (
          <div className="flex h-full items-center justify-center text-[12px] text-ink-3">
            {item ? <Loader2 size={16} className="animate-spin" /> : t("sc.page.gif.unknown")}
          </div>
        )}
      </Panel>
      <div className="flex flex-col gap-3 text-[12.5px] text-ink-2">
        <p className="flex gap-2">
          <Info size={14} className="mt-0.5 shrink-0 text-signal" />
          <span>{t("sc.pages.note")}</span>
        </p>
        <p className="flex gap-2">
          <Check size={14} className="mt-0.5 shrink-0 text-ok" />
          <span>{t("sc.pages.safe")}</span>
        </p>
      </div>
    </section>
  );
}

// ------------------------------------------------------------- the studio
//
// One flow, four steps (owner's review, 2026-11-10):
//   1 File     pick or drop a file
//   2 Edit     framing and timing on the live canvas      → "Önizle"
//   3 Preview  the exact bytes play; confirm               → "Klavyeye yükle"
//   4 Upload   progress in place; on success the studio returns to step 1
//              and says what was done.
// "Vazgeç" leaves the file at any step before the upload; nothing is kept.

type Phase = { kind: "idle" } | { kind: "opening"; progress: number } | { kind: "error"; message: string };

/** What the last finished flow did, shown above the file picker. */
type Done = { kind: "uploaded" | "saved"; name: string; seconds?: number };

function errorKey(e: unknown): MessageKey {
  if (e instanceof ImportError) {
    const map: Record<ImportError["code"], MessageKey> = {
      "too-large": "sc.err.tooLarge",
      unsupported: "sc.err.unsupported",
      decode: "sc.err.decode",
      "too-many-frames": "sc.err.tooManyFrames",
      dimensions: "sc.err.dimensions",
      timeout: "sc.err.timeout",
    };
    return map[e.code];
  }
  return "sc.err.decode";
}

function Studio({ usb, loaded }: { usb: boolean; loaded: boolean }) {
  const t = useT();
  const [source, setSource] = useState<Source | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [done, setDone] = useState<Done | null>(null);

  useEffect(() => () => source?.dispose(), [source]);

  const open = async (file: File) => {
    setDone(null);
    setPhase({ kind: "opening", progress: 0 });
    try {
      const s = await openSource(file, (p) => setPhase({ kind: "opening", progress: p }));
      setSource(s);
      setPhase({ kind: "idle" });
    } catch (e) {
      console.error(e);
      setPhase({ kind: "error", message: t(errorKey(e)) });
    }
  };

  if (!source) return <DropZone onFile={open} phase={phase} done={done} onDismiss={() => setDone(null)} />;
  return (
    <Editor
      key={source.name + source.width + source.durationMs}
      source={source}
      usb={usb}
      loaded={loaded}
      onCancel={() => setSource(null)}
      onFinished={(d) => {
        setSource(null);
        setDone(d);
      }}
    />
  );
}

type Step = 1 | 2 | 3 | 4;

function Stepper({ step }: { step: Step }) {
  const t = useT();
  const steps: MessageKey[] = ["sc.step.file", "sc.step.edit", "sc.step.preview", "sc.step.upload"];
  return (
    <ol className="flex items-center gap-2" aria-label={t("sc.title")}>
      {steps.map((k, i) => {
        const n = (i + 1) as Step;
        const state = n < step ? "done" : n === step ? "now" : "next";
        return (
          <li key={k} className="flex items-center gap-2" aria-current={state === "now" ? "step" : undefined}>
            {i > 0 && <span className={`h-px w-6 ${n <= step ? "bg-signal" : "bg-line-strong"}`} />}
            <span
              className={`num flex h-6 w-6 items-center justify-center rounded-full text-[11px] font-semibold ${
                state === "done"
                  ? "bg-signal/20 text-signal"
                  : state === "now"
                    ? "bg-signal text-signal-ink"
                    : "border border-line-strong text-ink-3"
              }`}
            >
              {state === "done" ? <Check size={12} /> : n}
            </span>
            <span className={`text-[12.5px] ${state === "now" ? "font-semibold text-ink" : "text-ink-3"}`}>{t(k)}</span>
          </li>
        );
      })}
    </ol>
  );
}

function DropZone({
  onFile,
  phase,
  done,
  onDismiss,
}: {
  onFile: (f: File) => void;
  phase: Phase;
  done: Done | null;
  onDismiss: () => void;
}) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  return (
    <section className="card flex flex-col gap-4 p-5">
      <Stepper step={1} />
      <AnimatePresence>
        {done && (
          <motion.div
            initial={{ opacity: 0, y: -6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            role="status"
            className="flex items-start gap-3 rounded-2xl border border-ok/30 bg-ok/10 px-4 py-3"
          >
            <Check size={18} className="mt-0.5 shrink-0 text-ok" />
            <div className="flex-1 text-[13px]">
              <div className="font-semibold">
                “{done.name}” {done.kind === "uploaded" ? t("sc.done.uploaded") : t("sc.done.saved")}
                {done.seconds != null && <span className="num ml-2 font-normal text-ink-3">{fmtSeconds(done.seconds)}</span>}
              </div>
              <div className="text-ink-2">{done.kind === "uploaded" ? t("sc.done.uploadedHint") : t("sc.done.savedHint")}</div>
            </div>
            <button
              type="button"
              onClick={onDismiss}
              aria-label={t("sc.close")}
              className="flex h-7 w-7 items-center justify-center rounded-md text-ink-3 hover:bg-surface hover:text-ink"
            >
              <X size={14} />
            </button>
          </motion.div>
        )}
      </AnimatePresence>
      <div
        className={`flex min-h-[300px] flex-col items-center justify-center gap-4 rounded-2xl border-2 border-dashed p-10 text-center transition-colors ${
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
          if (f) onFile(f);
        }}
      >
        <div className="flex gap-3 text-ink-3">
          <ImageIcon size={26} />
          <FileImage size={26} />
          <Film size={26} />
        </div>
        <div>
          <h2 className="font-display text-[22px] font-semibold tracking-[-0.02em]">
            {done ? t("sc.drop.titleNext") : t("sc.drop.title")}
          </h2>
          <p className="mt-1 max-w-[52ch] text-[13px] text-ink-2">{t("sc.drop.body")}</p>
        </div>
        <button
          type="button"
          disabled={phase.kind === "opening"}
          onClick={() => input.current?.click()}
          className="flex items-center gap-2 rounded-xl bg-signal px-4 py-2 text-[13px] font-semibold text-signal-ink disabled:opacity-50"
        >
          {phase.kind === "opening" ? <Loader2 size={15} className="animate-spin" /> : <Upload size={15} />}
          {phase.kind === "opening"
            ? `${t("sc.drop.opening")} ${Math.round(phase.progress * 100)}%`
            : t("sc.drop.choose")}
        </button>
        <input
          ref={input}
          type="file"
          hidden
          accept="image/*,video/mp4,video/webm,.gif,.webp,.apng,.png,.jpg,.jpeg,.bmp,.avif,.mp4,.webm,.m4v,.mov"
          onChange={(e) => {
            const f = e.target.files?.[0];
            e.target.value = "";
            if (f) onFile(f);
          }}
        />
        {phase.kind === "error" && (
          <p className="flex items-center gap-2 text-[12.5px] text-danger">
            <AlertTriangle size={14} /> {phase.message}
          </p>
        )}
        <p className="num text-[11px] text-ink-3">{t("sc.drop.formats")}</p>
      </div>
    </section>
  );
}

const FPS_VIDEO = [5, 8, 10, 12, 15, 20, 25];
const FPS_ANIMATED = [0, 5, 10, 15, 20, 25];

function Editor({
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
  const still = source.kind === "image";
  const upload = useScreen((s) => s.upload);
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
  const [name, setName] = useState(source.name);
  const [cursor, setCursor] = useState(0);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [preparing, setPreparing] = useState<{ done: number; total: number } | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const abort = useRef<AbortController | null>(null);

  const p = useMemo(() => plan(source, timing), [source, timing]);
  const loopMs = p.frames.reduce((a, f) => a + Math.max(2, Math.round(f.delayMs / 2) * 2), 0);
  const step: Step = uploading ? 4 : prepared ? 3 : 2;
  const finalName = name.trim() || source.name;

  // Any change to the picture drops a prepared result (back to editing).
  useEffect(() => {
    abort.current?.abort();
    setPrepared(null);
    setPreparing(null);
    setError(null);
  }, [framing, timing, dither]);

  // Keep the scrub cursor inside the range.
  useEffect(() => {
    setCursor((c) => Math.min(Math.max(c, timing.startMs), Math.max(timing.startMs, timing.endMs - 1)));
  }, [timing.startMs, timing.endMs]);

  // Stop preparing when the editor closes.
  useEffect(() => () => abort.current?.abort(), []);

  const runPreview = async () => {
    abort.current?.abort();
    const ctrl = new AbortController();
    abort.current = ctrl;
    setError(null);
    setPreparing({ done: 0, total: p.frames.length });
    try {
      const r = await prepare(source, p, framing, dither, finalName, (done, total) => setPreparing({ done, total }), ctrl.signal);
      if (!ctrl.signal.aborted) setPrepared(r);
    } catch (e) {
      if ((e as Error).name !== "AbortError") setError(t(errorKey(e)));
    } finally {
      if (abort.current === ctrl) setPreparing(null);
    }
  };

  const store = async () => useScreen.getState().add({ ...prepared!, name: finalName });

  const saveOnly = async () => {
    try {
      await store();
      onFinished({ kind: "saved", name: finalName });
    } catch (e) {
      setError(String(e));
    }
  };

  const runUpload = async () => {
    setConfirm(false);
    setError(null);
    setUploading(true);
    const started = performance.now();
    try {
      const item = await store();
      await useScreen.getState().uploadToKeyboard(item, "editor");
      onFinished({ kind: "uploaded", name: finalName, seconds: (performance.now() - started) / 1000 });
    } catch (e) {
      setError(`${t("sc.uploadFailed")}: ${String(e)}`);
      setUploading(false);
    }
  };

  const sourceInfo =
    source.kind === "image"
      ? t("sc.kind.image")
      : `${source.kind === "video" ? t("sc.kind.video") : t("sc.kind.animated")} · ${fmtSeconds(source.durationMs / 1000)}${
          source.kind === "animated" ? ` · ${source.frameStarts.length} ${t("sc.frames")}` : ""
        }`;

  const stats = (
    <div className="num flex flex-wrap gap-x-4 gap-y-1 text-[12px] text-ink-2">
      <span>
        {p.frames.length} {t("sc.frames")}
      </span>
      {!still && (
        <span>
          {t("sc.loop")}: {fmtSeconds(loopMs / 1000)}
        </span>
      )}
      <span>
        {pagesFor(p.frames.length)} {t("sc.pages")}
      </span>
      <span>
        {t("sc.uploadTime")}: ~{fmtSeconds(uploadSeconds(p.frames.length))}
      </span>
    </div>
  );

  return (
    <section className="card flex flex-col gap-5 p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Stepper step={step} />
        <button
          type="button"
          onClick={onCancel}
          disabled={uploading}
          className="flex h-8 items-center gap-1.5 rounded-lg border border-line px-3 text-[12.5px] font-medium text-ink-2 hover:bg-surface-2 hover:text-ink disabled:opacity-40"
        >
          <X size={14} /> {t("sc.cancel")}
        </button>
      </div>

      <div className="flex items-baseline gap-3">
        <span className="truncate text-[14px] font-semibold">{source.name}</span>
        <span className="num truncate text-[11.5px] text-ink-3">
          {source.width}×{source.height} · {sourceInfo}
        </span>
      </div>

      <div className="grid grid-cols-[480px_minmax(280px,1fr)] gap-6">
        <div className="flex flex-col gap-2">
          {prepared ? (
            <div className="relative">
              <FramePlayer
                frames={prepared.frames}
                delaysMs={prepared.delaysMs}
                className="h-[270px] w-[480px] rounded-lg bg-black"
              />
              <span className="absolute top-2 left-2 flex items-center gap-1 rounded-full bg-black/70 px-2 py-0.5 text-[11px] text-white">
                <Play size={10} /> {t("sc.preview.result")}
              </span>
            </div>
          ) : (
            <CropCanvas source={source} framing={framing} onFraming={setFraming} atMs={cursor} dither={dither} />
          )}
          <p className="text-[11.5px] text-ink-3">{prepared ? t("sc.preview.resultNote") : t("sc.preview.editNote")}</p>
          {!still && !prepared && (
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

        {step === 2 ? (
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
                <span className="text-[12.5px] font-medium">{t("sc.dither")}</span>
                <span className="text-[11.5px] text-ink-3">{t("sc.dither.hint")}</span>
              </span>
              <Toggle checked={dither} onChange={setDither} label={t("sc.dither")} />
            </div>
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <div>
              <div className="text-[15px] font-semibold">{step === 4 ? t("sc.uploading") : t("sc.preview.ready")}</div>
              <p className="mt-1 text-[12.5px] text-ink-2">
                {step === 4 ? t("sc.uploading.note") : t("sc.preview.check")}
              </p>
            </div>
            <Field label={t("sc.name")}>
              <input
                value={name}
                maxLength={60}
                disabled={uploading}
                onChange={(e) => setName(e.target.value)}
                className="h-9 rounded-lg border border-line bg-bg-raise px-3 text-[13px] outline-none focus:border-signal disabled:opacity-60"
              />
            </Field>
            {stats}
            {step === 4 ? (
              <InlineProgress />
            ) : (
              <ul className="flex flex-col gap-1.5 text-[12px] text-ink-2">
                <li className="flex gap-2">
                  <Check size={13} className="mt-0.5 shrink-0 text-ok" /> {t("sc.confirm.onlyGif")}
                </li>
                <li className="flex gap-2">
                  <Library size={13} className="mt-0.5 shrink-0 text-ink-3" /> {t("sc.preview.libraryNote")}
                </li>
              </ul>
            )}
          </div>
        )}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-4 border-t border-line pt-4">
        {step === 2 ? stats : <span />}
        <div className="flex flex-wrap items-center justify-end gap-2">
          {step === 2 && (
            <button
              type="button"
              disabled={!!preparing}
              onClick={runPreview}
              className="flex h-10 items-center gap-2 rounded-xl bg-signal px-5 text-[13px] font-semibold text-signal-ink disabled:opacity-50"
            >
              {preparing ? <Loader2 size={15} className="animate-spin" /> : <Play size={15} />}
              {preparing ? `${t("sc.preparing")} ${preparing.done}/${preparing.total}` : t("sc.prepare")}
            </button>
          )}
          {step === 3 && (
            <>
              <button
                type="button"
                onClick={saveOnly}
                title={t("sc.saveOnly.hint")}
                className="flex h-10 items-center gap-2 rounded-xl px-3 text-[12.5px] text-ink-2 hover:bg-surface-2 hover:text-ink"
              >
                <Library size={14} /> {t("sc.saveOnly")}
              </button>
              <button
                type="button"
                onClick={() => setPrepared(null)}
                className="flex h-10 items-center gap-2 rounded-xl border border-line px-4 text-[13px] font-medium hover:bg-surface-2"
              >
                <Pencil size={14} /> {t("sc.backToEdit")}
              </button>
              <button
                type="button"
                disabled={!usb || !loaded || !!upload}
                title={!usb ? t("sc.needsUsb") : undefined}
                onClick={() => setConfirm(true)}
                className="flex h-10 items-center gap-2 rounded-xl bg-signal px-5 text-[13px] font-semibold text-signal-ink disabled:opacity-40"
              >
                <Upload size={15} /> {t("sc.uploadAction")}
              </button>
            </>
          )}
        </div>
      </div>

      {step === 2 && p.clipped && (
        <p className="flex items-start gap-2 text-[12px] text-warn">
          <AlertTriangle size={13} className="mt-0.5 shrink-0" />
          {t("sc.warn.clipped")}
        </p>
      )}
      {p.frames.length > PROVEN_FRAMES && (
        <p className="flex items-start gap-2 text-[12px] text-ink-3">
          <Info size={13} className="mt-0.5 shrink-0" />
          {t("sc.warn.unproven")}
        </p>
      )}
      {step === 3 && !usb && (
        <p className="flex items-start gap-2 text-[12px] text-ink-3">
          <Usb size={13} className="mt-0.5 shrink-0" />
          {t("sc.needsUsbShort")}
        </p>
      )}
      {error && (
        <p className="flex items-start gap-2 text-[12px] text-danger">
          <X size={13} className="mt-0.5 shrink-0" /> {error}
        </p>
      )}

      <AnimatePresence>
        {confirm && prepared && (
          <ConfirmUpload
            frames={prepared.frames.length}
            loopMs={loopMs}
            preview={prepared.frames[Math.floor(prepared.frames.length / 3)]!}
            onCancel={() => setConfirm(false)}
            onConfirm={runUpload}
          />
        )}
      </AnimatePresence>
    </section>
  );
}

/** Time left ("~1 dk 20 sn") from the pages done so far; `null` until known. */
function remaining(u: { done: number; total: number; startedMs: number }, nowMs: number): string | null {
  const elapsed = Math.max(0, nowMs - u.startedMs);
  if (u.done === 0 || elapsed === 0) return null;
  const left = ((elapsed / u.done) * Math.max(0, u.total - u.done)) / 1000;
  return `~${fmtSeconds(left)}`;
}

/** Progress of the running upload, inside the studio. */
function InlineProgress() {
  const t = useT();
  const upload = useScreen((s) => s.upload);
  const now = useNow();
  if (!upload) {
    return (
      <div className="flex items-center gap-2 text-[12.5px] text-ink-2">
        <Loader2 size={14} className="animate-spin" /> {t("sc.saving")}
      </div>
    );
  }
  const pct = Math.round((upload.done / Math.max(1, upload.total)) * 100);
  const left = remaining(upload, now.getTime());
  return (
    <div className="flex flex-col gap-2" role="status">
      <div className="h-2 overflow-hidden rounded-full bg-surface-2">
        <div className="h-full rounded-full bg-signal transition-[width]" style={{ width: `${pct}%` }} />
      </div>
      <div className="num flex justify-between text-[12px] text-ink-2">
        <span>
          {upload.done}/{upload.total} {t("sc.pages")} · %{pct}
        </span>
        <span>{left ? `${left} ${t("sc.left")}` : ""}</span>
      </div>
    </div>
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

function ConfirmUpload({
  frames,
  loopMs,
  preview,
  onCancel,
  onConfirm,
}: {
  frames: number;
  loopMs: number;
  preview: Uint16Array;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const t = useT();
  const state = useScreen((s) => s.state);
  const factory = state?.items.some((i) => i.kind === "factory") ?? false;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      role="dialog"
      aria-modal
    >
      <motion.div
        initial={{ scale: 0.97, y: 8 }}
        animate={{ scale: 1, y: 0 }}
        className="card flex w-[480px] flex-col gap-4 p-6"
      >
        <h2 className="font-display text-[22px] font-semibold tracking-[-0.02em]">{t("sc.confirm.title")}</h2>
        <div className="flex gap-4">
          <FramePlayer frames={[preview]} delaysMs={[100]} className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />
          <div className="num flex flex-col gap-0.5 text-[12.5px] text-ink-2">
            <span>
              {frames} {t("sc.frames")}
              {frames > 1 ? ` · ${t("sc.loop")} ${fmtSeconds(loopMs / 1000)}` : ""}
            </span>
            <span>
              {pagesFor(frames)} {t("sc.pages")} · ~{fmtSeconds(uploadSeconds(frames))}
            </span>
            <span>
              {t("sc.confirm.todayCount")}: {(state?.uploadsToday ?? 0) + 1}
            </span>
          </div>
        </div>
        <ul className="flex flex-col gap-2 text-[12.5px] text-ink-2">
          <li className="flex gap-2">
            <Check size={14} className="mt-0.5 shrink-0 text-ok" /> {t("sc.confirm.onlyGif")}
          </li>
          <li className="flex gap-2">
            <Usb size={14} className="mt-0.5 shrink-0 text-signal" /> {t("sc.confirm.cable")}
          </li>
          <li className="flex gap-2">
            {factory ? (
              <Library size={14} className="mt-0.5 shrink-0 text-ok" />
            ) : (
              <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warn" />
            )}
            {factory ? t("sc.confirm.factoryKept") : t("sc.confirm.noFactory")}
          </li>
          <li className="flex gap-2">
            <Info size={14} className="mt-0.5 shrink-0 text-ink-3" /> {t("sc.wear.hint")}
          </li>
        </ul>
        <div className="flex justify-end gap-2 pt-1">
          <button
            type="button"
            autoFocus
            onClick={onCancel}
            className="h-10 rounded-xl border border-line px-4 text-[13px] text-ink-2 hover:bg-surface-2"
          >
            {t("sc.cancel")}
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className="flex h-10 items-center gap-2 rounded-xl bg-signal px-4 text-[13px] font-semibold text-signal-ink"
          >
            <Upload size={15} /> {t("sc.confirm.go")}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}

// --------------------------------------------------------------- library

/** The frame that shows the most: brightest of up to 24 evenly spaced
 *  frames (animations often start on black). */
function pickThumb(frames: Uint16Array[]): Uint16Array | null {
  if (frames.length === 0) return null;
  const step = Math.max(1, Math.floor(frames.length / 24));
  let best = frames[0]!;
  let bestScore = -1;
  for (let k = 0; k < frames.length; k += step) {
    const f = frames[k]!;
    let score = 0;
    for (let i = 0; i < f.length; i += 7) {
      const v = f[i]!;
      score += (v >> 11) + ((v >> 5) & 63) / 2 + (v & 31);
    }
    if (score > bestScore) {
      bestScore = score;
      best = f;
    }
  }
  return best;
}

function Thumb({ item }: { item: ScreenItem }) {
  const [f, setF] = useState<Uint16Array | null>(null);
  useEffect(() => {
    let live = true;
    loadFrames(item.id, item.frames === 1).then((d) => live && setF(pickThumb(d.frames)), console.error);
    return () => {
      live = false;
    };
  }, [item.id, item.frames]);
  return f ? (
    <FramePlayer frames={[f]} delaysMs={[100]} className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />
  ) : (
    <div className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />
  );
}

function LibraryCard({ usb }: { usb: boolean }) {
  const t = useT();
  const state = useScreen((s) => s.state);
  const upload = useScreen((s) => s.upload);
  const [confirmItem, setConfirmItem] = useState<ScreenItem | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ ok: boolean; text: string } | null>(null);
  const items = state?.items ?? [];

  const runUpload = async (it: ScreenItem) => {
    setConfirmItem(null);
    setNotice(null);
    try {
      await useScreen.getState().uploadToKeyboard(it, "library");
      setNotice({ ok: true, text: `“${it.name}” ${t("sc.done.uploaded")}. ${t("sc.done.fnHome")}` });
    } catch (e) {
      setNotice({ ok: false, text: `${t("sc.uploadFailed")}: ${String(e)}` });
    }
  };

  return (
    <section className="card flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2 px-1">
        <div className="flex items-center gap-2">
          <Library size={14} className="text-ink-3" />
          <div className="eyebrow">{t("sc.library")}</div>
          <span className="num text-[11px] text-ink-3">{items.length}</span>
        </div>
        <span className="text-[11.5px] text-ink-3">{t("sc.library.note")}</span>
      </div>
      {notice && (
        <p className={`flex items-start gap-2 px-1 text-[12.5px] ${notice.ok ? "text-ok" : "text-danger"}`} role="status">
          {notice.ok ? <Check size={14} className="mt-0.5 shrink-0" /> : <X size={14} className="mt-0.5 shrink-0" />}
          {notice.text}
        </p>
      )}
      {items.length === 0 && <p className="px-1 pb-1 text-[12.5px] text-ink-3">{t("sc.library.empty")}</p>}
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-2">
        {items.map((it) => {
          const onKeyboard = state?.shown === it.id;
          const busy = upload?.id === it.id;
          return (
            <li
              key={it.id}
              className={`flex gap-3 rounded-xl border p-3 transition-colors ${
                onKeyboard ? "border-ok/40 bg-ok/5" : "border-line hover:bg-surface-2/60"
              }`}
            >
              <Thumb item={it} />
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                {editing === it.id ? (
                  <input
                    autoFocus
                    defaultValue={it.name}
                    maxLength={60}
                    aria-label={t("sc.rename")}
                    onBlur={(e) => {
                      setEditing(null);
                      const v = e.target.value.trim();
                      if (v && v !== it.name) useScreen.getState().rename(it.id, v).catch((x) => setNotice({ ok: false, text: String(x) }));
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                      if (e.key === "Escape") setEditing(null);
                    }}
                    className="h-7 rounded-md border border-signal bg-bg-raise px-2 text-[12.5px] outline-none"
                  />
                ) : (
                  <span className="truncate text-[13px] font-semibold" title={it.name}>
                    {it.name}
                  </span>
                )}
                <span className="num text-[11px] text-ink-3">
                  {it.frames} {t("sc.frames")}
                  {it.frames > 1 ? ` · ${t("sc.loop")} ${fmtSeconds(it.loopMs / 1000)}` : ""}
                </span>
                <span className="num text-[11px] text-ink-3">
                  {t("sc.uploadTime")} ~{fmtSeconds(uploadSeconds(it.frames))}
                </span>
                <span className="flex gap-1.5 pt-1">
                  {it.kind === "factory" && (
                    <span className="rounded-full bg-surface-2 px-2 py-px text-[10.5px] text-ink-2">{t("sc.badge.factory")}</span>
                  )}
                  {onKeyboard && (
                    <span className="rounded-full bg-ok/15 px-2 py-px text-[10.5px] text-ok">{t("sc.badge.onKeyboard")}</span>
                  )}
                </span>
              </div>
              <div className="flex flex-col items-end justify-between gap-2">
                <button
                  type="button"
                  disabled={!usb || !!upload}
                  title={!usb ? t("sc.needsUsbShort") : it.kind === "factory" ? t("sc.factory.restoreHint") : undefined}
                  onClick={() => setConfirmItem(it)}
                  className="flex h-8 items-center gap-1.5 rounded-lg border border-signal/40 px-2.5 text-[12px] font-medium text-signal hover:bg-signal/10 disabled:opacity-35"
                >
                  {busy ? <Loader2 size={13} className="animate-spin" /> : <Upload size={13} />}
                  {it.kind === "factory" ? t("sc.factory.restore") : t("sc.upload.short")}
                </button>
                {it.kind !== "factory" && (
                  <div className="flex gap-1">
                    <button
                      type="button"
                      title={t("sc.rename")}
                      aria-label={t("sc.rename")}
                      onClick={() => setEditing(it.id)}
                      className="flex h-7 w-7 items-center justify-center rounded-md text-ink-3 hover:bg-surface hover:text-ink"
                    >
                      <Pencil size={13} />
                    </button>
                    <button
                      type="button"
                      title={deleting === it.id ? t("sc.remove.confirm") : t("sc.remove")}
                      aria-label={deleting === it.id ? t("sc.remove.confirm") : t("sc.remove")}
                      disabled={busy}
                      onBlur={() => setDeleting(null)}
                      onClick={() => {
                        if (deleting !== it.id) return setDeleting(it.id);
                        setDeleting(null);
                        useScreen.getState().remove(it.id).catch((x) => setNotice({ ok: false, text: String(x) }));
                      }}
                      className={`flex h-7 items-center justify-center gap-1 rounded-md px-1.5 text-[11.5px] ${
                        deleting === it.id ? "bg-danger/15 text-danger" : "text-ink-3 hover:bg-surface hover:text-danger"
                      }`}
                    >
                      <Trash2 size={13} />
                      {deleting === it.id && t("sc.remove.confirmShort")}
                    </button>
                  </div>
                )}
              </div>
            </li>
          );
        })}
      </ul>
      <AnimatePresence>
        {confirmItem && (
          <LibraryConfirm item={confirmItem} onCancel={() => setConfirmItem(null)} onConfirm={() => runUpload(confirmItem)} />
        )}
      </AnimatePresence>
    </section>
  );
}

function LibraryConfirm({ item, onCancel, onConfirm }: { item: ScreenItem; onCancel: () => void; onConfirm: () => void }) {
  const [first, setFirst] = useState<Uint16Array | null>(null);
  useEffect(() => {
    loadFrames(item.id, item.frames === 1).then((d) => setFirst(pickThumb(d.frames)), console.error);
  }, [item.id, item.frames]);
  if (!first) return null;
  return <ConfirmUpload frames={item.frames} loopMs={item.loopMs} preview={first} onCancel={onCancel} onConfirm={onConfirm} />;
}

// ----------------------------------------------------- upload progress

/** Floating progress while an upload runs. Hidden on the Screen page when
 *  the studio itself shows the progress. */
export function ScreenUploadToast() {
  const t = useT();
  const upload = useScreen((s) => s.upload);
  const page = useApp((s) => s.page);
  const [now, setNow] = useState(Date.now());
  const running = upload !== null;
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(id);
  }, [running]);
  const visible = !!upload && !(page === "screen" && upload.origin === "editor");
  return (
    <AnimatePresence>
      {visible && upload && (
        <motion.div
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 12 }}
          className="card fixed right-6 bottom-6 z-40 flex w-[340px] flex-col gap-2 p-4 shadow-xl"
          role="status"
        >
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <Loader2 size={14} className="animate-spin text-signal" />
            <span className="truncate">
              {t("sc.uploading")}: {upload.name}
            </span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
            <div
              className="h-full rounded-full bg-signal transition-[width]"
              style={{ width: `${Math.round((upload.done / Math.max(1, upload.total)) * 100)}%` }}
            />
          </div>
          <div className="num flex justify-between text-[11.5px] text-ink-3">
            <span>
              {upload.done}/{upload.total} {t("sc.pages")}
            </span>
            <span>
              {remaining(upload, now) ? `${remaining(upload, now)} ${t("sc.left")}` : ""}
            </span>
          </div>
          <p className="text-[11.5px] text-ink-2">{t("sc.uploading.note")}</p>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
