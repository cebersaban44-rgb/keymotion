// "Odak zamanlayıcısı" page (Araçlar): the focus timer. It uses the whole
// keyboard: the key lights show the time left, and (through the live mode)
// the screen can show a card for each phase. It lives here and not under
// the Screen page because its main output is the key lights and it works
// without the live mode; the live mode lists it as one of its sources.

import { Usb } from "lucide-react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { useT } from "../lib/i18n";
import { PomodoroPanel } from "./screen/PomodoroPanel";

export function Focus({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const usb = kb.connection === "usb" && kb.hasCommandChannel;
  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-2">
        <div className="eyebrow">{t("focus.eyebrow")}</div>
        <h1 className="font-display text-[40px] leading-none font-semibold tracking-[-0.035em]">{t("nav.focus")}</h1>
        <p className="max-w-[78ch] text-[13px] text-ink-2">{t("pomo.body")}</p>
      </header>
      {!usb && (
        <div className="flex items-center gap-3 rounded-2xl border border-signal/30 bg-signal/10 px-4 py-3 text-[13px]">
          <Usb size={16} className="text-signal" />
          {t("pomo.noUsb")}
        </div>
      )}
      <PomodoroPanel kb={kb} usb={usb} />
    </div>
  );
}
