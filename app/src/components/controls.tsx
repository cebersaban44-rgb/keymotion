import type { CSSProperties, ReactNode } from "react";
import { SWATCHES } from "../lib/lighting";

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
                boxShadow: on && i === value ? "0 0 10px var(--signal)" : undefined,
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

export function ColorPicker({
  value,
  onChange,
  disabled,
  label,
}: {
  value: string;
  onChange: (hex: string) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <div className={`flex flex-col gap-2.5 ${disabled ? "pointer-events-none opacity-40" : ""}`}>
      <div className="grid grid-cols-12 gap-1.5" role="radiogroup" aria-label={label}>
        {SWATCHES.map((c) => (
          <button
            key={c}
            type="button"
            role="radio"
            aria-checked={value.toLowerCase() === c}
            aria-label={c}
            onClick={() => onChange(c)}
            className="aspect-square rounded-md border border-black/20 transition-transform hover:scale-110"
            style={{
              background: c,
              boxShadow: value.toLowerCase() === c ? `0 0 0 2px var(--surface), 0 0 0 4px ${c}` : undefined,
            }}
          />
        ))}
      </div>
      <label className="flex items-center gap-2.5">
        <span
          className="relative h-8 w-8 shrink-0 overflow-hidden rounded-lg border border-line-strong"
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
          className="num h-8 w-28 rounded-lg border border-line bg-bg-raise px-2.5 text-[12.5px] outline-none focus:border-signal"
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
        <span className="text-[12.5px] font-medium">{label}</span>
        {hint && <span className="num text-[11px] text-ink-3">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

export type SwatchVariant = "static" | "wave" | "breath" | "sparkle" | "ripple" | "spectrum" | "off";

/** Tiny animated preview of a firmware effect (approximation, not a render). */
export function EffectSwatch({ variant, color, rainbow }: { variant: SwatchVariant; color: string; rainbow: boolean }) {
  return (
    <div className={`fx fx-${variant} ${rainbow ? "fx-rainbow" : ""}`} aria-hidden>
      {Array.from({ length: 14 }, (_, i) => (
        <span key={i} style={{ "--i": i, "--col": i % 7, "--fx-color": color } as CSSProperties} />
      ))}
    </div>
  );
}
