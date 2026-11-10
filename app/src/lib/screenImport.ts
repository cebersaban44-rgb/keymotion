// Screen import pipeline, UI side (owner's rule, 2026-11-10): whatever the
// user brings is decoded here with the webview's own decoders, framed and
// converted to the keyboard's format (240×135 RGB565, ≤140 frames), shown
// as a preview made from exactly those bytes, and sent only after the user
// confirms. Rust validates the result again before storing or uploading.
//
// Only the frames that will be sent are rendered, so a long video never
// sits in memory as a whole.

export const W = 240;
export const H = 135;
export const HEADER_LEN = 256;
export const FRAME_PIXELS = W * H;
export const MAX_FRAMES = 140;
export const PROVEN_FRAMES = 135;
export const MIN_DELAY_MS = 2;
export const MAX_DELAY_MS = 508;
export const STILL_DELAY_MS = 100;
/** Measured on the keyboard (E1/E4): ~143 ms per 4 KiB page. */
export const PAGE_MS = 145;

/** Upper bounds for files the importer opens. */
export const LIMITS = {
  imageBytes: 64 * 1024 * 1024,
  videoBytes: 1024 * 1024 * 1024,
  side: 8192,
  sourceFrames: 3000,
  /** Longest wait for one decode or seek before the file is given up. */
  stepTimeoutMs: 8000,
};

export type SourceKind = "image" | "animated" | "video";

export class ImportError extends Error {
  constructor(
    public code: "too-large" | "unsupported" | "decode" | "too-many-frames" | "dimensions" | "timeout",
    detail?: string,
  ) {
    super(detail ? `${code}: ${detail}` : code);
  }
}

export interface Source {
  kind: SourceKind;
  /** File name without its extension. */
  name: string;
  width: number;
  height: number;
  /** Total length in ms (0 for a still image). */
  durationMs: number;
  /** Animated images: start time and delay of every frame. */
  frameStarts: number[];
  frameDelays: number[];
  /** Draw the source at `atMs` into `ctx` at `rect` (output pixels). */
  draw(ctx: CanvasRenderingContext2D, atMs: number, rect: Rect): Promise<void>;
  dispose(): void;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

function withTimeout<T>(p: Promise<T>, what: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new ImportError("timeout", what)), LIMITS.stepTimeoutMs);
    p.then(
      (v) => {
        clearTimeout(t);
        resolve(v);
      },
      (e) => {
        clearTimeout(t);
        reject(e);
      },
    );
  });
}

function baseName(file: File) {
  return file.name.replace(/\.[^.]+$/, "").slice(0, 60) || "image";
}

function checkSize(w: number, h: number) {
  if (!w || !h) throw new ImportError("decode", "empty image");
  if (w > LIMITS.side || h > LIMITS.side) throw new ImportError("dimensions", `${w}×${h}`);
}

/** GIF delays of 0 or 10 ms play at 100 ms in browsers, and on the keyboard
 *  (its factory animation, compared by the owner on 2026-11-10). */
function normalizeDelay(ms: number) {
  return ms <= 10 ? 100 : ms;
}

const ANIMATED_TYPES = ["image/gif", "image/webp", "image/png", "image/apng", "image/avif"];

function mimeOf(file: File): string {
  if (file.type) return file.type;
  const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    gif: "image/gif",
    webp: "image/webp",
    png: "image/png",
    apng: "image/apng",
    avif: "image/avif",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    bmp: "image/bmp",
    mp4: "video/mp4",
    webm: "video/webm",
    mov: "video/quicktime",
    m4v: "video/mp4",
  };
  return map[ext] ?? "";
}

/** Open a file the user picked or dropped. `onProgress` reports decoding
 *  of animated images (0–1). */
export async function openSource(file: File, onProgress?: (f: number) => void): Promise<Source> {
  const type = mimeOf(file);
  if (type.startsWith("video/")) {
    if (file.size > LIMITS.videoBytes) throw new ImportError("too-large");
    return openVideo(file);
  }
  if (!type.startsWith("image/")) throw new ImportError("unsupported", type || file.name);
  if (file.size > LIMITS.imageBytes) throw new ImportError("too-large");
  if (ANIMATED_TYPES.includes(type) && "ImageDecoder" in window) {
    const animated = await openAnimated(file, type, onProgress);
    if (animated) return animated;
  }
  return openStill(file);
}

