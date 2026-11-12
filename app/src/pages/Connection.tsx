// Bağlantı: how the keyboard is connected, where its mode switch sits, what
// the app can do over each connection, and the keyboard clock. The mode is
// chosen only with the switch on the back of the keyboard; the app cannot
// change it.

import { Bluetooth, Check, Radio, Usb, type LucideIcon } from "lucide-react";
import type { Connection as Conn } from "../bindings/Connection";
import type { Feature } from "../bindings/Feature";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { ClockCard } from "../components/ClockCard";
import { ModeSwitch } from "../components/ModeSwitch";
import { connectionKey, useT, type MessageKey } from "../lib/i18n";

const COLUMNS: Conn[] = ["usb", "dongle", "bluetooth"];

export const CONNECTION_ICON: Record<Conn, LucideIcon> = { usb: Usb, dongle: Radio, bluetooth: Bluetooth };

/** One sentence on what this connection allows. */
export function connectionSummary(c: Conn): MessageKey {
  return c === "usb" ? "conn.summary.usb" : c === "dongle" ? "conn.summary.dongle" : "conn.summary.bluetooth";
}

type Cell = "yes" | "no" | "last";

interface Row {
  label: MessageKey;
  cell: (mode: Conn, features: Feature[]) => Cell;
}

const has = (f: Feature) => (_: Conn, features: Feature[]) => (features.includes(f) ? "yes" : "no") as Cell;

// Rows follow what the app drives in each mode (KeyboardInfo.modes, from
// kb_core::app_capabilities), so a new driver shows up here by itself.
const ROWS: Row[] = [
  { label: "conn.row.typing", cell: () => "yes" },
  { label: "conn.row.lighting", cell: has("lighting") },
  { label: "conn.row.keymap", cell: (_, f) => (f.includes("keymap") && f.includes("macros") ? "yes" : "no") },
  { label: "conn.row.screen", cell: has("screen") },
  { label: "conn.row.clock", cell: has("clock") },
  // Real-time frames exist only on the cable (none over the receiver).
  { label: "conn.row.live", cell: (m, f) => (m === "usb" && f.includes("lighting") ? "yes" : "no") },
  {
    label: "conn.row.battery",
    // No battery reading over USB; the last Bluetooth one is shown instead.
    cell: (m, f) => (f.includes("battery") ? "yes" : m === "usb" ? "last" : "no"),
  },
];

export function Connection({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const Icon = CONNECTION_ICON[kb.connection];
  const channel = kb.bleName?.match(/-(\d)$/)?.[1];
  const featuresOf = (c: Conn) => kb.modes.find((m) => m.connection === c)?.features ?? [];
  const columns = COLUMNS.filter((c) => kb.modes.some((m) => m.connection === c));

  // The model name, not the USB product string (which names the chip maker).
  const details: [MessageKey, string, string?][] = [["overview.mode.device", kb.modelName]];
  if (kb.firmware) details.push(["conn.firmware", kb.firmware, "num"]);
  if (kb.bleName) details.push(["overview.mode.btName", kb.bleName]);
  if (channel) details.push(["overview.mode.channel", `${channel} / 3`, "num"]);
  if (kb.bleAddress) details.push(["overview.mode.btAddress", kb.bleAddress, "mono text-[12px]"]);

  return (
    <div className="flex flex-col gap-6" data-connection>
      <header className="flex flex-col gap-2">
        <h1 className="page-title">{t("nav.connection")}</h1>
        <p className="page-desc">{t("conn.desc")}</p>
      </header>

      <section className="card flex flex-wrap items-start justify-between gap-x-8 gap-y-5 p-5">
        <div className="flex min-w-[280px] flex-1 flex-col gap-1">
          <span className="text-[12px] text-ink-3">{t("conn.current")}</span>
          <span className="flex items-center gap-2.5 text-[20px] font-semibold" data-connection-current={kb.connection}>
            <Icon size={20} className="text-signal" aria-hidden />
            {t(connectionKey(kb.connection))}
          </span>
          <p className="text-[13px] text-ink-2">{t(connectionSummary(kb.connection))}</p>
          {details.length > 0 && (
            <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-[13px]">
              {details.map(([label, value, cls]) => (
                <div key={label} className="contents">
                  <dt className="text-ink-3">{t(label)}</dt>
                  <dd className={cls}>{value}</dd>
                </div>
              ))}
            </dl>
          )}
        </div>
        <div className="flex w-[260px] flex-col gap-2">
          <span className="text-[12px] text-ink-3">{t("conn.switch")}</span>
          <ModeSwitch value={kb.connection} />
          <span className="text-[12px] text-ink-3">{t("conn.switch.hint")}</span>
        </div>
      </section>

      <section className="card flex flex-col gap-3 p-5" data-connection-map>
        <h2 className="eyebrow">{t("conn.map.title")}</h2>
        <table className="w-full table-fixed border-collapse text-[13px]">
          <thead>
            <tr>
              <th className="w-[44%] pb-2 text-left font-normal" />
              {columns.map((c) => (
                <th
                  key={c}
                  scope="col"
                  className={`pb-2 text-center text-[12px] font-normal ${c === kb.connection ? "text-ink" : "text-ink-3"}`}
                >
                  <span className="block font-medium">{t(connectionKey(c))}</span>
                  {c === kb.connection && <span className="block text-[11px] text-signal">{t("conn.now")}</span>}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {ROWS.map((row) => (
              <tr key={row.label} className="border-t border-line">
                <th scope="row" className="py-2.5 pr-3 text-left font-normal">
                  {t(row.label)}
                </th>
                {columns.map((c) => {
                  const v = row.cell(c, featuresOf(c));
                  return (
                    <td
                      key={c}
                      className={`py-2.5 text-center ${c === kb.connection ? "bg-surface-2/60" : ""}`}
                      data-cell={`${row.label}:${c}:${v}`}
                    >
                      {v === "yes" ? (
                        <Check size={15} strokeWidth={2.5} className="inline text-ok" aria-label={t("conn.cell.yes")} />
                      ) : v === "last" ? (
                        <span className="text-[12px] text-ink-2">{t("conn.cell.last")}</span>
                      ) : (
                        <span className="text-ink-3" aria-label={t("conn.cell.no")}>
                          —
                        </span>
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
        <div className="flex flex-col gap-1 text-[12px] text-ink-3">
          <p>{t("conn.note.live")}</p>
          <p>{t("conn.note.modes")}</p>
        </div>
      </section>

      {(kb.features.includes("clock") || featuresOf("usb").includes("clock")) && <ClockCard usb={kb.features.includes("clock")} />}
    </div>
  );
}
