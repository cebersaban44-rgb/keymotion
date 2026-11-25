import { Lock } from "lucide-react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { useEffect, useState } from "react";
import { Row } from "./components/controls";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { RestoreToast } from "./components/KeyboardBackups";
import { Reporting } from "./components/Reporting";
import { Sidebar } from "./components/Sidebar";
import { UnsavedDialog } from "./components/UnsavedDialog";
import { SafetyDialogs, useSafetyDialogOpen } from "./components/SafetyDialogs";
import { GuidePanel } from "./components/TestGuide";
import {
  cancelQuit,
  closeWindow,
  getKeyboards,
  getUiEnvironment,
  isWindowShown,
  onCloseRequested,
  onKeyboardsChanged,
  onTrayProfile,
  onWindowShown,
  setTrayTexts,
  setUnsavedFlag,
  setWindowTheme,
} from "./lib/ipc";
import { follow } from "./lib/follow";
import { useGuideFollow } from "./lib/aaGuide";
import { useUnsupported } from "./lib/report";
import { useUnsaved } from "./lib/unsaved";
import { connectionKey, modelNames, tNow, useT, type MessageKey } from "./lib/i18n";
import { useKeymap } from "./lib/keymap";
import { useLayout } from "./lib/layout";
import { startAgentCards } from "./lib/agentCards";
import { startLive } from "./lib/live";
import {
  agentCard,
  agentCardWords,
  calendarCard,
  calendarWords,
  gameStartCard,
  gameSummaryCard,
  gameWords,
  nowPlayingCard,
  pomodoroCard,
  pomodoroWords,
  reminderCard,
  reminderWords,
} from "./lib/liveCards";
import { gameCover, startGames } from "./lib/games";
import { programIconData } from "./lib/icons";
import { startPomodoro, usePomodoro, DEFAULT_SETTINGS } from "./lib/pomodoro";
import { followGauges, useGauges } from "./lib/gauges";
import { followMusic, useMusic } from "./lib/music";
import { startCalendar, useCalendar } from "./lib/calendar";
import { startReminders } from "./lib/reminders";
import { startUpdates, useUpdates } from "./lib/updates";
import { UpdateDialog, UpdateStrip } from "./components/Updates";
import { useMacros } from "./lib/macros";
import { followAutoWrites, followKeyboardRecords } from "./lib/profiles";
import { useScreen } from "./lib/screen";
import { followSelectWidths } from "./lib/selectWidth";
import { OFFLINE_KEYBOARD } from "./lib/offline";
import { useActiveKeyboard, useApp, type PageId } from "./lib/store";
import { Calendar } from "./pages/Calendar";
import { Games } from "./pages/Games";
import { Connection } from "./pages/Connection";
import { Keymap } from "./pages/Keymap";
import { Macros } from "./pages/Macros";
import { Focus } from "./pages/Focus";
import { Gauges } from "./pages/Gauges";
import { Music } from "./pages/Music";
import { Reminders } from "./pages/Reminders";
import { Lighting } from "./pages/Lighting";
import { Overview } from "./pages/Overview";
import { Profiles } from "./pages/Profiles";
import { Screen, ScreenUploadToast } from "./pages/Screen";
import { ScreenNotices } from "./pages/screen/Safety";
import { Settings } from "./pages/Settings";
import { Agents } from "./pages/Agents";
import { Welcome } from "./pages/Welcome";
import { finishIntro } from "./lib/intro";

function useBootstrap() {
  const setKeyboards = useApp((s) => s.setKeyboards);
  const setEnv = useApp((s) => s.setEnv);
  useEffect(() => {
    getUiEnvironment().then(setEnv).catch(console.error);
    const off = follow(getKeyboards, onKeyboardsChanged, setKeyboards);
    // Key labels follow the Windows keyboard layout; it can change while
    // the app is in the background.
    const refreshLayout = () => void useLayout.getState().refresh();
    refreshLayout();
    window.addEventListener("focus", refreshLayout);
    return () => {
      off();
      window.removeEventListener("focus", refreshLayout);
    };
  }, [setKeyboards, setEnv]);
  // List boxes as wide as their longest choice, whatever is chosen.
  useEffect(() => followSelectWidths(), []);
}

function useThemeSync() {
  const theme = useApp((s) => s.theme);
  const locale = useApp((s) => s.locale);
  const mica = useApp((s) => s.env?.mica);
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
    // Until the backend answers, theme-boot.js's mark from the last start stays.
    if (mica !== undefined) document.documentElement.dataset.mica = String(mica);
  }, [mica]);
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
}

/**
 * Load the app's keymap, macro and screen records for the connected
 * keyboard (kept per record id: model and command family), so unsaved edits
 * are known (and guarded) before their pages are opened.
 */
