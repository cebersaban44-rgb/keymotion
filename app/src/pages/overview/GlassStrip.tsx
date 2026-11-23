// The keyboard's glass strip at the top of the Overview (design E), drawn
// after the AK980 PRO's own: the 240×135 screen on the left at its real
// size, the mountain picture printed on the glass, and the aluminium knob
// with the brightness ring. Only the screen changes: it shows the picture
// the app put on the GIF page, or the keyboard's clock page when the app
// has not put one there.

import type { CSSProperties } from "react";
import { useT } from "../../lib/i18n";
import { ClockPage } from "../Screen";
import { GifPicture, useGifPage } from "../screen/common";

/** One brightness step on the ring: 5 arcs on a 135 px knob, open at the bottom. */
const R = 63;
const CIRC = 2 * Math.PI * R;
const SEG = 52.78;
const GAP = 6.6;

function ringDash(lit: number): string {
  if (lit <= 0) return `0 ${CIRC}`;
  const parts: number[] = [];
  for (let i = 0; i < lit; i++) parts.push(SEG, i < lit - 1 ? GAP : CIRC - lit * SEG - (lit - 1) * GAP);
  return parts.join(" ");
}

export function GlassStrip({ level, knob }: { level: number | null; knob: boolean }) {
  const t = useT();
  const gif = useGifPage();
  const onKeyboard = !!gif.shown || gif.showsLive;
  // The pointer on the knob: from −135° at 0 to 135° at 5.
  const turn = level == null ? 0 : -135 + level * 54;
  return (
    <div className="glass" role="img" aria-label={t("overview.glass")} data-overview-glass>
      <div className="glass-screen" data-overview-screen-shows={onKeyboard ? "gif" : "clock"}>
        {onKeyboard ? <GifPicture gif={gif} /> : <ClockPage />}
      </div>
      <div className="glass-art" aria-hidden>
        <svg viewBox="0 0 480 120" preserveAspectRatio="xMidYMax meet">
          <g fill="none" stroke="#E9ECF0" strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke">
            <path
              strokeOpacity=".26"
              strokeWidth=".9"
              d="M0 98 C24 92 34 86 52 88 S84 74 104 70 S130 78 146 72 S176 60 196 64 S226 76 248 70 S286 58 306 62 S336 74 356 70 S392 60 414 66 S452 80 480 78"
            />
            <path
              strokeOpacity=".7"
              strokeWidth="1.15"
              d="M0 118 L28 106 L46 110 L74 90 L92 96 L118 66 L132 74 L160 30 L172 44 L182 40 L204 70 L222 64 L244 88 L262 84 L288 60 L300 66 L330 22 L344 40 L356 36 L380 70 L398 64 L424 92 L446 86 L480 108"
            />
            <path
              strokeOpacity=".48"
              strokeWidth=".9"
              d="M160 30 L150 52 L140 66 L132 74 M160 30 L166 46 L160 60 L166 76 M330 22 L320 44 L308 58 L300 66 M330 22 L336 40 L330 54 L338 72 M118 66 L112 80 M288 60 L282 74 M182 40 L188 58"
            />
            <path
              strokeOpacity=".42"
              strokeWidth=".8"
              d="M152 42 l7 -4 M148 50 l9 -5 M146 58 l7 -4 M322 34 l7 -4 M318 42 l9 -5 M314 50 l7 -4 M168 52 l6 -3 M338 46 l6 -3"
            />
            <path strokeOpacity=".75" strokeWidth=".8" d="M58 22h5M60.5 19.5v5M236 14h5M238.5 11.5v5M454 30h4M456 28v4" />
            <path strokeOpacity=".6" strokeWidth=".9" d="M412 14 a9 9 0 1 0 8 13 a7 7 0 0 1 -8 -13 Z" />
          </g>
          <g fill="#E9ECF0">
            <circle cx="112" cy="18" r=".9" fillOpacity=".7" />
            <circle cx="198" cy="28" r=".7" fillOpacity=".5" />
            <circle cx="276" cy="20" r=".8" fillOpacity=".6" />
            <circle cx="20" cy="40" r=".7" fillOpacity=".5" />
            <circle cx="380" cy="30" r=".7" fillOpacity=".5" />
          </g>
        </svg>
      </div>
      {knob && (
        <div className="glass-knob" style={{ "--turn": `${turn}deg` } as CSSProperties} data-overview-knob={level ?? "unknown"}>
          <svg viewBox="0 0 135 135" aria-hidden>
            <circle
              cx="67.5"
              cy="67.5"
              r={R}
              fill="none"
              stroke="rgb(255 255 255 / 0.1)"
              strokeWidth="3"
              strokeDasharray={ringDash(5)}
              transform="rotate(138 67.5 67.5)"
            />
            {level != null && level > 0 && (
              <circle
                cx="67.5"
                cy="67.5"
                r={R}
                fill="none"
                stroke="#FF8A3D"
                strokeWidth="3"
                strokeDasharray={ringDash(level)}
                transform="rotate(138 67.5 67.5)"
                style={{ filter: "drop-shadow(0 0 3px rgb(255 138 61 / 0.7))" }}
              />
            )}
          </svg>
          <div className="cap" />
          <div className="face" />
        </div>
      )}
    </div>
  );
}
