// A program's icon in a list (read from its file, lib/icons.ts), or a
// stand-in while it loads or when the program has none.

import type { LucideIcon } from "lucide-react";
import { useProgramIcon } from "../lib/icons";

export function ProgramIcon({ path, fallback: Fallback, className = "" }: { path: string | null | undefined; fallback: LucideIcon; className?: string }) {
  const url = useProgramIcon(path);
  return url ? (
    <img src={url} alt="" aria-hidden width={16} height={16} className={`h-4 w-4 shrink-0 ${className}`} draggable={false} />
  ) : (
    <Fallback size={15} className={`shrink-0 ${className}`} aria-hidden />
  );
}
