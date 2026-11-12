// Pieces every Screen Studio source shares: the step frame (Stepper +
// "Vazgeç"), the preview-and-upload step, the upload confirmation and
// progress, and small helpers.
//
// The flow (owner's review, 2026-11-10):
//   1 Source   pick a file, a text card or a slideshow
//   2 Edit     each source's own editor                      → "Önizle"
//   3 Preview  the exact bytes play; confirm                  → "Klavyeye yükle"
//   4 Upload   progress in place; on success the studio returns to step 1
//              and says what was done.
// "Vazgeç" leaves the flow at any step before the upload.

import { AlertTriangle, Check, Download, Info, Library, Loader2, Pencil, Play, Upload, Usb, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { ScreenItem } from "../../bindings/ScreenItem";
import { Field } from "../../components/controls";
import { decimal, percent, tNow, useT, type MessageKey } from "../../lib/i18n";
import { useLive } from "../../lib/live";
import { itemData, useScreen } from "../../lib/screen";
import {
  H,
  ImportError,
  brightestFrame,
  PROVEN_FRAMES,
  W,
  fromBlob,
  toRgba,
  uploadSeconds,
  type Prepared,
} from "../../lib/screenImport";
import { friendlyError } from "../../lib/errors";

// ---------------------------------------------------------------- helpers

export function fmtSeconds(s: number) {
  if (s < 60) return `${s < 10 ? decimal(s) : Math.round(s)} ${tNow("time.s")}`;
  const m = Math.floor(s / 60);
  return `${m} ${tNow("time.m")} ${Math.round(s - m * 60)} ${tNow("time.s")}`;
}

/** Loop length as the keyboard plays it (delays are stored in 2 ms units). */
export function loopMsOf(delaysMs: number[]) {
  return delaysMs.reduce((a, d) => a + Math.max(2, Math.round(d / 2) * 2), 0);
}

export function useNow() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(id);
  }, []);
  return now;
}

/** Frames cache for library previews (data never changes for an id). */
const framesCache = new Map<string, Promise<{ frames: Uint16Array[]; delaysMs: number[] }>>();
export function loadFrames(id: string, first?: boolean) {
  const key = first ? `${id}:1` : id;
  let p = framesCache.get(key);
  if (!p) {
    p = itemData(id, first ? 1 : undefined).then(fromBlob);
    framesCache.set(key, p);
    p.catch(() => framesCache.delete(key));
  }
  return p;
}

/** The frame that shows the most (library thumbnails, confirmations). */
export const pickThumb = brightestFrame;

/**
 * The name to show for a library item, or for something put back from the
 * library: the built-in items (the factory backup, Keymotion's animation)
 * get their translated labels (the backend keeps one stored name for each).
 * Without an id, the stored name is compared.
 */
export function useItemName() {
  const t = useT();
  const factoryName = t("sc.factory.name");
  const keymotionName = t("sc.keymotion.name");
  const factory = useScreen((s) => s.state?.items.find((i) => i.kind === "factory") ?? null);
  const keymotion = useScreen((s) => s.state?.items.find((i) => i.kind === "keymotion") ?? null);
  // The same function until a built-in item or the language changes, so a
  // memo that uses it does not recompute on every render.
  return useCallback(
    (x: { id?: string | null; name: string }) => {
      const is = (item: { id: string; name: string } | null) =>
        !!item && (x.id ? x.id === item.id : x.name === item.name);
      return is(factory) ? factoryName : is(keymotion) ? keymotionName : x.name;
    },
    [factory, factoryName, keymotion, keymotionName],
  );
}

/** The library has the factory backup (`true` until the library is known). */
export function useHasFactory() {
  return useScreen((s) => (s.state ? s.state.items.some((i) => i.kind === "factory") : true));
}

/**
 * "Resmi sürücüden yedek al": while there is no factory backup and AJAZZ's
 * official driver left its copy on this PC (looked for once per run). It
 * copies that file into the library as the factory backup.
 */
export function DriverBackup({ onDone }: { onDone?: () => void }) {
  const t = useT();
  const driver = useScreen((s) => s.driver);
  const hasFactory = useHasFactory();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!hasFactory) useScreen.getState().findDriver();
  }, [hasFactory]);
  if (hasFactory || driver !== "found") return null;
  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      await useScreen.getState().backupFromDriver();
      // The live mode no longer needs to ask about the factory animation.
      await useLive.getState().load();
      onDone?.();
    } catch (e) {
      setError(friendlyError(e, t));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-1.5">
      <button
        type="button"
        onClick={run}
        disabled={busy}
        className="flex h-8 w-fit items-center gap-1.5 rounded-lg border border-signal/50 px-3 text-[12px] font-semibold text-signal hover:bg-signal/10 disabled:opacity-50"
        data-driver-backup
      >
        {busy ? <Loader2 size={13} className="animate-spin" /> : <Download size={13} />}
        {busy ? t("sc.factory.backingUp") : t("sc.factory.backup")}
      </button>
      {error && <Hint tone="error">{error}</Hint>}
    </div>
  );
}

