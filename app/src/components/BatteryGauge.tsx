import { useApp } from "../lib/store";
import { percent as fmtPercent } from "../lib/i18n";

/** Ten-segment arc gauge; segments light up from the left. */
export function BatteryGauge({ percent }: { percent: number | null }) {
  // Turkish writes the percent sign first (%94), English last (94%).
  const signFirst = useApp((s) => s.locale) === "tr";
  const segments = 10;
  const lit = percent == null ? 0 : Math.round((percent / 100) * segments);
  const color = percent == null ? "var(--ink-3)" : percent <= 15 ? "var(--danger)" : percent <= 30 ? "var(--warn)" : "var(--ok)";
  const r = 52;
  const start = 200; // degrees, sweep clockwise
  const sweep = 140 * 2;
  const gap = 4;
  const seg = (sweep - gap * (segments - 1)) / segments;

  const arc = (from: number, to: number) => {
    const p = (deg: number) => {
      const rad = ((deg - 90) * Math.PI) / 180;
      return [60 + r * Math.cos(rad), 60 + r * Math.sin(rad)];
    };
    const [x1, y1] = p(from);
    const [x2, y2] = p(to);
    return `M ${x1} ${y1} A ${r} ${r} 0 0 1 ${x2} ${y2}`;
  };

  return (
    <svg viewBox="0 0 120 100" className="w-full max-w-[220px]" role="img" aria-label={percent == null ? "—" : fmtPercent(percent)}>
      {Array.from({ length: segments }, (_, i) => {
        const from = start + i * (seg + gap);
        const on = i < lit;
        return (
          <path
            key={i}
            d={arc(from, from + seg)}
            stroke={on ? color : "var(--line-strong)"}
            strokeWidth={9}
            strokeLinecap="round"
            fill="none"
            style={on ? { filter: `drop-shadow(0 0 4px ${color})` } : undefined}
          />
        );
      })}
      <text x="60" y="66" textAnchor="middle" className="num" fill="var(--ink)" fontSize="26" fontWeight={600}>
        {percent != null && signFirst && (
          <tspan fontSize="13" fill="var(--ink-3)" dx="-2">
            %
          </tspan>
        )}
        <tspan dx={signFirst ? 2 : 0}>{percent == null ? "—" : percent}</tspan>
        {percent != null && !signFirst && (
          <tspan fontSize="13" fill="var(--ink-3)" dx="2">
            %
          </tspan>
        )}
      </text>
    </svg>
  );
}
