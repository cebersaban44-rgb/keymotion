import { Check, Copy } from "lucide-react";
import { useState } from "react";
import type { ThemePreference } from "../bindings/ThemePreference";
import { BackgroundCard } from "../components/BackgroundCard";
import { BackupCard, Row, secondaryButton } from "../components/BackupCard";
import { CalibrationCard } from "../components/CalibrationCard";
import { Segmented } from "../components/controls";
import { copyDiagnostics } from "../lib/diagnostics";
import { useT, type Locale, type MessageKey } from "../lib/i18n";
import { openFolder } from "../lib/ipc";
import { useApp } from "../lib/store";

export function Settings() {
  const t = useT();
  const { theme, setTheme, locale, setLocale, env } = useApp();
  // The order Windows uses.
  const themes: ThemePreference[] = ["light", "dark", "system"];
  // An import may replace the calibration: its card reads it again.
  const [imports, setImports] = useState(0);
  const [copied, setCopied] = useState<"no" | "busy" | "yes" | "failed">("no");

  const copy = () => {
    setCopied("busy");
    copyDiagnostics().then(
      () => {
        setCopied("yes");
        setTimeout(() => setCopied((c) => (c === "yes" ? "no" : c)), 2500);
      },
      (e) => {
        console.error(e);
        setCopied("failed");
      },
    );
  };

  return (
    <div className="flex max-w-[760px] flex-col gap-6">
      <header className="flex flex-col gap-2">
        <h1 className="page-title">{t("nav.settings")}</h1>
        <p className="page-desc">{t("settings.desc")}</p>
      </header>

      <section className="card px-5 py-1">
        <div className="eyebrow pt-4 pb-1">{t("settings.appearance")}</div>
        <Row label={t("settings.theme")}>
          <Segmented
            label={t("settings.theme")}
            value={theme}
            onChange={setTheme}
            options={themes.map((v) => ({ value: v, label: t(`settings.theme.${v}` as MessageKey) }))}
          />
        </Row>
        <Row label={t("settings.language")}>
          <Segmented<Locale>
            label={t("settings.language")}
            value={locale}
            onChange={setLocale}
            options={[
              { value: "tr", label: "Türkçe" },
              { value: "en", label: "English" },
            ]}
          />
        </Row>
      </section>

      <BackgroundCard />

      <CalibrationCard key={imports} />

      <BackupCard onImported={() => setImports((n) => n + 1)} />

      <section className="card px-5 py-1">
        <div className="eyebrow pt-4 pb-1">{t("settings.about")}</div>
        <Row label={t("settings.version")}>
          <span className="num text-[13px] text-ink-2">{env?.appVersion ?? "—"}</span>
        </Row>
        <Row label={t("settings.backdrop")}>
          <span className="num text-[13px] text-ink-2" title={env && env.osBuild > 0 ? t("settings.build", { build: env.osBuild }) : undefined}>
            {env?.mica ? t("settings.backdrop.mica") : t("settings.backdrop.solid")}
          </span>
        </Row>
        <Row
          label={t("data.diag")}
          hint={copied === "yes" ? t("data.diag.copied") : copied === "failed" ? t("data.diag.failed") : t("data.diag.hint")}
        >
          <button type="button" onClick={copy} disabled={copied === "busy"} className={secondaryButton}>
            {copied === "yes" ? <Check size={14} /> : <Copy size={14} />}
            {t("data.diag.copyShort")}
          </button>
        </Row>
        <Row label={t("data.logs")} hint={t("data.logs.hint")}>
          <button type="button" onClick={() => openFolder("logs").catch(console.error)} className={secondaryButton}>
            {t("data.open")}
          </button>
        </Row>
        <Row label={t("data.folder")} hint={t("data.folder.hint")}>
          <button type="button" onClick={() => openFolder("data").catch(console.error)} className={secondaryButton}>
            {t("data.open")}
          </button>
        </Row>
        <div className="flex flex-col gap-0.5 border-t border-line py-4">
          <span className="text-[13.5px]">{t("data.privacy")}</span>
          <p className="text-[12.5px] text-ink-2">{t("data.privacy.body")}</p>
        </div>
      </section>
    </div>
  );
}
