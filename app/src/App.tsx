import { RefreshCw } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { UnsavedDialog } from "./components/UnsavedDialog";
import {
  closeWindow,
  getKeyboards,
  getUiEnvironment,
  onCloseRequested,
  onKeyboardsChanged,
  refreshKeyboards,
  setUnsavedFlag,
  setWindowTheme,
} from "./lib/ipc";
import { useUnsaved } from "./lib/unsaved";
import { useT } from "./lib/i18n";
import { useActiveKeyboard, useApp } from "./lib/store";
import { ComingSoon } from "./pages/ComingSoon";
import { Lighting } from "./pages/Lighting";
import { Overview } from "./pages/Overview";
import { Settings } from "./pages/Settings";

function useBootstrap() {
  const setKeyboards = useApp((s) => s.setKeyboards);
  const setEnv = useApp((s) => s.setEnv);
  useEffect(() => {
    let off: (() => void) | undefined;
    getUiEnvironment().then(setEnv).catch(console.error);
    getKeyboards().then(setKeyboards).catch(console.error);
    onKeyboardsChanged(setKeyboards).then((u) => (off = u));
    return () => off?.();
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

function EmptyState() {
  const t = useT();
  return (
    <div className="flex h-[calc(100vh-8rem)] flex-col items-center justify-center gap-5 text-center">
      <div className="relative h-24 w-24">
        <span className="absolute inset-0 animate-ping rounded-full bg-signal/15" />
        <img src="/app-icon.svg" alt="" className="relative h-24 w-24 opacity-90" />
      </div>
      <h1 className="font-display text-[30px] font-semibold tracking-[-0.03em]">{t("device.none.title")}</h1>
      <p className="max-w-[46ch] text-[13.5px] text-ink-2">{t("device.none.body")}</p>
      <button
        type="button"
        onClick={() => refreshKeyboards()}
        className="flex items-center gap-2 rounded-full bg-signal px-4 py-2 text-[13px] font-medium text-signal-ink transition-transform active:scale-[0.97]"
      >
        <RefreshCw size={14} />
        {t("device.refresh")}
      </button>
    </div>
  );
}

export default function App() {
  useBootstrap();
  useThemeSync();
  const guard = useCloseGuard();
  const page = useApp((s) => s.page);
  const loaded = useApp((s) => s.loaded);
  const kb = useActiveKeyboard();

  const content = () => {
    if (page === "settings") return <Settings />;
    if (!kb) return loaded ? <EmptyState /> : null;
    switch (page) {
      case "overview":
      case "connection":
        return <Overview kb={kb} />;
      case "lighting":
        return <Lighting kb={kb} />;
      case "keymap":
        return <ComingSoon title="nav.keymap" features={["keymap", "options"]} />;
      case "macros":
        return <ComingSoon title="nav.macros" features={["macros"]} />;
      case "screen":
        return <ComingSoon title="nav.screen" features={["screen", "clock"]} />;
    }
  };

  return (
    <div className="flex h-full bg-bg">
      <div className="contents" inert={guard.asking}>
        <Sidebar />
        <main className="relative flex-1 overflow-y-auto">
          <AnimatePresence mode="wait">
            <motion.div
              key={page + (kb?.key ?? "none")}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.18 }}
              className="mx-auto min-h-full max-w-[1240px] px-10 pt-9 pb-12"
            >
              {content()}
            </motion.div>
          </AnimatePresence>
        </main>
      </div>
      <AnimatePresence>
        {guard.asking && <UnsavedDialog items={guard.items} onCancel={guard.cancel} />}
      </AnimatePresence>
    </div>
  );
}