function useKeyboardRecords() {
  const kb = useActiveKeyboard();
  const modelId = kb?.modelId;
  const recordId = kb?.recordId;
  useEffect(() => {
    if (!recordId) return;
    useKeymap.getState().load(recordId).catch(console.error);
    useMacros.getState().load(recordId).catch(console.error);
    useScreen.getState().load(recordId).catch(console.error);
  }, [recordId]);
  // A profile written by the per-app profiles: those records are read again.
  useEffect(() => (modelId ? followAutoWrites(modelId) : undefined), [modelId]);
  // An "AA" keyboard's keys and macros, as the keyboard holds them.
  useEffect(() => (modelId ? followKeyboardRecords(modelId) : undefined), [modelId]);
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
  useEffect(() => startAgentCards((card) => agentCard(card, agentCardWords(tNow, useApp.getState().locale))), []);
  useEffect(
    () =>
      startGames(async (card) => {
        if (card.kind !== "start") return gameSummaryCard(card, gameWords(tNow));
        const cover = card.steamApp != null ? await gameCover(card.steamApp) : null;
        // No cover (or one that can't be read): the program's own icon.
        const exe = card.exe;
        return gameStartCard(card, cover, gameWords(tNow), () => (exe ? programIconData(exe, 48) : Promise.resolve(null)));
      }),
    [],
  );
  useEffect(() => startUpdates(), []);
  // Whether the system gauges are on (the Lighting page says so); the
  // tray menu can switch them too.
  useEffect(() => {
    useGauges.getState().load().catch(console.error);
    return followGauges();
  }, []);
  // Whether music mode is on (the Lighting and gauges pages say so); the
  // tray menu can switch it too.
  useEffect(() => {
    useMusic.getState().load().catch(console.error);
    return followMusic();
  }, []);
}

/**
 * The tray: its menu's words in the UI language, whether the window is on
 * screen (hidden, the app runs in the tray), and profiles chosen there.
 */
function useTray() {
  const locale = useApp((s) => s.locale);
  const setWindowShown = useApp((s) => s.setWindowShown);
  const requestProfile = useApp((s) => s.requestProfile);
  // A keyboard Keymotion can't drive yet (an AK980 PRO on an AJAZZ firmware
  // it doesn't know, or another model): the menu says why no keyboard shows.
  const unsupported = useUnsupported();
  const newFirmware = unsupported.newFirmware.length > 0;
  const otherKeyboard = unsupported.list.length > 0;
  useEffect(() => {
    // Templates go as they are: the backend fills in {min}, {n}, ….
    setTrayTexts({
      locale,
      open: tNow("tray.open"),
      noKeyboard: tNow(newFirmware ? "newfw.tray" : otherKeyboard ? "unsupported.tray" : "tray.noKeyboard"),
      battery: tNow("tray.battery"),
      connUsb: tNow(connectionKey("usb")),
      connDongle: tNow(connectionKey("dongle")),
      connBluetooth: tNow(connectionKey("bluetooth")),
      focus: tNow("tray.focus"),
      focusStart: tNow("tray.focus.start"),
      focusPause: tNow("tray.focus.pause"),
      focusResume: tNow("tray.focus.resume"),
      focusSkip: tNow("tray.focus.skip"),
      focusStop: tNow("tray.focus.stop"),
      phaseFocus: tNow("tray.phase.focus"),
      phaseBreak: tNow("tray.phase.break"),
      round: tNow("tray.round"),
      left: tNow("tray.left"),
      paused: tNow("tray.paused"),
      ready: tNow("tray.ready"),
      reminders: tNow("tray.reminders"),
      remindersOn: tNow("tray.reminders.on"),
      remindersOff: tNow("tray.reminders.off"),
      remindersNext: tNow("tray.reminders.next"),
      remindersNone: tNow("tray.reminders.none"),
      tomorrow: tNow("tray.tomorrow"),
      gauges: tNow("tray.gauges"),
      music: tNow("tray.music"),
      profiles: tNow("tray.profiles"),
      defaultProfile: tNow("prof.default"),
      quit: tNow("tray.quit"),
      hiddenTitle: tNow("tray.hidden.title"),
      hiddenBody: tNow("tray.hidden.body"),
      updateAvailable: tNow("tray.update.available"),
      updateReady: tNow("tray.update.ready"),
      updateTooltip: tNow("tray.update.tooltip"),
      game: tNow("tray.game"),
      gameHours: tNow("games.time.hours"),
      gameMinutes: tNow("games.time.minutes"),
      gameLess: tNow("games.time.less"),
      autoProfile: tNow("tray.autoProfile"),
      noticeReadyTitle: tNow("notice.update.ready.title"),
      noticeReadyBody: tNow("notice.update.ready.body"),
      noticeInstall: tNow("notice.update.install"),
      noticeLater: tNow("update.later"),
      noticeInstalledTitle: tNow("notice.update.installed.title"),
      noticeInstalledAuto: tNow("notice.update.installed.auto"),
      noticeWhatsNew: tNow("update.whatsNew"),
      modelNames: modelNames(),
    }).catch(console.error);
  }, [locale, newFirmware, otherKeyboard]);
  useEffect(() => {
    let alive = true;
    const offs: (() => void)[] = [];
    const keep = (off: () => void) => (alive ? offs.push(off) : off());
    // On screen: shown by the backend (not in the tray, not minimized) and
    // not hidden to the page itself, where WebView2 says so.
    let shown = true;
    const update = () => setWindowShown(shown && document.visibilityState !== "hidden");
    const fromBackend = (value: boolean) => {
      shown = value;
      update();
    };
    isWindowShown().then(fromBackend).catch(console.error);
    onWindowShown(fromBackend).then(keep);
    document.addEventListener("visibilitychange", update);
    onTrayProfile(requestProfile).then(keep);
    return () => {
      alive = false;
      offs.forEach((off) => off());
      document.removeEventListener("visibilitychange", update);
    };
  }, [setWindowShown, requestProfile]);
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
  const cancel = () => {
    setAsking(false);
    // After "Quit" in the tray: closing hides the window again.
    cancelQuit().catch(console.error);
  };
  return { items, asking: asking && unsaved, cancel };
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
        <header>
          <h1 className="page-title">{t(info.title)}</h1>
          <p className="page-desc">{t(info.desc)}</p>
        </header>
      )}
      <div className="rows">
        <Row icon={Lock} title={t("locked.title")} desc={t("locked.body")}>
          <button type="button" onClick={() => setPage("overview")} className="btn-secondary">
            {t("locked.steps")}
          </button>
        </Row>
      </div>
    </div>
  );
}

