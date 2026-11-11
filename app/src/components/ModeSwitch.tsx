import { motion } from "motion/react";
import type { Connection } from "../bindings/Connection";
import { connectionKey, useT } from "../lib/i18n";

// Order matches the physical slide switch on the back of the AK980 PRO,
// left to right as printed: 2.4G · USB · BT (verified from a photo).
const POSITIONS: Connection[] = ["dongle", "usb", "bluetooth"];

/** Mirror of the hardware slide switch; read-only, it reflects the device. */
export function ModeSwitch({ value }: { value: Connection }) {
  const t = useT();
  const index = Math.max(0, POSITIONS.indexOf(value));
  return (
    <div
      role="img"
      aria-label={t(connectionKey(value))}
      className="relative grid h-12 grid-cols-3 rounded-full border border-line-strong bg-bg-raise p-1"
    >
      {POSITIONS.map((p) => (
        <span key={p} className="num flex items-center justify-center text-[11px] tracking-[0.12em] text-ink-3">
          {t(connectionKey(p, true))}
        </span>
      ))}
      <motion.div
        className="absolute top-1 bottom-1 left-1 flex items-center justify-center gap-2 rounded-full bg-surface-2 shadow-[0_6px_16px_-8px_rgb(0_0_0/0.6),inset_0_1px_0_rgb(255_255_255/0.08),0_0_0_1px_var(--line)]"
        style={{ width: "calc((100% - 8px) / 3)" }}
        initial={false}
        animate={{ x: `${index * 100}%` }}
        transition={{ type: "spring", stiffness: 420, damping: 34 }}
      >
        <span className="h-1.5 w-1.5 rounded-full bg-signal" />
        <span className="num text-[11px] font-semibold tracking-[0.12em] text-ink">{t(connectionKey(value, true))}</span>
      </motion.div>
    </div>
  );
}
