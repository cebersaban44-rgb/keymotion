// "Takvim" page (Araçlar): the calendars added, the meetings ahead, the card
// before a meeting and the Esc reminder. It used to sit under Ekran →
// Canlı ekran; the live screen keeps the calendar's place in its order and
// a link here. Works without the keyboard (the calendars are read anyway).

import { Toggle } from "../components/controls";
import { CALENDAR_SOURCE, hhmm, useCalendar } from "../lib/calendar";
import { useT } from "../lib/i18n";
import { useLive } from "../lib/live";
import { useApp } from "../lib/store";
import { CalendarBody, calendarStatus } from "./screen/CalendarPanel";

export function Calendar() {
  const t = useT();
  const c = useCalendar((x) => x.state);
  const configure = useCalendar((x) => x.configure);
  const live = useLive((x) => x.state);
  const openScreen = useApp((s) => s.openScreen);
  if (!c) return null;
  const key = live ? calendarStatus(live, c, live.showing === CALENDAR_SOURCE) : null;
  const status = key ? (key === "calendar.st.next" && c.nextCardMs != null ? t(key, { time: hhmm(c.nextCardMs) }) : t(key)) : null;

  return (
    <div className="flex flex-col gap-6" data-calendar-page>
      <header className="flex flex-col gap-2">
        <h1 className="page-title">{t("nav.calendar")}</h1>
        <p className="page-desc">{t("calendar.desc")}</p>
      </header>

      <section className="card flex flex-col gap-5 p-5">
        <div className="flex flex-wrap items-start justify-between gap-4 border-b border-line pb-4">
          <span className="flex min-w-0 max-w-[70ch] flex-1 flex-col gap-1">
            <span className="text-[14px] font-semibold">{t("calendar.screen")}</span>
            <span className="text-[12px] text-ink-3">{t("calendar.body")}</span>
            {status && (
              <span className="mt-1 flex items-center gap-2 text-[12px] text-ink-2" data-calendar-status>
                <span className={`h-2 w-2 rounded-full ${live?.enabled && c.settings.screen ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
                {status}
                {!live?.enabled && (
                  <button type="button" onClick={() => openScreen("live")} className="text-signal hover:underline">
                    {t("calendar.openLive")}
                  </button>
                )}
              </span>
            )}
          </span>
          <Toggle
            checked={c.settings.screen}
            onChange={(screen) => configure({ screen }).catch(console.error)}
            label={t("calendar.screen")}
          />
        </div>
        <CalendarBody />
      </section>
    </div>
  );
}
