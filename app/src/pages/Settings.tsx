import type { ReactNode } from "react";
import type { ThemePreference } from "../bindings/ThemePreference";
import { CalibrationCard } from "../components/CalibrationCard";
import { Segmented } from "../components/controls";
import { useT, type Locale, type MessageKey } from "../lib/i18n";
import { useApp } from "../lib/store";

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-t border-line py-4 first:border-t-0">
      <span className="text-[13.5px]">{label}</span>
      {children}
    </div>
  );
}

export function Settings() {
  const t = useT();
  const { theme, setTheme, locale, setLocale, env } = useApp();
  const themes: ThemePreference[] = ["system", "dark", "light"];
  return (
    <div className="flex max-w-[760px] flex-col gap-6">
      <h1 className="page-title">{t("nav.settings")}</h1>

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

      <CalibrationCard />

      <section className="card px-5 py-1">
        <div className="eyebrow pt-4 pb-1">{t("settings.about")}</div>
        <Row label={t("settings.version")}>
          <span className="num text-[13px] text-ink-2">{env?.appVersion ?? "—"}</span>
        </Row>
        <Row label={t("settings.backdrop")}>
          <span className="num text-[13px] text-ink-2">
            {env?.mica ? t("settings.backdrop.mica") : t("settings.backdrop.solid")}
            {env && env.osBuild > 0 ? ` · build ${env.osBuild}` : ""}
          </span>
        </Row>
      </section>
    </div>
  );
}
