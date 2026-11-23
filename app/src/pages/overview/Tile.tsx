// Pieces the Overview's rows share: colour swatches and a clock that ticks
// often enough for "in 25 min".

import { useEffect, useState } from "react";
import { useT } from "../../lib/i18n";
import { fmtWait } from "../../lib/live";

const RAINBOW = "conic-gradient(#ff3b30, #ffcc00, #34c759, #2ee6d6, #3a5bff, #e040fb, #ff3b30)";

/** A round colour sample; `rainbow` draws all the colours. */
export function Swatch({ color, rainbow, label }: { color?: string; rainbow?: boolean; label?: string }) {
  return (
    <span
      className="h-4 w-4 shrink-0 rounded-full border border-line-strong"
      style={{ background: rainbow ? RAINBOW : color }}
      title={label}
      aria-hidden
    />
  );
}

/** The time now, updated every `stepMs`. */
export function useClock(stepMs = 15_000) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), stepMs);
    return () => window.clearInterval(id);
  }, [stepMs]);
  return now;
}

/** "25 dk sonra", "2 sa 10 dk sonra" for times within the next 12 hours;
 *  `null` further out (the time of day says enough). */
export function useInText() {
  const t = useT();
  return (ms: number, now: number) => {
    const wait = ms - now;
    if (wait <= 0 || wait > 12 * 3_600_000) return null;
    return t("overview.in", { wait: fmtWait(wait, { h: t("sc.live.h"), m: t("sc.live.m") }) });
  };
}