async function openStill(file: File): Promise<Source> {
  let bmp: ImageBitmap;
  try {
    bmp = await withTimeout(createImageBitmap(file), "image");
  } catch (e) {
    if (e instanceof ImportError) throw e;
    throw new ImportError("decode", String(e));
  }
  checkSize(bmp.width, bmp.height);
  return {
    kind: "image",
    name: baseName(file),
    width: bmp.width,
    height: bmp.height,
    durationMs: 0,
    frameStarts: [0],
    frameDelays: [STILL_DELAY_MS],
    draw: async (ctx, _at, r) => ctx.drawImage(bmp, r.x, r.y, r.w, r.h),
    dispose: () => bmp.close(),
  };
}

/** Animated GIF/WebP/APNG via WebCodecs. `null` when the file has one frame. */
async function openAnimated(file: File, type: string, onProgress?: (f: number) => void): Promise<Source | null> {
  // WebCodecs typings are not in every TS lib; keep the surface small.
  const Decoder = (window as unknown as { ImageDecoder: ImageDecoderCtor }).ImageDecoder;
  if (!(await Decoder.isTypeSupported(type))) return null;
  let dec: ImageDecoderLike;
  try {
    dec = new Decoder({ data: await file.arrayBuffer(), type });
    await withTimeout(dec.tracks.ready, "tracks");
  } catch (e) {
    if (e instanceof ImportError) throw e;
    throw new ImportError("decode", String(e));
  }
  const track = dec.tracks.selectedTrack;
  const count = track?.frameCount ?? 1;
  if (!track || count <= 1) {
    dec.close();
    return null;
  }
  if (count > LIMITS.sourceFrames) {
    dec.close();
    throw new ImportError("too-many-frames", String(count));
  }
  // Durations are only known per decoded frame.
  const delays: number[] = [];
  let width = 0;
  let height = 0;
  for (let i = 0; i < count; i++) {
    const { image } = await withTimeout(dec.decode({ frameIndex: i }), `frame ${i}`);
    if (i === 0) {
      width = image.displayWidth;
      height = image.displayHeight;
    }
    delays.push(normalizeDelay((image.duration ?? 0) / 1000));
    image.close();
    if (i % 8 === 0) onProgress?.(i / count);
  }
  onProgress?.(1);
  checkSize(width, height);
  const starts: number[] = [];
  let t = 0;
  for (const d of delays) {
    starts.push(t);
    t += d;
  }
  return {
    kind: "animated",
    name: baseName(file),
    width,
    height,
    durationMs: t,
    frameStarts: starts,
    frameDelays: delays,
    draw: async (ctx, atMs, r) => {
      const { image } = await withTimeout(dec.decode({ frameIndex: frameAt(starts, atMs) }), "frame");
      ctx.drawImage(image as unknown as CanvasImageSource, r.x, r.y, r.w, r.h);
      image.close();
    },
    dispose: () => dec.close(),
  };
}

/** Index of the frame showing at `ms`. */
export function frameAt(starts: number[], ms: number): number {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid]! <= ms) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

async function openVideo(file: File): Promise<Source> {
  const url = URL.createObjectURL(file);
  const v = document.createElement("video");
  v.muted = true;
  v.playsInline = true;
  v.preload = "auto";
  v.src = url;
  try {
    await withTimeout(
      new Promise<void>((resolve, reject) => {
        v.onloadeddata = () => resolve();
        v.onerror = () => reject(new ImportError("decode", v.error?.message ?? "video"));
      }),
      "video",
    );
  } catch (e) {
    URL.revokeObjectURL(url);
    throw e;
  }
  checkSize(v.videoWidth, v.videoHeight);
  // Some WebM files (e.g. from screen or MediaRecorder captures) carry no
  // duration; seeking far past the end makes the browser work it out.
  if (!Number.isFinite(v.duration)) {
    try {
      await withTimeout(
        new Promise<void>((resolve) => {
          v.ondurationchange = () => Number.isFinite(v.duration) && resolve();
          v.currentTime = 1e9;
        }),
        "duration",
      );
    } catch {
      /* falls through to the check below */
    }
    v.ondurationchange = null;
    v.currentTime = 0;
  }
  const duration = Number.isFinite(v.duration) ? v.duration * 1000 : 0;
  if (duration <= 0) {
    URL.revokeObjectURL(url);
    throw new ImportError("decode", "unknown duration");
  }
  const seek = (ms: number) =>
    withTimeout(
      new Promise<void>((resolve) => {
        if (Math.abs(v.currentTime * 1000 - ms) < 0.5 && v.readyState >= 2) return resolve();
        v.onseeked = () => resolve();
        v.currentTime = Math.min(ms, duration - 1) / 1000;
      }),
      "seek",
    );
  return {
    kind: "video",
    name: baseName(file),
    width: v.videoWidth,
    height: v.videoHeight,
    durationMs: duration,
    frameStarts: [],
    frameDelays: [],
    draw: async (ctx, atMs, r) => {
      await seek(atMs);
      ctx.drawImage(v, r.x, r.y, r.w, r.h);
    },
    dispose: () => {
      v.removeAttribute("src");
      v.load();
      URL.revokeObjectURL(url);
    },
  };
}

