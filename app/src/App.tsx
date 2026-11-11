import { Lock } from "lucide-react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { useEffect, useState } from "react";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Sidebar } from "./components/Sidebar";
import { UnsavedDialog } from "./components/UnsavedDialog";
import { SafetyDialogs, useSafetyDialogOpen } from "./components/SafetyDialogs";
import {
  closeWindow,
  getKeyboards,
  getUiEnvironment,
  onCloseRequested,
  onKeyboardsChanged,
  setUnsavedFlag,
  setWindowTheme,
} from "./lib/ipc";
import { useUnsaved } from "./lib/unsaved";
import { tNow, useT, type MessageKey } from "./lib/i18n";
import { useKeymap } from "./lib/keymap";
import { useLayout } from "./lib/layout";
import { startLive } from "./lib/live";
import { calendarCard, calendarWords, nowPlayingCard, pomodoroCard, pomodoroWords, reminderCard, reminderWords } from "./lib/liveCards";
import { startPomodoro, usePomodoro, DEFAULT_SETTINGS } from "./lib/pomodoro";
import { useGauges } from "./lib/gauges";
import { startCalendar, useCalendar } from "./lib/calendar";
import { startReminders } from "./lib/reminders";
import { useMacros } from "./lib/macros";
import { useScreen } from "./lib/screen";
import { OFFLINE_KEYBOARD } from "./lib/offline";
import { useActiveKeyboard, useApp, type PageId } from "./lib/store";
import { Calendar } from "./pages/Calendar";
import { Connection } from "./pages/Connection";
import { Keymap } from "./pages/Keymap";
import { Macros } from "./pages/Macros";
import { Focus } from "./pages/Focus";
import { Gauges } from "./pages/Gauges";
import { Reminders } from "./pages/Reminders";
import { Lighting } from "./pages/Lighting";
import { Overview } from "./pages/Overview";
import { Screen, ScreenUploadToast } from "./pages/Screen";
import { ScreenNotices } from "./pages/screen/Safety";
import { Settings } from "./pages/Settings";
import { Welcome } from "./pages/Welcome";

function useBootstrap() {
  const setKeyboards = useApp((s) => s.setKeyboards);
  const setEnv = useApp((s) => s.setEnv);
  useEffect(() => {
    let off: (() => void) | undefined;
    getUiEnvironment().then(setEnv).catch(console.error);
    getKeyboards().then(setKeyboards).catch(console.error);
    onKeyboardsChanged(setKeyboards).then((u) => (off = u));
    // Key labels follow the Windows keyboard layout; it can change while
    // the app is in the background.
    const refreshLayout = () => void useLayout.getState().refresh();
    refreshLayout();
    window.addEventListener("focus", refreshLayout);
    return () => {
      off?.();
      window.removeEventListener("focus", refreshLayout);
    };
  }, [setKeyboards, setEnv]);
}

function useThemeSync() {
  const theme = useApp((s) => s.theme);
  const locale = useApp((s) => s.locale);
  const mica = useApp((s) => s.env?.mica ?? false);
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const resolved = theme === "system" ? (media.matches ? "dark" : "light") : theme;
      document.documentElement.dataset.theme = resolved;
    };
    apply();
    media.addEventListener("change", apply);
    setWindowTheme(theme).catch(console.error);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
  useEffect(() => {
    document.documentElement.dataset.mica = String(mica);
  }, [mica]);
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
}

/**
 * Load the app's keymap, macro and screen records for the connected model,
 * so unsaved edits are known (and guarded) before their pages are opened.
 */
function useKeyboardRecords() {
  const modelId = useActiveKeyboard()?.modelId;
  useEffect(() => {
    if (!modelId) return;
    useKeymap.getState().load(modelId).catch(console.error);
    useMacros.getState().load(modelId).catch(console.error);
    useScreen.getState().load(modelId).catch(console.error);
  }, [modelId]);
}

/** Words of the Pomodoro cards in the current language. */
/** The live screen and the Pomodoro run for the whole session, whatever
 *  page is open. */
function useLiveScreen() {
  useEffect(() => startLive((track, cover) => nowPlayingCard(track, cover, tNow("sc.live.card.np"))), []);
  useEffect(
    () =>
      startPomodoro((card) =>
        pomodoroCard(card, usePomodoro.getState().state?.settings ?? DEFAULT_SETTINGS, pomodoroWords(tNow)),
      ),
    [],
  );
  useEffect(
    () =>
      startCalendar((card) => {
        const name = useCalendar.getState().state?.sources.find((s) => s.id === card.event.source)?.name ?? "";
        return calendarCard(card, name, calendarWords(tNow, useApp.getState().locale));
      }),
    [],
  );
  useEffect(() => startReminders((card) => reminderCard(card, reminderWords(tNow, useApp.getState().locale))), []);
  // Whether the system gauges are on (the Lighting page says so).
  useEffect(() => {
    useGauges.getState().load().catch(console.error);
  }, []);
}

