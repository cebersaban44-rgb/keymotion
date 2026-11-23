// The app's controls in the Windows 11 look (design A+E): switch, slider
// with notches, segmented choice, tabs, colour grid, settings rows and the
// info bar. The visual content (keyboard drawing, effect previews, screen
// pictures) is drawn by the pages themselves.

import { Check, ChevronRight, Info, TriangleAlert, CircleCheck, CircleX, type LucideIcon } from "lucide-react";
import { useRef, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { useT } from "../lib/i18n";
import { keyTone, parseColor } from "../lib/keyColor";
import { PALETTE } from "../lib/lighting";

/**
 * The keyboard's 0–5 levels as a notched slider (Windows' slider with tick
 * marks): one notch per level.
 */
export function Steps({
  value,
  onChange,
  label,
  max = 5,
  disabled,
}: {
  value: number;
  onChange: (v: number) => void;
  label: string;
  max?: number;
  disabled?: boolean;
}) {
  const pct = (value / max) * 100;
  return (
    <div className="flex flex-col" data-steps>
      <input
        type="range"
        className="range"
        min={0}
        max={max}
        step={1}
        value={value}
        disabled={disabled}
        aria-label={label}
        aria-valuetext={`${value} / ${max}`}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ "--pct": `calc(10px + (100% - 20px) * ${pct / 100})` } as CSSProperties}
      />
      <Ticks count={max + 1} />
    </div>
  );
}

/** Tick marks under a notched slider. */
function Ticks({ count }: { count: number }) {
  return (
    <span className="ticks" aria-hidden>
      {Array.from({ length: count }, (_, i) => (
        <i key={i} />
      ))}
    </span>
  );
}

/** Windows' toggle switch. `state` shows "On"/"Off" to its left, as Windows Settings does. */
export function Toggle({
  checked,
  onChange,
  label,
  disabled,
  state,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  disabled?: boolean;
  state?: boolean;
}) {
  const t = useT();
  const sw = (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="tog"
    />
  );
  if (!state) return sw;
  return (
    <span className={`flex items-center gap-3 text-[14px] ${disabled ? "text-ink-3" : ""}`}>
      <span aria-hidden>{t(checked ? "overview.on" : "overview.off")}</span>
      {sw}
    </span>
  );
}

/**
 * The app's colour picker, like Windows' accent colour grid: the shared
 * palette as squares (named for screen readers, the chosen one ticked),
 * then "Custom color" and its code. `inline` lays it out in one row for a
 * toolbar.
 */
