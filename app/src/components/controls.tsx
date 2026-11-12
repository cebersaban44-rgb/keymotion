import type { CSSProperties, ReactNode } from "react";
import { useT } from "../lib/i18n";
import { PALETTE } from "../lib/lighting";

/** Six-step level selector that mirrors the keyboard's 0–5 levels. */
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
  return (
    <div role="radiogroup" aria-label={label} className={`flex gap-1 ${disabled ? "opacity-40" : ""}`}>
      {Array.from({ length: max + 1 }, (_, i) => {
        const on = i <= value;
        return (
          <button
            key={i}
            type="button"
            role="radio"
            aria-checked={i === value}
            aria-label={`${label} ${i}`}
            disabled={disabled}
            onClick={() => onChange(i)}
            className="group flex h-8 flex-1 items-end justify-center rounded-md border border-line bg-bg-raise p-1 transition-colors hover:border-line-strong disabled:cursor-not-allowed"
          >
            <span
              className="block w-full rounded-sm transition-all"
              style={{
                height: `${22 + i * 13}%`,
                background: on ? "var(--signal)" : "var(--line-strong)",
              }}
            />
          </button>
        );
      })}
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 shrink-0 rounded-full border transition-colors disabled:opacity-40 ${
        checked ? "border-signal bg-signal" : "border-line-strong bg-bg-raise"
      }`}
    >
      <span
        className={`absolute top-0.5 h-[18px] w-[18px] rounded-full shadow transition-all ${
          checked ? "left-[22px] bg-signal-ink" : "left-0.5 bg-ink-3"
        }`}
      />
    </button>
  );
}

/**
 * The app's colour picker: the shared palette (named for screen readers),
 * then a custom colour and its code. `inline` lays it out in one row for a
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
  return (
    <div
      className={`${inline ? "flex flex-wrap items-center gap-x-3 gap-y-2" : "flex flex-col gap-2.5"} ${
        disabled ? "pointer-events-none opacity-40" : ""
      }`}
    >
      <div className={inline ? "flex flex-wrap gap-1.5" : "grid grid-cols-12 gap-1.5"} role="radiogroup" aria-label={label}>
        {PALETTE.map((p) => (
          <button
            key={p.hex}
            type="button"
            role="radio"
            aria-checked={current === p.hex}
            aria-label={t(p.name)}
            title={t(p.name)}
            onClick={() => onChange(p.hex)}
            className={`${inline ? "h-7 w-7" : "aspect-square"} rounded-md border border-black/20 transition-transform hover:scale-110`}
            style={{
              background: p.hex,
              boxShadow: current === p.hex ? "0 0 0 2px var(--surface), 0 0 0 4px var(--ink)" : undefined,
            }}
          />
        ))}
      </div>
      <label className="flex items-center gap-2.5">
        <span
          className={`relative ${inline ? "h-7 w-7" : "h-8 w-8"} shrink-0 overflow-hidden rounded-lg border border-line-strong`}
          style={{ background: value }}
        >
          <input
            type="color"
            value={value}
            onChange={(e) => onChange(e.target.value)}
            className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
            aria-label={label}
          />
        </span>
        <input
          type="text"
          value={value.toUpperCase()}
          spellCheck={false}
          onChange={(e) => {
            const v = e.target.value.trim();
            if (/^#?[0-9a-fA-F]{6}$/.test(v)) onChange(`#${v.replace("#", "").toLowerCase()}`);
          }}
          className={`mono ${inline ? "h-7 w-[88px] text-[12px]" : "h-8 w-28 text-[13px]"} rounded-lg border border-line bg-bg-raise px-2.5 outline-none focus:border-signal`}
          aria-label={`${label} HEX`}
        />
      </label>
    </div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between">
        <span className="text-[13px] font-medium">{label}</span>
        {hint && <span className="num text-[11px] text-ink-3">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

/** Pill-shaped single choice (radio group). */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
  disabled,
}: {
  value: T;
  options: { value: T; label: string; icon?: ReactNode }[];
  onChange: (v: T) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      aria-disabled={disabled}
      className={`inline-flex rounded-lg border border-line bg-bg-raise p-1 ${disabled ? "opacity-40" : ""}`}
    >
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          disabled={disabled}
          onClick={() => onChange(o.value)}
          className={`flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-[13px] transition-colors disabled:cursor-not-allowed ${
            o.value === value ? "bg-surface-2 text-ink shadow-[inset_0_0_0_1px_var(--line)]" : "text-ink-2 hover:text-ink"
          }`}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** Labelled range input; the filled part of the track uses `accent`. */
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
}) {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <label className={`flex flex-col gap-1.5 ${disabled ? "opacity-40" : ""}`}>
      <span className="flex items-baseline justify-between gap-3">
        <span className="text-[13px] font-medium">
          {label}
          {hint && <span className="ml-2 font-normal text-ink-3">{hint}</span>}
        </span>
        <span className="num text-[12px] text-ink-2">{format(value)}</span>
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
        style={{ "--pct": `${pct}%`, "--fill": accent } as CSSProperties}
      />
    </label>
  );
}