/**
 * Ask before closing while changes are not on the keyboard. The backend
 * only intercepts the close when told something is unsaved.
 */
function useCloseGuard() {
  const items = useUnsaved();
  const [asking, setAsking] = useState(false);
  const unsaved = items.length > 0;
  useEffect(() => {
    setUnsavedFlag(unsaved).catch(console.error);
  }, [unsaved]);
  useEffect(() => {
    let off: (() => void) | undefined;
    onCloseRequested(() => setAsking(true)).then((u) => (off = u));
    return () => off?.();
  }, []);
  // Nothing left to ask about (saved in the meantime): just close.
  useEffect(() => {
    if (asking && !unsaved) closeWindow().catch(console.error);
  }, [asking, unsaved]);
  return { items, asking: asking && unsaved, cancel: () => setAsking(false) };
}

/** Pages that change the keyboard's own settings: without it they say so. */
const LOCKED_PAGE: Partial<Record<PageId, { title: MessageKey; desc: MessageKey }>> = {
  lighting: { title: "nav.lighting", desc: "light.desc" },
  keymap: { title: "nav.keymap", desc: "km.eyebrow" },
  macros: { title: "nav.macros", desc: "mc.eyebrow" },
};

/** A keyboard page while no keyboard is connected. */
function DeviceLocked({ page }: { page: PageId }) {
  const t = useT();
  const setPage = useApp((s) => s.setPage);
  const info = LOCKED_PAGE[page];
  return (
    <div className="flex flex-col gap-6" data-locked={page}>
      {info && (
        <header className="flex flex-col gap-2">
          <h1 className="page-title">{t(info.title)}</h1>
          <p className="page-desc">{t(info.desc)}</p>
        </header>
      )}
      <section className="card flex flex-wrap items-center gap-4 p-5">
        <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-surface-2 text-ink-2">
          <Lock size={18} aria-hidden />
        </span>
        <div className="min-w-[220px] flex-1">
          <h2 className="eyebrow">{t("locked.title")}</h2>
          <p className="mt-0.5 text-[13px] text-ink-2">{t("locked.body")}</p>
        </div>
        <button
          type="button"
          onClick={() => setPage("overview")}
          className="h-9 rounded-lg border border-line px-3 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
        >
          {t("locked.steps")}
        </button>
      </section>
    </div>
  );
}

export default function App() {
  useBootstrap();
  useThemeSync();
  useKeyboardRecords();
  useLiveScreen();
  const guard = useCloseGuard();
  const safetyOpen = useSafetyDialogOpen();
  const page = useApp((s) => s.page);
  const loaded = useApp((s) => s.loaded);
  const kb = useActiveKeyboard();

  const content = () => {
    if (page === "settings") return <Settings />;
    if (!loaded) return null;
    // Tools and the screen library work without the keyboard.
    const tool = kb ?? OFFLINE_KEYBOARD;
    switch (page) {
      case "focus":
        return <Focus kb={tool} />;
      case "gauges":
        return <Gauges kb={tool} />;
      case "reminders":
        return <Reminders kb={tool} />;
      case "calendar":
        return <Calendar />;
      case "screen":
        return <Screen kb={tool} />;
    }
    if (!kb) return page === "overview" || page === "connection" ? <Welcome /> : <DeviceLocked page={page} />;
    switch (page) {
      case "overview":
        return <Overview kb={kb} />;
      case "connection":
        return <Connection kb={kb} />;
      case "lighting":
        return <Lighting kb={kb} />;
      case "keymap":
        return <Keymap kb={kb} />;
      case "macros":
        return <Macros kb={kb} />;
    }
  };

  return (
    // Follows Windows' "Animation effects" setting.
    <MotionConfig reducedMotion="user">
    <div className="flex h-full bg-bg">
      <div className="contents" inert={guard.asking || safetyOpen}>
        <Sidebar />
        <main className="relative min-w-0 flex-1 overflow-x-hidden overflow-y-auto">
          <AnimatePresence mode="wait">
            <motion.div
              key={page + (kb?.key ?? "none")}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.18 }}
              className="@container mx-auto min-h-full max-w-[1240px] px-6 pt-7 pb-10 min-[1200px]:px-10 min-[1200px]:pt-9"
            >
              {/* A page that fails to draw leaves the sidebar usable. */}
              <ErrorBoundary>{content()}</ErrorBoundary>
            </motion.div>
          </AnimatePresence>
        </main>
      </div>
      <ScreenUploadToast />
      <SafetyDialogs />
      <ScreenNotices />
      <AnimatePresence>
        {guard.asking && <UnsavedDialog items={guard.items} onCancel={guard.cancel} />}
      </AnimatePresence>
    </div>
    </MotionConfig>
  );
}
