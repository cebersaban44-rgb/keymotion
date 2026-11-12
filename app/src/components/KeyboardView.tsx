import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { Connection } from "../bindings/Connection";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import { useT } from "../lib/i18n";
import { colorNameKey, keyTone, parseColor } from "../lib/keyColor";
import { useApp } from "../lib/store";

interface Props {
  geometry: GeometryInfo;
  battery?: number | null;
  connection: Connection;
  /**
   * led → CSS colour ("#rrggbb" or "rgb(r,g,b)") the key is lit with. Lit
   * keys take the colour on the whole cap; keys missing from the map (or
   * black) are drawn unlit. `null`/`undefined` plays an idle rainbow, a
   * stand-in when the real lights are unknown.
   */
  colors?: Record<number, string> | null;
  /**
   * "lights" (default): the drawing shows key lights. "plain": every cap
   * unlit and no colour in the key names (keymap).
   */
  look?: "lights" | "plain";
  /** Small preview (focus timer, gauges): plain screen, own plate. */
  compact?: boolean;
  /** Accessible name of the drawing when its keys are not buttons. */
  label?: string;
  /**
   * Makes keys interactive. `press` on pointer down (or Enter/Space on a
   * focused key), `drag` when the pointer enters a key with the button
   * held. `additive` is true while Ctrl, Shift or ⌘ is held.
   */
  onKeyAction?: (led: number, action: KeyAction, additive: boolean) => void;
  /** Keys drawn with a selection ring. */
  selected?: ReadonlySet<number>;
  /** Pointer and ARIA semantics: painting (default) or selecting keys. */
  tool?: "brush" | "select";
  /**
   * led → legend shown instead of the keycap label, with the original
   * label small underneath (keymap view).
   */
  legends?: Record<number, Legend>;
  /** led → extra class on the key (`is-pending`, `is-locked`, `is-pressed`, …). */
  keyClass?: Record<number, string>;
}

export interface Legend {
  main: ReactNode;
  /** Text for assistive technology when `main` is an icon. */
  text: string;
  sub?: ReactNode;
}

export type KeyAction = "press" | "drag";

/** Legend size as a share of one key unit; hidden when it would be under 9 px. */
const LEGEND = 0.3;
const SUB = 0.22;
const MIN_PX = 9;

type LegendSize = "full" | "main" | "none";

/** Ticks once a second; kept local so only the mini screen re-renders. */
function useNow() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1_000);
    return () => clearInterval(id);
  }, []);
  return now;
}

/** Which legends fit, from the drawing's width. Changes only at the thresholds. */
function useLegendSize(ref: React.RefObject<HTMLDivElement | null>, units: number): LegendSize {
  const [size, setSize] = useState<LegendSize>("full");
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const u = el.clientWidth / units;
      setSize(u * SUB >= MIN_PX ? "full" : u * LEGEND >= MIN_PX ? "main" : "none");
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref, units]);
  return size;
}

type Vars = CSSProperties & Record<`--${string}`, string | number>;

/**
 * Top-down drawing of the keyboard from the model's measured geometry, the
 * one used everywhere in the app. A lit key's cap takes its light colour
 * (legend dark or light by luminance); an unlit key is a dark cap.
 */
