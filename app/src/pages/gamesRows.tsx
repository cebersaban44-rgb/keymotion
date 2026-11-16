// Building blocks of the Game session page: a group of settings and a
// switch inside it (Games.tsx, GamesLol.tsx).

import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Toggle } from "../components/controls";

export function Switch({ label, body, on, onChange }: { label: string; body: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex items-start justify-between gap-3">
      <span className="flex flex-col">
        <span className="text-[13px] font-medium">{label}</span>
        <span className="text-[12px] text-ink-3">{body}</span>
      </span>
      <Toggle checked={on} onChange={onChange} label={label} />
    </div>
  );
}

/** One group of settings: a title and what it does, its switch, and what
 *  belongs to it, indented under it (as on the gauges page). */
export function Row({
  icon: Icon,
  title,
  body,
  checked,
  onChange,
  children,
}: {
  icon: LucideIcon;
  title: string;
  body: string;
  checked?: boolean;
  onChange?: (v: boolean) => void;
  children?: ReactNode;
}) {
  const nested = Array.isArray(children) ? children.some(Boolean) : !!children;
  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <div className="flex items-start justify-between gap-3">
        <span className="flex items-start gap-2.5">
          <Icon size={16} className="mt-0.5 shrink-0 text-signal" />
          <span className="flex max-w-[80ch] flex-col">
            <span className="text-[13px] font-semibold">{title}</span>
            <span className="text-[12px] text-ink-3">{body}</span>
          </span>
        </span>
        {onChange && checked !== undefined && <Toggle checked={checked} onChange={onChange} label={title} />}
      </div>
      {nested && <div className="ml-[7px] flex flex-col gap-3 border-l-2 border-line pl-5">{children}</div>}
    </div>
  );
}