export function errorKey(e: unknown): MessageKey {
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

/** What the last finished flow did, shown above the source picker. */
export type Done = { kind: "uploaded" | "saved"; name: string; seconds?: number; /** library item */ id?: string };

/** Plays RGB565 frames on a 240×135 canvas, scaled with sharp pixels. */
export function FramePlayer({
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

/** A library item's representative frame, 120×68. */
export function Thumb({ item }: { item: ScreenItem }) {
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

/** One line of guidance under a step (warning, note, error). */
export function Hint({ tone, children }: { tone: "warn" | "note" | "usb" | "error"; children: ReactNode }) {
  const icon =
    tone === "warn" ? (
      <AlertTriangle size={13} className="mt-0.5 shrink-0" />
    ) : tone === "usb" ? (
      <Usb size={13} className="mt-0.5 shrink-0" />
    ) : tone === "error" ? (
      <X size={13} className="mt-0.5 shrink-0" />
    ) : (
      <Info size={13} className="mt-0.5 shrink-0" />
    );
  const color = tone === "warn" ? "text-warn" : tone === "error" ? "text-danger" : "text-ink-3";
  return (
    <p className={`flex items-start gap-2 text-[12px] ${color}`}>
      {icon}
      {children}
    </p>
  );
}

// ------------------------------------------------------------- the frame

export type Step = 1 | 2 | 3 | 4;

export function Stepper({ step }: { step: Step }) {
  const t = useT();
  const steps: MessageKey[] = ["sc.step.source", "sc.step.edit", "sc.step.preview", "sc.step.upload"];
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
            <span className={`text-[13px] ${state === "now" ? "font-semibold text-ink" : "text-ink-3"}`}>{t(k)}</span>
          </li>
        );
      })}
    </ol>
  );
}

/** Steps 2–4 of any source: the stepper, "Vazgeç", a line saying what is
 *  being edited, and the step's content. */
export function StudioFrame({
  step,
  locked,
  onCancel,
  title,
  detail,
  children,
}: {
  step: Step;
  locked: boolean;
  onCancel: () => void;
  title: string;
  detail?: string;
  children: ReactNode;
}) {
  const t = useT();
  return (
    <section className="card flex flex-col gap-5 p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Stepper step={step} />
        <button
          type="button"
          onClick={onCancel}
          disabled={locked}
          className="flex h-8 items-center gap-1.5 rounded-lg border border-line px-3 text-[13px] font-medium text-ink-2 hover:bg-surface-2 hover:text-ink disabled:opacity-40"
        >
          <X size={14} /> {t("sc.cancel")}
        </button>
      </div>
      <div className="flex min-w-0 items-baseline gap-3">
        <span className="truncate text-[14px] font-semibold">{title}</span>
        {detail && <span className="num truncate text-[12px] text-ink-3">{detail}</span>}
      </div>
      {children}
    </section>
  );
}

export function Stats({ frames, loopMs }: { frames: number; loopMs: number }) {
  const t = useT();
  return (
    <div className="num flex flex-wrap gap-x-4 gap-y-1 text-[12px] text-ink-2">
      <span>{t("sc.frames", { count: frames })}</span>
      {frames > 1 && <span>{t("sc.loopOf", { time: fmtSeconds(loopMs / 1000) })}</span>}
      <span>{t("sc.uploadAbout", { time: fmtSeconds(uploadSeconds(frames)) })}</span>
    </div>
  );
}

/** Bottom bar of an editor (step 2): what will be sent, and "Önizle". */
export function EditFooter({
  frames,
  loopMs,
  preparing,
  disabled,
  onPreview,
}: {
  frames: number;
  loopMs: number;
  preparing: { done: number; total: number } | null;
  disabled?: boolean;
  onPreview: () => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-wrap items-center justify-between gap-4 border-t border-line pt-4">
      <Stats frames={frames} loopMs={loopMs} />
      <button
        type="button"
        disabled={!!preparing || disabled}
        onClick={onPreview}
        className="flex h-10 items-center gap-2 rounded-lg bg-signal px-5 text-[13px] font-semibold text-signal-ink disabled:opacity-50"
      >
        {preparing ? <Loader2 size={15} className="animate-spin" /> : <Play size={15} />}
        {preparing ? t("sc.preparing", { done: preparing.done, total: preparing.total }) : t("sc.prepare")}
      </button>
    </div>
  );
}