export default function App() {
  useBootstrap();
  useThemeSync();
  useKeyboardRecords();
  useLiveScreen();
  useTray();
  useGuideFollow();
  const guard = useCloseGuard();
  const safetyOpen = useSafetyDialogOpen();
  const updateOpen = useUpdates((s) => s.dialog);
  const page = useApp((s) => s.page);
  const loaded = useApp((s) => s.loaded);
  const kb = useActiveKeyboard();
  // The launch animation (index.html) ends once the first page can show,
  // and never later than a few seconds, should the keyboards not answer.
  useEffect(() => {
    if (loaded) void finishIntro();
  }, [loaded]);
  useEffect(() => {
    const id = window.setTimeout(() => void finishIntro(), 4000);
    return () => window.clearTimeout(id);
  }, []);

  const content = () => {
    if (page === "settings") return <Settings />;
    if (!loaded) return null;
    // Tools, the screen library and the profiles work without the keyboard.
    const tool = kb ?? OFFLINE_KEYBOARD;
    switch (page) {
      case "profiles":
        return <Profiles kb={tool} />;
      case "focus":
        return <Focus kb={tool} />;
      case "gauges":
        return <Gauges kb={tool} />;
      case "music":
        return <Music kb={tool} />;
      case "reminders":
        return <Reminders kb={tool} />;
      case "calendar":
        return <Calendar />;
      case "games":
        return <Games kb={tool} />;
      case "screen":
        return <Screen kb={tool} />;
      case "agents":
        return <Agents />;
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
    <div className="flex h-full">
      <div className="contents" inert={guard.asking || safetyOpen || updateOpen}>
        <Sidebar />
        <main className="page-layer relative min-w-0 flex-1 overflow-x-hidden overflow-y-auto">
          <AnimatePresence mode="wait">
            <motion.div
              key={page + (kb?.key ?? "none")}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.18 }}
              className="@container mx-auto min-h-full max-w-[1240px] px-6 pt-[26px] pb-11 min-[1200px]:px-10"
            >
              {/* A new version found by the daily look (or downloading). */}
              <UpdateStrip />
              {/* A page that fails to draw leaves the sidebar usable. */}
              <ErrorBoundary>{content()}</ErrorBoundary>
            </motion.div>
          </AnimatePresence>
        </main>
        {/* The 3.01 test guide, beside every page while it is open. */}
        <GuidePanel />
      </div>
      <ScreenUploadToast />
      <RestoreToast />
      <SafetyDialogs />
      <ScreenNotices />
      <Reporting />
      <UpdateDialog />
      <AnimatePresence>
        {guard.asking && <UnsavedDialog items={guard.items} onCancel={guard.cancel} />}
      </AnimatePresence>
    </div>
    </MotionConfig>
  );
}
