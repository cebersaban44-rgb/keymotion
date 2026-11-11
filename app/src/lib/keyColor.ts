// How a key light looks and is named: parsing the CSS colours the pages
// pass to the keyboard drawing, telling lit from dark keys, choosing a
// legend colour that stays readable, and naming a colour in words for
// screen readers ("Q, orange" instead of "Q #ff8a3d").

import type { MessageKey } from "./i18n";

export type Rgb = [number, number, number];

/** "#rgb", "#rrggbb" or "rgb(r, g, b)" → channels; anything else → null. */
export function parseColor(css: string | undefined | null): Rgb | null {
  if (!css) return null;
  const s = css.trim();
  if (s[0] === "#") {
    const h = s.slice(1);
    if (/^[0-9a-f]{3}$/i.test(h)) return [0, 1, 2].map((i) => parseInt(h[i]! + h[i]!, 16)) as Rgb;
    if (/^[0-9a-f]{6}/i.test(h)) return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;
    return null;
  }
  const m = /^rgba?\(\s*(\d+(?:\.\d+)?)[\s,]+(\d+(?:\.\d+)?)[\s,]+(\d+(?:\.\d+)?)/i.exec(s);
  return m ? ([m[1], m[2], m[3]].map((v) => Math.min(255, Math.round(Number(v)))) as Rgb) : null;
}

const linear = (c: number) => {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
};

/** WCAG relative luminance, 0 (black) to 1 (white). */
export function luminance([r, g, b]: Rgb): number {
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

/** Below this the LED is off or too dim to read as a colour. */
const DARK = 0.006;
/** Above this a dark legend reads better than a light one. */
const BRIGHT = 0.18;

export type KeyTone = "off" | "deep" | "bright";

/** Off (drawn as an unlit cap), a deep colour (light legend) or a bright one (dark legend). */
export function keyTone(rgb: Rgb | null): KeyTone {
  if (!rgb) return "off";
  const l = luminance(rgb);
  return l < DARK ? "off" : l > BRIGHT ? "bright" : "deep";
}

/** Colour name (message key) for a lit key; `null` when the key is off. */
export function colorNameKey(rgb: Rgb | null): MessageKey | null {
  if (!rgb || keyTone(rgb) === "off") return null;
  const [r, g, b] = rgb.map((v) => v / 255) as Rgb;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  if (s < 0.18 || d < 0.08) return l > 0.78 ? "color.white" : "color.gray";
  let h = 0;
  if (max === r) h = ((g - b) / d) % 6;
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  h = (h * 60 + 360) % 360;
  if (h < 15 || h >= 340) return "color.red";
  if (h < 40) return "color.orange";
  if (h < 66) return "color.yellow";
  if (h < 95) return "color.lime";
  if (h < 160) return "color.green";
  if (h < 190) return "color.cyan";
  if (h < 215) return "color.sky";
  if (h < 250) return "color.blue";
  if (h < 285) return "color.violet";
  if (h < 320) return "color.magenta";
  return "color.pink";
}