// ----------------------------------------------------------------- framing

export type Fit = "fill" | "fit" | "stretch";

export interface Framing {
  fit: Fit;
  /** 1 = the fit's own size. */
  zoom: number;
  /** −1…1: moves the image right/down within the free room. */
  panX: number;
  panY: number;
}

export const DEFAULT_FRAMING: Framing = { fit: "fill", zoom: 1, panX: 0, panY: 0 };

/** Where the source lands in the 240×135 output. */
export function placement(sw: number, sh: number, f: Framing): Rect {
  if (f.fit === "stretch") return { x: 0, y: 0, w: W, h: H };
  const base = f.fit === "fill" ? Math.max(W / sw, H / sh) : Math.min(W / sw, H / sh);
  const s = base * f.zoom;
  const w = sw * s;
  const h = sh * s;
  const slackX = W - w;
  const slackY = H - h;
  return {
    x: slackX / 2 + (f.panX * Math.abs(slackX)) / 2,
    y: slackY / 2 + (f.panY * Math.abs(slackY)) / 2,
    w,
    h,
  };
}

/** Free room (output px) the image can move in, per axis. */
export function slack(sw: number, sh: number, f: Framing) {
  const r = placement(sw, sh, { ...f, panX: 0, panY: 0 });
  return { x: Math.abs(W - r.w), y: Math.abs(H - r.h) };
}

// ------------------------------------------------------------------ timing

export interface Timing {
  startMs: number;
  endMs: number;
  /** `0` keeps the source's own frames and delays (animated images). */
  fps: number;
}

export interface PlannedFrame {
  atMs: number;
  delayMs: number;
}

export interface Plan {
  frames: PlannedFrame[];
  /** More frames would be needed than the keyboard takes. */
  clipped: boolean;
}

export function delayByte(ms: number) {
  return Math.min(254, Math.max(1, Math.round(ms / 2)));
}

/** Which source moments become frames, and for how long each shows. */
export function plan(src: Source, t: Timing): Plan {
  if (src.kind === "image") return { frames: [{ atMs: 0, delayMs: STILL_DELAY_MS }], clipped: false };
  const start = Math.max(0, Math.min(t.startMs, src.durationMs));
  const end = Math.max(start, Math.min(t.endMs, src.durationMs));
  if (t.fps === 0 && src.kind === "animated") {
    const frames: PlannedFrame[] = [];
    src.frameStarts.forEach((s, i) => {
      const d = src.frameDelays[i]!;
      if (s + d > start && s < end) frames.push({ atMs: Math.max(s, start), delayMs: d });
    });
    if (frames.length === 0) frames.push({ atMs: start, delayMs: src.frameDelays[frameAt(src.frameStarts, start)]! });
    return { frames: frames.slice(0, MAX_FRAMES), clipped: frames.length > MAX_FRAMES };
  }
  const fps = t.fps || 10;
  const step = 1000 / fps;
  const wanted = Math.max(1, Math.floor((end - start) / step + 1e-6));
  const n = Math.min(MAX_FRAMES, wanted);
  const delay = Math.min(MAX_DELAY_MS, Math.max(MIN_DELAY_MS, Math.round(step)));
  return {
    frames: Array.from({ length: n }, (_, k) => ({ atMs: start + k * step, delayMs: delay })),
    clipped: wanted > MAX_FRAMES,
  };
}

export function pagesFor(frames: number) {
  return Math.ceil((HEADER_LEN + FRAME_PIXELS * 2 * frames) / 4096);
}

/** Estimated upload time in seconds (measured ~143 ms/page + session). */
export function uploadSeconds(frames: number) {
  return (pagesFor(frames) * PAGE_MS) / 1000 + 0.5;
}

// --------------------------------------------------------------- converting

const BAYER4 = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/** RGBA (240×135) → RGB565. Ordered (Bayer 4×4) dithering spreads the
 *  RGB565 steps so gradients do not band; it is stable from frame to frame,
 *  so animations do not shimmer. */
export function toRgb565(rgba: Uint8ClampedArray, dither: boolean): Uint16Array {
  const out = new Uint16Array(FRAME_PIXELS);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const i = y * W + x;
      const p = i * 4;
      const t = dither ? (BAYER4[(y & 3) * 4 + (x & 3)]! + 0.5) / 16 : 0.5;
      const r = Math.min(31, Math.floor((rgba[p]! * 31) / 255 + t));
      const g = Math.min(63, Math.floor((rgba[p + 1]! * 63) / 255 + t));
      const b = Math.min(31, Math.floor((rgba[p + 2]! * 31) / 255 + t));
      out[i] = (r << 11) | (g << 5) | b;
    }
  }
  return out;
}

