import { useEffect, useState } from "react";
import { onMirror, setMirror } from "./ipc";
import { rgbToHex, type ColorMap } from "./lighting";

/**
 * Colours currently shown on the keyboard, read back with `04 F5` about
 * four times a second while `enabled`. `null` until the first reading.
 */
export function useMirror(enabled: boolean): ColorMap | null {
  const [colors, setColors] = useState<ColorMap | null>(null);
  useEffect(() => {
    if (!enabled) {
      setColors(null);
      return;
    }
    let off: (() => void) | undefined;
    let alive = true;
    onMirror((list) => {
      if (!alive) return;
      const map: ColorMap = {};
      for (const c of list) map[c.led] = rgbToHex(c.r, c.g, c.b);
      setColors(map);
    }).then((u) => (off = u));
    setMirror(true).catch(console.error);
    return () => {
      alive = false;
      off?.();
      setMirror(false).catch(console.error);
    };
  }, [enabled]);
  return colors;
}
