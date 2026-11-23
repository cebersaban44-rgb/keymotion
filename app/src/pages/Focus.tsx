// "Odak zamanlayıcısı" page (Araçlar): the focus timer. It uses the whole
// keyboard: the key lights show the time left, and (through the live mode)
// the screen can show a card for each phase. It lives here and not under
// the Screen page because its main output is the key lights and it works
// without the live mode; the live mode lists it as one of its sources.

import { Usb } from "lucide-react";
import { InfoBar } from "../components/controls";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { useT } from "../lib/i18n";
import { PomodoroPanel } from "./screen/PomodoroPanel";
import { LiveLightsNotice } from "../components/KeyboardNotices";

export function Focus({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const usb = kb.connection === "usb" && kb.hasCommandChannel;
  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-2">
        <h1 className="page-title">{t("nav.focus")}</h1>
        <p className="page-desc">{t("pomo.body")}</p>
      </header>
      {!usb && (
        <InfoBar tone="warn" icon={<Usb size={20} strokeWidth={1.6} />} title={t("pomo.noUsb")} data="usb" />
      )}
      <LiveLightsNotice />
      <PomodoroPanel kb={kb} usb={usb} />
    </div>
  );
}