/** RGB565 → RGBA for previews: exactly the colours the panel gets. */
export function toRgba(
  px: Uint16Array,
  out: Uint8ClampedArray<ArrayBuffer> = new Uint8ClampedArray(FRAME_PIXELS * 4),
): Uint8ClampedArray<ArrayBuffer> {
  for (let i = 0; i < FRAME_PIXELS; i++) {
    const v = px[i]!;
    const r = (v >> 11) & 31;
    const g = (v >> 5) & 63;
    const b = v & 31;
    const p = i * 4;
    out[p] = (r << 3) | (r >> 2);
    out[p + 1] = (g << 2) | (g >> 4);
    out[p + 2] = (b << 3) | (b >> 2);
    out[p + 3] = 255;
  }
  return out;
}

export interface Prepared {
  frames: Uint16Array[];
  delaysMs: number[];
  name: string;
  kind: "image" | "animation";
}

/** A canvas for one 240×135 output frame. */
export function outputCanvas() {
  const c = document.createElement("canvas");
  c.width = W;
  c.height = H;
  const ctx = c.getContext("2d", { willReadFrequently: true })!;
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  return { canvas: c, ctx };
}

/** Draw one source moment with the framing into `ctx` (black background). */
export async function renderAt(src: Source, ctx: CanvasRenderingContext2D, atMs: number, f: Framing) {
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, W, H);
  await src.draw(ctx, atMs, placement(src.width, src.height, f));
}

/** Render every planned frame. Cancellable through `signal`. */
export async function prepare(
  src: Source,
  p: Plan,
  f: Framing,
  dither: boolean,
  name: string,
  onProgress?: (done: number, total: number) => void,
  signal?: AbortSignal,
): Promise<Prepared> {
  const { ctx } = outputCanvas();
  const frames: Uint16Array[] = [];
  for (let i = 0; i < p.frames.length; i++) {
    if (signal?.aborted) throw new DOMException("aborted", "AbortError");
    await renderAt(src, ctx, p.frames[i]!.atMs, f);
    frames.push(toRgb565(ctx.getImageData(0, 0, W, H).data, dither));
    onProgress?.(i + 1, p.frames.length);
  }
  return {
    frames,
    delaysMs: p.frames.map((x) => x.delayMs),
    name,
    kind: frames.length > 1 ? "animation" : "image",
  };
}

/** The keyboard's blob (header + frames, little-endian), as Rust expects. */
export function toBlob(p: Prepared): Uint8Array {
  const n = p.frames.length;
  const out = new Uint8Array(HEADER_LEN + n * FRAME_PIXELS * 2);
  out.fill(0xff, 0, HEADER_LEN);
  out[0] = n;
  p.delaysMs.forEach((d, i) => (out[1 + i] = delayByte(d)));
  const view = new DataView(out.buffer);
  p.frames.forEach((f, k) => {
    const base = HEADER_LEN + k * FRAME_PIXELS * 2;
    for (let i = 0; i < FRAME_PIXELS; i++) view.setUint16(base + i * 2, f[i]!, true);
  });
  return out;
}

/** Read a stored blob back into frames (for library previews). */
export function fromBlob(data: ArrayBuffer): { frames: Uint16Array[]; delaysMs: number[] } {
  const bytes = new Uint8Array(data);
  const n = bytes[0] ?? 0;
  const view = new DataView(data);
  const frames: Uint16Array[] = [];
  const delaysMs: number[] = [];
  for (let k = 0; k < n; k++) {
    const base = HEADER_LEN + k * FRAME_PIXELS * 2;
    if (base + FRAME_PIXELS * 2 > bytes.length) break;
    const f = new Uint16Array(FRAME_PIXELS);
    for (let i = 0; i < FRAME_PIXELS; i++) f[i] = view.getUint16(base + i * 2, true);
    frames.push(f);
    delaysMs.push(bytes[1 + k]! * 2);
  }
  return { frames, delaysMs };
}

// Minimal WebCodecs ImageDecoder surface used above.
interface DecodedImage {
  displayWidth: number;
  displayHeight: number;
  duration: number | null;
  close(): void;
}
interface ImageDecoderLike {
  tracks: { ready: Promise<void>; selectedTrack: { frameCount: number } | null };
  decode(o: { frameIndex: number }): Promise<{ image: DecodedImage }>;
  close(): void;
}
interface ImageDecoderCtor {
  new (init: { data: ArrayBuffer; type: string }): ImageDecoderLike;
  isTypeSupported(type: string): Promise<boolean>;
}
