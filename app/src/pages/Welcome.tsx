// First run, or no keyboard connected: which keyboard the app supports, how
// to connect it, and what works without it. Shown in place of Genel bakış
// and Bağlantı until a keyboard is found (the app looks every 2 seconds).
// A SONiX or AJAZZ keyboard the app doesn't support yet gets a card on top
// with "Request support" (lib/report.ts).

import { ArrowUpRight, BellRing, Images, Keyboard, Timer, type LucideIcon } from "lucide-react";
import { ModeSwitch } from "../components/ModeSwitch";
import { useT, type MessageKey } from "../lib/i18n";
import { openLink } from "../lib/ipc";
import { SUPPORTED_MODEL, docsPage } from "../lib/offline";
import { useReport, useUnsupportedKeyboards } from "../lib/report";
import { useApp } from "../lib/store";

const STEPS: MessageKey[] = ["welcome.step1", "welcome.step2", "welcome.step3"];

export function Welcome() {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  const locale = useApp((s) => s.locale);
  const openScreen = useApp((s) => s.openScreen);
  const openReport = useReport((s) => s.openReport);
  const unsupported = useUnsupportedKeyboards();

  const tools: { icon: LucideIcon; title: MessageKey; body: MessageKey; open: () => void }[] = [
    { icon: Timer, title: "nav.focus", body: "welcome.focus", open: () => setPage("focus") },
    { icon: BellRing, title: "nav.reminders", body: "welcome.reminders", open: () => setPage("reminders") },
    { icon: Images, title: "welcome.screen", body: "welcome.screen.body", open: () => openScreen("visuals") },
  ];

  return (
    <div className="mx-auto flex max-w-[680px] flex-col gap-7 pt-4" data-welcome>
      <header className="flex flex-col gap-2">
        <img src="/app-icon.svg" alt="" className="h-14 w-14" />
        <h1 className="page-title mt-2">{t("welcome.title")}</h1>
        <p className="page-desc">{t("welcome.desc")}</p>
      </header>

      {unsupported.map((k) => (
        <section key={k.id} className="card flex flex-wrap items-center gap-x-5 gap-y-4 p-5" data-unsupported={k.id}>
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-warn/15 text-warn">
            <Keyboard size={19} aria-hidden />
          </span>
          <div className="flex min-w-[240px] flex-1 flex-col gap-1">
            <span className="text-[12px] text-ink-3">{t("unsupported.title")}</span>
            <span className="text-[15px] font-semibold">{k.name ?? t("unsupported.unnamed")}</span>
            <p className="text-[13px] text-ink-2">{t("unsupported.body")}</p>
          </div>
          <button
            type="button"
            onClick={() =>
              openReport({
                kind: "keyboard",
                title: `[Keyboard] ${k.name ?? k.id}`,
                keyboardId: k.id,
                model: k.name ?? undefined,
              })
            }
            className="flex h-9 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink transition-transform active:scale-[0.98]"
          >
            {t("unsupported.request")}
          </button>
        </section>
      ))}

      <section className="card flex flex-wrap gap-x-8 gap-y-5 p-6" aria-labelledby="welcome-connect">
        <div className="flex min-w-[260px] flex-1 flex-col gap-3.5">
          <h2 id="welcome-connect" className="eyebrow">
            {t("welcome.connect")}
          </h2>
          <ol className="flex list-decimal flex-col gap-2 pl-5 text-[14px] text-ink-2">
            {STEPS.map((k) => (
              <li key={k}>{t(k)}</li>
            ))}
          </ol>
          <p className="mt-1 flex items-center gap-2.5 text-[13px] text-ink-3" role="status" data-welcome-searching>
            <span className="h-2 w-2 rounded-full bg-signal motion-safe:animate-pulse" aria-hidden />
            {t("welcome.searching")}
          </p>
        </div>
        <figure className="flex w-[190px] flex-col items-center justify-center gap-2">
          <figcaption className="text-[12px] text-ink-3">{t("welcome.back")}</figcaption>
          <div className="w-[180px]">
            <ModeSwitch value="usb" target />
          </div>
        </figure>
      </section>

      <section className="flex flex-wrap items-center justify-between gap-4 border-t border-line pt-5">
        <div className="flex flex-col gap-0.5">
          <span className="text-[12px] text-ink-3">{t("welcome.supported")}</span>
          <span className="text-[15px] font-semibold">{SUPPORTED_MODEL}</span>
        </div>
        <button
          type="button"
          onClick={() => openLink(docsPage("help/keyboards/", locale)).catch(console.error)}
          className="flex items-center gap-1 rounded-md text-[13px] text-signal hover:underline"
          data-welcome-other
        >
          {t("welcome.other")}
          <ArrowUpRight size={14} aria-hidden />
        </button>
      </section>

      <section className="flex flex-col gap-3" aria-labelledby="welcome-without">
        <h2 id="welcome-without" className="eyebrow">
          {t("welcome.without")}
        </h2>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-3">
          {tools.map(({ icon: Icon, title, body, open }) => (
            <button
              key={title}
              type="button"
              onClick={open}
              className="flex flex-col gap-1 rounded-lg border border-line px-4 py-3.5 text-left transition-colors hover:bg-surface"
            >
              <span className="flex items-center gap-2 text-[14px] font-semibold">
                <Icon size={15} className="text-signal" aria-hidden />
                {t(title)}
              </span>
              <span className="text-[13px] text-ink-3">{t(body)}</span>
            </button>
          ))}
        </div>
      </section>

      <p className="text-[12px] leading-relaxed text-ink-3">{t("welcome.independent", { name: t("app.name") })}</p>
    </div>
  );
}