export function ColorPicker({
  value,
  onChange,
  disabled,
  label,
  inline,
}: {
  value: string;
  onChange: (hex: string) => void;
  disabled?: boolean;
  label: string;
  inline?: boolean;
}) {
  const t = useT();
  const current = value.toLowerCase();
  const custom = !PALETTE.some((p) => p.hex === current);
  return (
    <div
      className={`${inline ? "flex flex-wrap items-center gap-x-3 gap-y-2" : "flex flex-col gap-2.5"} ${
        disabled ? "pointer-events-none opacity-40" : ""
      }`}
      data-color-picker
    >
      <div
        className={inline ? "flex flex-wrap gap-1.5" : "grid grid-cols-[repeat(6,minmax(0,40px))] gap-1.5"}
        role="radiogroup"
        aria-label={label}
      >
        {PALETTE.map((p) => {
          const on = current === p.hex;
          return (
            <button
              key={p.hex}
              type="button"
              role="radio"
              aria-checked={on}
              aria-label={t(p.name)}
              title={t(p.name)}
              onClick={() => onChange(p.hex)}
              className={`${inline ? "h-7 w-7" : "aspect-square"} grid place-items-center rounded-md transition-[filter] hover:brightness-110`}
              style={{
                background: p.hex,
                boxShadow: on
                  ? "0 0 0 2px var(--surface), 0 0 0 4px var(--ink)"
                  : "inset 0 0 0 1px rgb(0 0 0 / 0.12)",
              }}
            >
              {on && <Check size={inline ? 13 : 16} strokeWidth={2.4} color={keyTone(parseColor(p.hex)) === "deep" ? "#ffffff" : "#16171a"} aria-hidden />}
            </button>
          );
        })}
      </div>
      <div className="flex items-center gap-2.5">
        <label
          className={`btn-secondary relative ${inline ? "!h-7 !px-2.5 text-[13px]" : ""}`}
          style={custom ? { boxShadow: "0 0 0 2px var(--surface), 0 0 0 4px var(--ink)" } : undefined}
        >
          <span className="h-4 w-4 shrink-0 rounded-[3px] shadow-[0_0_0_1px_var(--line-strong)]" style={{ background: value }} aria-hidden />
          {t("color.custom")}
          <input
            type="color"
            value={value}
            onChange={(e) => onChange(e.target.value)}
            className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
            aria-label={`${label}: ${t("color.custom")}`}
          />
        </label>
        <input
          type="text"
          value={value.toUpperCase()}
          spellCheck={false}
          onChange={(e) => {
            const v = e.target.value.trim();
            if (/^#?[0-9a-fA-F]{6}$/.test(v)) onChange(`#${v.replace("#", "").toLowerCase()}`);
          }}
          className={`mono ${inline ? "h-7 w-[88px] text-[12px]" : "h-8 w-28 text-[13px]"} rounded-md border border-line bg-bg-raise px-2.5 outline-none`}
          aria-label={`${label} HEX`}
        />
      </div>
    </div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[14px]">{label}</span>
        {hint && <span className="num text-[13px] text-ink-2">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

/** Single choice in a row of options (Windows' segmented control). An
 *  option's `lang` marks a label in another language (the language picker),
 *  so it gets that language's font. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
  disabled,
}: {
  value: T;
  options: { value: T; label: string; icon?: ReactNode; lang?: string }[];
  onChange: (v: T) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <div role="radiogroup" aria-label={label} aria-disabled={disabled} className={`seg ${disabled ? "opacity-40" : ""}`}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          lang={o.lang}
          disabled={disabled}
          onClick={() => onChange(o.value)}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

/**
 * Tabs of a page (Windows' selector bar): words with a short accent line
 * under the chosen one. Left and right arrows move between them.
 */
export function Tabs<T extends string>({
  value,
  options,
  onChange,
  label,
  className = "",
  data,
}: {
  value: T;
  options: { value: T; label: string; icon?: LucideIcon; data?: string }[];
  onChange: (v: T) => void;
  label: string;
  className?: string;
  data?: string;
}) {
  const list = useRef<HTMLDivElement>(null);
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    const i = options.findIndex((o) => o.value === value);
    const next = options[(i + (e.key === "ArrowRight" ? 1 : options.length - 1)) % options.length];
    if (!next) return;
    e.preventDefault();
    onChange(next.value);
    window.requestAnimationFrame(() =>
      list.current?.querySelector<HTMLElement>(`[data-tab-value="${CSS.escape(next.value)}"]`)?.focus(),
    );
  };
  return (
    <div ref={list} role="tablist" aria-label={label} className={`tabs ${className}`} onKeyDown={onKey} data-tabs={data}>
      {options.map(({ value: v, label: l, icon: Icon, data: d }) => (
        <button
          key={v}
          type="button"
          role="tab"
          aria-selected={v === value}
          tabIndex={v === value ? 0 : -1}
          onClick={() => onChange(v)}
          data-tab-value={v}
          {...(d ? { [`data-${d}`]: v } : {})}
        >
          {Icon && <Icon size={15} strokeWidth={1.6} aria-hidden />}
          {l}
        </button>
      ))}
    </div>
  );
}

/** Labelled range input (Windows' slider); the filled part uses `accent`.
 *  `ticks` draws a notch for each step. */
export function Slider({
  label,
  hint,
  value,
  min,
  max,
  step,
  onChange,
  format = (v) => String(v),
  accent = "var(--signal)",
  disabled,
  ticks,
}: {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  format?: (v: number) => string;
  accent?: string;
  disabled?: boolean;
  ticks?: boolean;
}) {
  const share = (value - min) / (max - min);
  return (
    <label className={`flex flex-col gap-1 ${disabled ? "opacity-45" : ""}`}>
      <span className="flex items-baseline justify-between gap-3">
        <span className="text-[14px]">
          {label}
          {hint && <span className="ml-2 text-[13px] text-ink-2">{hint}</span>}
        </span>
        <span className="num text-[13px] text-ink-2">{format(value)}</span>
      </span>
      <input
        type="range"
        className="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        aria-valuetext={format(value)}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ "--pct": `calc(10px + (100% - 20px) * ${share})`, "--fill": accent } as CSSProperties}
      />
      {ticks && <Ticks count={Math.round((max - min) / step) + 1} />}
    </label>
  );
}

// ------------------------------------------------------------------ rows

/** Rows under one heading (Windows Settings' groups of cards). */
export function Rows({ title, aside, children, data }: { title?: ReactNode; aside?: ReactNode; children: ReactNode; data?: string }) {
  return (
    <section className="flex flex-col" data-rows={data}>
      {title && (
        <h2 className="section-title">
          <span>{title}</span>
          {aside}
        </h2>
      )}
      <div className="rows">{children}</div>
    </section>
  );
}

/**
 * One full-width settings row: an icon, a title, a line under it, and the
 * control on the right (`children`). With `onOpen` the row opens a page:
 * the whole row is the button and a chevron shows it; a control in the row
 * still works on its own.
 */
export function Row({
  icon: Icon,
  lead,
  title,
  desc,
  children,
  onOpen,
  openLabel,
  data,
  className = "",
}: {
  icon?: LucideIcon;
  /** Something drawn instead of an icon (a coloured dot, a tile). */
  lead?: ReactNode;
  title: ReactNode;
  desc?: ReactNode;
  children?: ReactNode;
  onOpen?: () => void;
  /** Accessible name of the row's button, when the title is not text. */
  openLabel?: string;
  data?: string;
  className?: string;
}) {
  const head = (
    <>
      {(Icon || lead) && <span className="row-icon">{Icon ? <Icon size={20} strokeWidth={1.4} aria-hidden /> : lead}</span>}
      <span className="row-text">
        <span className="row-title">{title}</span>
        {desc && <span className="row-desc">{desc}</span>}
      </span>
    </>
  );
  if (onOpen && !children)
    return (
      <button type="button" className={`row ${className}`} onClick={onOpen} data-row={data}>
        {head}
        <ChevronRight size={16} className="shrink-0 text-ink-2" aria-hidden />
      </button>
    );
  if (onOpen)
    return (
      <div className={`row ${className}`} data-row={data}>
        <button
          type="button"
          className="absolute inset-0 rounded-[inherit] transition-colors hover:bg-[var(--subtle-hover)]"
          onClick={onOpen}
          aria-label={openLabel ?? (typeof title === "string" ? title : undefined)}
        />
        <span className="pointer-events-none relative flex min-w-0 flex-1 items-center gap-4">{head}</span>
        <span className="row-end relative">{children}</span>
        <ChevronRight size={16} className="pointer-events-none relative shrink-0 text-ink-2" aria-hidden />
      </div>
    );
  return (
    <div className={`row ${className}`} data-row={data}>
      {head}
      {children && <span className="row-end">{children}</span>}
    </div>
  );
}

// --------------------------------------------------------------- info bar

const TONE_ICON = { info: Info, warn: TriangleAlert, error: CircleX, ok: CircleCheck } as const;
const TONE_COLOR = { info: "text-signal", warn: "text-warn", error: "text-danger", ok: "text-ok" } as const;

/**
 * Windows' info bar: an icon, a bold title and a line, then the buttons.
 * `floating` keeps it in view at the bottom of the page (the save bar).
 */
export function InfoBar({
  tone = "info",
  icon,
  title,
  children,
  actions,
  floating,
  live,
  data,
  className = "",
}: {
  tone?: keyof typeof TONE_ICON;
  icon?: ReactNode;
  title?: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  floating?: boolean;
  /** Read out when it changes (role="status"). */
  live?: boolean;
  data?: string;
  className?: string;
}) {
  const Icon = TONE_ICON[tone];
  return (
    <div className={`infobar ${floating ? "floating sticky bottom-4 z-20" : ""} ${className}`} data-tone={tone} data-infobar={data}>
      <span className={`flex shrink-0 self-start pt-0.5 ${TONE_COLOR[tone]}`} aria-hidden>
        {icon ?? <Icon size={20} strokeWidth={1.6} />}
      </span>
      <div className="flex min-w-0 flex-1 flex-col" role={live ? "status" : undefined}>
        {title && <span className="text-[14px] font-semibold">{title}</span>}
        {children && <span className="text-[13px] text-ink-2">{children}</span>}
      </div>
      {actions && <div className="flex shrink-0 flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}