export function KeyboardView({
  geometry: g,
  battery,
  connection,
  colors,
  look = "lights",
  compact = false,
  label,
  onKeyAction,
  selected,
  tool = "brush",
  legends,
  keyClass,
}: Props) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const stageRef = useRef<HTMLDivElement>(null);
  const legendSize = useLegendSize(stageRef, g.width);
  const plain = look === "plain";
  const idle = !plain && colors == null;
  const stage: Vars = { "--u": `calc(100cqw / ${g.width})`, "--rows": g.height };
  const paintable = !!onKeyAction;
  const additive = (e: { ctrlKey: boolean; shiftKey: boolean; metaKey: boolean }) => e.ctrlKey || e.shiftKey || e.metaKey;
  const at = (x: number, y: number, w: number, h: number): CSSProperties => ({
    left: `calc(${x} * var(--u))`,
    top: `calc(${y} * var(--u))`,
    width: `calc(${w} * var(--u))`,
    height: `calc(${h} * var(--u))`,
  });

  const keys = g.keys.map((k) => {
    const name = k.label || k.id;
    const rgb = plain || idle ? null : parseColor(colors?.[k.led]);
    const tone = idle ? "idle" : keyTone(rgb);
    const style: Vars = { "--x": k.x, "--y": k.y, "--w": k.w, "--h": k.h };
    // Even a very dim light tints the cap (a draining timer's faint track).
    if (rgb) style["--c"] = `rgb(${rgb[0]} ${rgb[1]} ${rgb[2]})`;
    const legend = legends?.[k.led];
    let aria = name;
    if (legend) aria = t("kb.key.action", { key: name, action: legend.text });
    else if (!plain && !idle) {
      const color = colorNameKey(rgb);
      aria = color ? t("kb.key.color", { key: name, color: t(color).toLocaleLowerCase(locale) }) : t("kb.key.off", { key: name });
    }
    const long = k.label.length >= 4 && k.w <= 1;
    const cls = [
      "kb-key",
      `tone-${tone}`,
      long ? "kb-long" : "",
      selected?.has(k.led) ? "is-selected" : "",
      keyClass?.[k.led] ?? "",
    ]
      .filter(Boolean)
      .join(" ");
    return { k, aria, style, cls, legend };
  });

  const drawing = (
    <div
      className={[
        "kb",
        idle ? "kb-idle" : "",
        paintable ? "kb-paint" : "",
        tool === "select" ? "kb-select" : "",
        compact ? "kb-compact" : "",
        legendSize === "none" ? "kb-nolegend" : legendSize === "main" ? "kb-nosub" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      style={stage}
    >
      {keys.map(({ k, aria, style, cls, legend }) =>
        paintable ? (
          <button
            key={k.id}
            type="button"
            className={cls}
            style={style}
            data-key={k.id}
            aria-label={aria}
            aria-pressed={tool === "select" ? (selected?.has(k.led) ?? false) : undefined}
            onPointerDown={(e) => {
              e.preventDefault();
              onKeyAction(k.led, "press", additive(e));
            }}
            onPointerEnter={(e) => {
              if (e.buttons & 1) onKeyAction(k.led, "drag", true);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onKeyAction(k.led, "press", additive(e));
              }
            }}
          >
            <KeyLegend label={k.label} legend={legend} />
          </button>
        ) : (
          <div key={k.id} className={cls} style={style} data-key={k.id}>
            <KeyLegend label={k.label} legend={legend} />
          </div>
        ),
      )}
      {g.screen && (
        <div className="kb-screen" style={at(g.screen.x + 0.06, g.screen.y + 0.06, g.screen.w - 0.12, g.screen.h - 0.12)}>
          {!compact && <MiniScreen battery={battery ?? null} connection={connection} />}
        </div>
      )}
      {g.knob && (
        <div className="kb-knob" style={at(g.knob.x + 0.08, g.knob.y + 0.08, g.knob.d - 0.16, g.knob.d - 0.16)} />
      )}
    </div>
  );

  const a11y = {
    role: !paintable && label ? "img" : undefined,
    "aria-label": !paintable ? label : undefined,
    "aria-hidden": !paintable && !label ? true : undefined,
  };
  const stageEl = (
    <div ref={stageRef} className="kb-stage" {...(compact ? {} : a11y)}>
      {drawing}
    </div>
  );
  // A compact preview brings its own plate; the full drawing sits on the
  // page's plate.
  return compact ? (
    <div className="kb-plate-compact" {...a11y}>
      {stageEl}
    </div>
  ) : (
    stageEl
  );
}

function KeyLegend({ label, legend }: { label: string; legend?: Legend }) {
  if (!legend) return <span className="kb-cap">{label}</span>;
  return (
    <>
      <span className="kb-legend">{legend.main}</span>
      {legend.sub !== undefined && <span className="sub">{legend.sub}</span>}
    </>
  );
}

const pad = (n: number) => String(n).padStart(2, "0");

/**
 * Imitation of the stock home page, matched to a photo of the real panel
 * (2026-11-10): blue status card on the left with date, charge state,
 * clock with seconds and the status chips (WIN, USB, 2.4G, BT1, Win lock,
 * Caps, Num); line-art mountains on black to the right.
 */
