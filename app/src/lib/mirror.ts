import { useEffect, useMemo, useState } from "react";
import type { EffectInfo } from "../bindings/EffectInfo";
import type { MirrorHint } from "../bindings/MirrorHint";
import catalog from "../bindings/lighting-catalog.json";
import { onMirror, setMirror } from "./ipc";
import { rgbToHex, toLedColors, useLighting, type ColorMap, type LightingState } from "./lighting";
import { useApp } from "./store";

/**
 * Colours currently shown on the keyboard, read back with `04 F5` about
 * four times a second while `enabled` and the window is on screen (not
 * while the app runs in the tray). `null` until the first reading.
 */
export function useMirror(wanted: boolean): ColorMap | null {
  const shown = useApp((s) => s.windowShown);
  const enabled = wanted && shown;
  const [colors, setColors] = useState<ColorMap | null>(null);
  const appliedEffect = useLighting((s) => s.appliedEffect);
  const savedPerKey = useLighting((s) => s.savedPerKey);
  const hint = useMemo(() => mirrorHint({ appliedEffect, savedPerKey }), [appliedEffect, savedPerKey]);

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
    return () => {
      alive = false;
      off?.();
      setMirror(false).catch(console.error);
    };
  }, [enabled]);

  // Starts the mirror and updates the hint whenever something new is saved.
  useEffect(() => {
    if (enabled) setMirror(true, hint).catch(console.error);
  }, [enabled, hint]);

  return colors;
}

/**
 * What the app last saved to the keyboard. The read-back reports red as
 * `2·r mod 256`; knowing the expected colour lets the backend pick the
 * right one of the two values each byte can stand for.
 */
export function mirrorHint(s: Pick<LightingState, "appliedEffect" | "savedPerKey">): MirrorHint | null {
  if (s.appliedEffect) {
    const e = s.appliedEffect;
    const fx = (catalog as EffectInfo[]).find((f) => f.id === e.mode);
    const single = !!fx?.color && !(fx.rainbow && e.rainbow);
    return { brightness: e.brightness, color: single ? e.color : null, keys: null };
  }
  if (s.savedPerKey) {
    return { brightness: s.savedPerKey.brightness, color: null, keys: toLedColors(s.savedPerKey.colors) };
  }
  return null;
}
