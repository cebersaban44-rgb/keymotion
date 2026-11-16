// Program icons, read by the backend from the program's file (the program
// is never opened: kb-winsys `icon::exe_icon`). Each is asked for once per
// size and kept for the session.

import { useEffect, useState } from "react";
import { inTauri, invoke } from "./ipc";

const cache = new Map<string, Promise<ImageData | null>>();

/** The icon of the program at `path`, `size`×`size` pixels; `null` when it has none. */
export function programIconData(path: string, size = 32): Promise<ImageData | null> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(null);
  const key = `${size}:${path.toLowerCase()}`;
  let icon = cache.get(key);
  if (!icon) {
    icon = invoke<ArrayBuffer>("program_icon", { path, size })
      .then((buf) => (buf.byteLength === size * size * 4 ? new ImageData(new Uint8ClampedArray(buf), size, size) : null))
      .catch(() => null);
    cache.set(key, icon);
  }
  return icon;
}

const urls = new Map<string, string | null>();

/** The icon as an image address for `<img>`, or `null`. */
export async function programIcon(path: string, size = 32): Promise<string | null> {
  const key = `${size}:${path.toLowerCase()}`;
  const known = urls.get(key);
  if (known !== undefined) return known;
  const data = await programIconData(path, size);
  let url: string | null = null;
  if (data) {
    const canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    canvas.getContext("2d")?.putImageData(data, 0, 0);
    url = canvas.toDataURL("image/png");
  }
  urls.set(key, url);
  return url;
}

/** `programIcon` for a component; `null` until it is read, or without one. */
export function useProgramIcon(path: string | null | undefined, size = 32): string | null {
  const [url, setUrl] = useState<{ path: string; url: string | null } | null>(null);
  useEffect(() => {
    if (!path) return;
    let live = true;
    programIcon(path, size).then((u) => live && setUrl({ path, url: u }), console.error);
    return () => {
      live = false;
    };
  }, [path, size]);
  return path && url?.path === path ? url.url : null;
}

/** Its main colour: the average of its opaque, colourful pixels (a start
 *  card's background when the game has no cover). */
export function iconTone(data: ImageData): [number, number, number] | null {
  let r = 0;
  let g = 0;
  let b = 0;
  let weight = 0;
  const d = data.data;
  for (let i = 0; i < d.length; i += 4) {
    const a = d[i + 3]! / 255;
    if (a < 0.5) continue;
    const pr = d[i]!;
    const pg = d[i + 1]!;
    const pb = d[i + 2]!;
    // Grey pixels (outlines, shadows) count little.
    const sat = Math.max(pr, pg, pb) - Math.min(pr, pg, pb);
    const w = a * (0.1 + sat / 255);
    r += pr * w;
    g += pg * w;
    b += pb * w;
    weight += w;
  }
  return weight > 0 ? [Math.round(r / weight), Math.round(g / weight), Math.round(b / weight)] : null;
}
