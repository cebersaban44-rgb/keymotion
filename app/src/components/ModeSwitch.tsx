import type { Connection } from "../bindings/Connection";
import { connectionKey, useT } from "../lib/i18n";

// Order matches the physical slide switch on the back of the AK980 PRO,
// left to right as printed: 2.4G · USB · BT (verified from a photo).
const POSITIONS: Connection[] = ["dongle", "usb", "bluetooth"];

/** Drawing of the hardware slide switch at its current position. It is not
 *  a control: the mode changes only with the switch on the keyboard. */
export function ModeSwitch({ value }: { value: Connection }) {
  const t = useT();
  return (
    <div
      role="img"
      aria-label={t("conn.switch.at", { mode: t(connectionKey(value, true)) })}
      className="grid grid-cols-3 overflow-hidden rounded-md border border-line-strong"
      data-mode-switch={value}
    >
      {POSITIONS.map((p) => (
        <span
          key={p}
          className={`num py-1.5 text-center text-[12px] ${
            p === value ? "bg-surface-2 font-semibold text-ink" : "text-ink-3"
          } ${p !== POSITIONS[0] ? "border-l border-line" : ""}`}
        >
          {t(connectionKey(p, true))}
        </span>
      ))}
    </div>
  );
}
