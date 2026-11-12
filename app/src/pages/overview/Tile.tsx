// Pieces the Overview's tiles share: the tile itself (a title, what is on
// now, a way to its page), colour swatches, load bars and a clock that
// ticks often enough for "in 25 min".

import { ArrowRight, type LucideIcon } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { percent, useT } from "../../lib/i18n";
import { fmtWait } from "../../lib/live";

/** A tile under the keyboard: what is set now and a link to its page.
 *  `badge` is a short on/off word in the corner. */
export function Tile({
  icon: Icon,
  title,
  data,
  badge,
  link,
  onLink,
  children,
}: {
  icon: LucideIcon;
  title: string;
  data: string;
  badge?: { on: boolean; text: string } | null;
  link: string;
  onLink: () => void;
  children: ReactNode;
}) {
  return (
    <section className="card flex min-w-0 flex-col gap-3 p-4" data-quick={data}>
      <header className="flex items-center gap-2.5">
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-line bg-bg-raise text-ink-2">
          <Icon size={15} aria-hidden />
        </span>
        <h2 className="min-w-0 flex-1 truncate text-[13px] font-semibold">{title}</h2>
        {badge && (
          <span className="flex shrink-0 items-center gap-1.5 text-[12px] text-ink-3" data-tile-badge={badge.on ? "on" : "off"}>
            <span className={`h-1.5 w-1.5 rounded-full ${badge.on ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
            {badge.text}
          </span>
        )}
      </header>
      <div className="flex min-w-0 flex-1 flex-col gap-2">{children}</div>
      <TileLink text={link} onClick={onLink} />
    </section>
  );
}

export function TileLink({ text, onClick }: { text: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="group mt-auto flex items-center gap-1 self-start rounded-md pt-1 text-[13px] text-signal hover:underline"
    >
      {text}
      <ArrowRight size={13} aria-hidden className="transition-transform group-hover:translate-x-0.5" />
    </button>
  );
}

/** A plain sentence for a tile with nothing to show. */
export function Empty({ children }: { children: ReactNode }) {
  return <p className="text-[13px] text-ink-3">{children}</p>;
}

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

/** One load reading as a short bar: "İşlemci ▇▇▇░░ %32". */
export function LoadBar({ label, value, none }: { label: string; value: number | null; none: string }) {
  const share = value == null ? 0 : Math.max(0, Math.min(100, value));
  return (
    <>
      <span className="truncate text-ink-2">{label}</span>
      <span className="h-1.5 overflow-hidden rounded-full bg-line-strong" aria-hidden>
        <span className="block h-full rounded-full bg-signal transition-[width] duration-300" style={{ width: `${share}%` }} />
      </span>
      <span className="num text-right text-ink">{value == null ? none : percent(Math.round(value))}</span>
    </>
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
