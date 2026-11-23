import { AlertTriangle, ArrowUpRight } from "lucide-react";
import { useEffect, useState } from "react";
import type { BackgroundSettings } from "../bindings/BackgroundSettings";
import { friendlyError } from "../lib/errors";
import { useT } from "../lib/i18n";
import { getBackground, openWindowsPage, setBackground } from "../lib/ipc";
import { Row, secondaryButton } from "./BackupCard";
import { Toggle } from "./controls";

/** Keep running in the tray when the window is closed; start with Windows. */
export function BackgroundCard() {
  const t = useT();
  const [settings, setSettings] = useState<BackgroundSettings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getBackground().then(setSettings, console.error);
  }, []);

  const change = (next: BackgroundSettings) => {
    setError(null);
    setBackground(next).then(setSettings, (e) => setError(friendlyError(e, t)));
  };

  // Turned off in Task Manager: Windows skips it, so the switch shows off.
  const startOn = !!settings && settings.startWithWindows && !settings.startTurnedOff;
  // The Store build, turned off in Windows' settings: only they turn it on.
  const locked = !!settings?.startLocked;
  const startHint = locked ? t("bg.start.locked") : settings?.startTurnedOff ? t("bg.start.off") : t("bg.start.hint");

  return (
    <section className="rows">
      <h2 className="section-title">{t("bg.title")}</h2>
      <p className="max-w-[80ch] pb-2 text-[13px] text-ink-2">{t("bg.intro")}</p>
      <Row label={t("bg.tray")} hint={t("bg.tray.hint")}>
        <Toggle
          label={t("bg.tray")}
          checked={settings?.closeToTray ?? true}
          disabled={!settings}
          onChange={(v) => settings && change({ ...settings, closeToTray: v })}
        />
      </Row>
      <Row label={t("bg.start")} hint={startHint}>
        {locked ? (
          <button type="button" className={secondaryButton} onClick={() => void openWindowsPage("startup-apps")}>
            {t("bg.start.openSettings")}
            <ArrowUpRight size={14} aria-hidden />
          </button>
        ) : (
          <Toggle
            label={t("bg.start")}
            checked={startOn}
            disabled={!settings}
            onChange={(v) => settings && change({ ...settings, startWithWindows: v })}
          />
        )}
      </Row>
      {error && (
        <p role="alert" className="flex items-start gap-2 pt-1 text-[13px] text-danger">
          <AlertTriangle size={14} className="mt-0.5 shrink-0" />
          {error}
        </p>
      )}
    </section>
  );
}