function MiniScreen({ battery, connection }: { battery: number | null; connection: Connection }) {
  const now = useNow();
  const date = `${now.getFullYear()}/${pad(now.getMonth() + 1)}/${pad(now.getDate())}`;
  const time = `${pad(now.getHours())}: ${pad(now.getMinutes())}: ${pad(now.getSeconds())}`;
  const level = battery == null ? 4 : Math.max(1, Math.round(battery / 25));
  const mono = "JetBrains Mono Variable, monospace";
  // Lining serif digits like the panel's clock (Georgia uses old-style figures).
  const clockFont = "'Times New Roman', Times, serif";

  // Chip colours as on the panel; the active connection is filled.
  const chips: { label: string; color: string; active: boolean }[] = [
    { label: "WIN", color: "#5fe3ff", active: true },
    { label: "USB", color: "#8a6bff", active: connection === "usb" },
    { label: "2.4", color: "#7ee08a", active: connection === "dongle" },
    { label: "BT1", color: "#3aa0ff", active: connection === "bluetooth" },
    { label: "lock", color: "#e8d24a", active: false },
    { label: "A", color: "#e8d24a", active: false },
    { label: "1", color: "#e8d24a", active: false },
  ];

  return (
    <svg viewBox="0 0 300 100" width="100%" height="100%" preserveAspectRatio="xMidYMid slice">
      <defs>
        <linearGradient id="status" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#123a73" />
          <stop offset="1" stopColor="#0a2550" />
        </linearGradient>
      </defs>
      <rect width="300" height="100" fill="#020305" />
      <rect x="3" y="4" width="146" height="92" rx="6" fill="url(#status)" />
      <text x="12" y="22" fill="#7fd3ff" fontFamily={mono} fontSize="11">
        {date}
      </text>
      {/* charge bolt + battery */}
      <path d="M113 12 l-4 7 h3 l-2 6 l5 -8 h-3 l2 -5 z" fill="#5fe3ff" />
      <rect x="120" y="12" width="20" height="10" rx="1.5" fill="none" stroke="#5fe3ff" strokeWidth="1.3" />
      <rect x="140.5" y="15" width="1.6" height="4" fill="#5fe3ff" />
      {Array.from({ length: 4 }, (_, i) => (
        <rect key={i} x={122 + i * 4.3} y="14" width="3.2" height="6" fill={i < level ? "#5fe3ff" : "#163a5a"} />
      ))}
      <text x="76" y="56" textAnchor="middle" fill="#f4f8ff" fontFamily={clockFont} fontSize="26">
        {time}
      </text>
      {chips.map((c, i) => (
        <g key={c.label} transform={`translate(${9 + i * 19.6} 72)`}>
          <rect
            width="17"
            height="16"
            rx="3"
            fill={c.active ? c.color : "none"}
            fillOpacity={c.active ? 0.85 : 0}
            stroke={c.color}
            strokeWidth="1.3"
          />
          {c.label === "lock" ? (
            // Win-lock padlock: shackle + body.
            <g fill="none" stroke={c.active ? "#071426" : c.color} strokeWidth="1.2">
              <path d="M6 7.5 V5.6 a2.5 2.5 0 0 1 5 0 V7.5" />
              <rect x="4.6" y="7.5" width="7.8" height="5.6" rx="1" />
            </g>
          ) : (
            <text
              x="8.5"
              y="11.5"
              textAnchor="middle"
              fill={c.active ? "#071426" : c.color}
              fontFamily={mono}
              fontSize={c.label.length > 2 ? 6 : 9}
              fontWeight="700"
            >
              {c.label}
            </text>
          )}
        </g>
      ))}
      {/* line-art mountains and birds, as in the stock artwork */}
      <g transform="translate(156 0)" stroke="#d6d9dc" strokeWidth="1.1" fill="none" strokeLinejoin="round">
        <path d="M4 92 L26 48 L36 58 L56 22 L70 44 L84 30 L104 62 L118 40 L136 76" />
        <path d="M26 48 L30 66 L40 74 M56 22 L50 46 L62 64 L74 90 M84 30 L80 52 L94 72 M118 40 L112 60 L124 82" opacity="0.7" />
        <path d="M40 92 L64 70 L88 92" opacity="0.6" />
        <path d="M70 14 l3 2 l3 -2 M96 10 l3 2 l3 -2 M118 18 l2.5 1.6 l2.5 -1.6" strokeWidth="0.9" />
      </g>
    </svg>
  );
}