// ------------------------------------------------- preview and upload

/** Steps 3 and 4, the same for every source: the prepared bytes play,
 *  the user names it and uploads (or only saves it to the library). */
export function PreviewPanel({
  prepared,
  name,
  onName,
  fallbackName,
  usb,
  loaded,
  onBack,
  onBusy,
  onFinished,
}: {
  prepared: Prepared;
  /** The library name; kept by the source so it survives "Düzenlemeye dön". */
  name: string;
  onName: (name: string) => void;
  fallbackName: string;
  usb: boolean;
  loaded: boolean;
  onBack: () => void;
  onBusy: (busy: boolean) => void;
  onFinished: (d: Done) => void;
}) {
  const t = useT();
  const upload = useScreen((s) => s.upload);
  const [confirm, setConfirm] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const finalName = name.trim() || fallbackName;
  const frames = prepared.frames.length;
  const loopMs = loopMsOf(prepared.delaysMs);

  const store = () => useScreen.getState().add({ ...prepared, name: finalName });

  const saveOnly = async () => {
    setError(null);
    try {
      const item = await store();
      onFinished({ kind: "saved", name: finalName, id: item.id });
    } catch (e) {
      setError(friendlyError(e, t));
    }
  };

  const runUpload = async () => {
    setConfirm(false);
    setError(null);
    setUploading(true);
    onBusy(true);
    const started = performance.now();
    try {
      const item = await store();
      await useScreen.getState().uploadToKeyboard(item, "editor");
      onBusy(false);
      onFinished({ kind: "uploaded", name: finalName, seconds: (performance.now() - started) / 1000, id: item.id });
    } catch (e) {
      setError(friendlyError(e, t));
      setUploading(false);
      onBusy(false);
    }
  };

  return (
    <>
      <div className="grid grid-cols-1 gap-6 @min-[860px]:grid-cols-[480px_minmax(280px,1fr)]">
        <div className="flex flex-col gap-2">
          <div className="relative">
            <FramePlayer frames={prepared.frames} delaysMs={prepared.delaysMs} className="h-[270px] w-[480px] rounded-lg bg-black" />
            <span className="absolute top-2 left-2 flex items-center gap-1 rounded-full bg-black/70 px-2 py-0.5 text-[11px] text-white">
              <Play size={10} /> {t("sc.preview.result")}
            </span>
          </div>
          <p className="text-[12px] text-ink-3">{t("sc.preview.resultNote")}</p>
        </div>
        <div className="flex flex-col gap-4">
          <div>
            <div className="text-[15px] font-semibold">{uploading ? t("sc.uploading") : t("sc.preview.ready")}</div>
            <p className="mt-1 text-[13px] text-ink-2">{uploading ? t("sc.uploading.note") : t("sc.preview.check")}</p>
          </div>
          <Field label={t("sc.name")}>
            <input
              value={name}
              maxLength={60}
              disabled={uploading}
              onChange={(e) => onName(e.target.value)}
              className="h-9 rounded-lg border border-line bg-bg-raise px-3 text-[13px] outline-none focus:border-signal disabled:opacity-60"
            />
          </Field>
          <Stats frames={frames} loopMs={loopMs} />
          {uploading ? (
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
      </div>

      {!uploading && (
        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-line pt-4">
          <button
            type="button"
            onClick={saveOnly}
            title={t("sc.saveOnly.hint")}
            className="flex h-10 items-center gap-2 rounded-lg px-3 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
          >
            <Library size={14} /> {t("sc.saveOnly")}
          </button>
          <button
            type="button"
            onClick={onBack}
            className="flex h-10 items-center gap-2 rounded-lg border border-line px-4 text-[13px] font-medium hover:bg-surface-2"
          >
            <Pencil size={14} /> {t("sc.backToEdit")}
          </button>
          <button
            type="button"
            disabled={!usb || !loaded || !!upload}
            title={!usb ? t("sc.needsUsb") : undefined}
            onClick={() => setConfirm(true)}
            className="flex h-10 items-center gap-2 rounded-lg bg-signal px-5 text-[13px] font-semibold text-signal-ink disabled:opacity-40"
          >
            <Upload size={15} /> {t("sc.uploadAction")}
          </button>
        </div>
      )}

      {frames > PROVEN_FRAMES && <Hint tone="note">{t("sc.warn.unproven")}</Hint>}
      {!usb && !uploading && <Hint tone="usb">{t("sc.needsUsbShort")}</Hint>}
      {error && <Hint tone="error">{error}</Hint>}

      <AnimatePresence>
        {confirm && (
          <ConfirmUpload
            frames={frames}
            loopMs={loopMs}
            preview={pickThumb(prepared.frames)!}
            onCancel={() => setConfirm(false)}
            onConfirm={runUpload}
          />
        )}
      </AnimatePresence>
    </>
  );
}

/** Time left ("~1 dk 20 sn") from the pages done so far; `null` until known. */
export function remaining(u: { done: number; total: number; startedMs: number }, nowMs: number): string | null {
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
      <div className="flex items-center gap-2 text-[13px] text-ink-2">
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
          {percent(pct)}
        </span>
        <span>{left ? t("sc.left", { time: left }) : ""}</span>
      </div>
    </div>
  );
}

/** The upload confirmation for a library item (its first frame as preview). */
export function LibraryConfirm({ item, onCancel, onConfirm }: { item: ScreenItem; onCancel: () => void; onConfirm: () => void }) {
  const [first, setFirst] = useState<Uint16Array | null>(null);
  useEffect(() => {
    loadFrames(item.id, item.frames === 1).then((d) => setFirst(pickThumb(d.frames)), console.error);
  }, [item.id, item.frames]);
  if (!first) return null;
  return <ConfirmUpload frames={item.frames} loopMs={item.loopMs} preview={first} onCancel={onCancel} onConfirm={onConfirm} />;
}

/**
 * "Yeniden yükle" for an upload that was cut short: confirms like any
 * upload, then uploads the item again (the progress toast follows it).
 */
export function RetryUpload({ item, usb, onDone }: { item: ScreenItem; usb: boolean; onDone?: () => void }) {
  const t = useT();
  const upload = useScreen((s) => s.upload);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async () => {
    setConfirm(false);
    setError(null);
    try {
      await useScreen.getState().uploadToKeyboard(item, "library");
      onDone?.();
    } catch (e) {
      setError(friendlyError(e, t));
    }
  };
  return (
    <div className="flex flex-col gap-1.5">
      <button
        type="button"
        disabled={!usb || !!upload}
        title={!usb ? t("sc.needsUsbShort") : undefined}
        onClick={() => setConfirm(true)}
        className="flex h-8 w-fit items-center gap-1.5 rounded-lg bg-signal px-3 text-[12px] font-semibold text-signal-ink disabled:opacity-40"
        data-retry-upload
      >
        {upload?.id === item.id ? <Loader2 size={13} className="animate-spin" /> : <Upload size={13} />}
        {t("sc.incomplete.action")}
      </button>
      {error && <Hint tone="error">{error}</Hint>}
      {/* On the page body: the button may sit in a floating notice. */}
      {createPortal(
        <AnimatePresence>
          {confirm && <LibraryConfirm item={item} onCancel={() => setConfirm(false)} onConfirm={run} />}
        </AnimatePresence>,
        document.body,
      )}
    </div>
  );
}

export function ConfirmUpload({
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
      <motion.div initial={{ scale: 0.97, y: 8 }} animate={{ scale: 1, y: 0 }} className="card flex w-[480px] flex-col gap-4 p-6">
        <h2 className="font-display text-[22px] font-semibold">{t("sc.confirm.title")}</h2>
        <div className="flex gap-4">
          <FramePlayer frames={[preview]} delaysMs={[100]} className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />
          <div className="num flex flex-col gap-0.5 text-[13px] text-ink-2">
            <span>{t("sc.frames", { count: frames })}</span>
            {frames > 1 && <span>{t("sc.loopOf", { time: fmtSeconds(loopMs / 1000) })}</span>}
            <span>{t("sc.uploadAbout", { time: fmtSeconds(uploadSeconds(frames)) })}</span>
            <span>{t("sc.confirm.todayNth", { n: (state?.uploadsToday ?? 0) + 1 })}</span>
          </div>
        </div>
        <ul className="flex flex-col gap-2 text-[13px] text-ink-2">
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
            <span className="flex flex-col gap-2">
              {factory ? t("sc.confirm.factoryKept") : t("sc.confirm.noFactory")}
              <DriverBackup />
            </span>
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
            className="h-10 rounded-lg border border-line px-4 text-[13px] text-ink-2 hover:bg-surface-2"
          >
            {t("sc.cancel")}
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className="flex h-10 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink"
          >
            <Upload size={15} /> {t("sc.confirm.go")}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
