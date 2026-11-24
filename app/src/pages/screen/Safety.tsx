// Screen safety notices shown over any page:
//
// * An upload that stopped partway in an earlier run (a quit, a crash, the
//   cable): offered again once, on the next start.
// * Turning the live mode on while the factory animation has no backup: its
//   first card erases the animation for good, so the user is asked once.
// * The live screen has a card for a keyboard of AJAZZ's "AA" family whose
//   clock page the first upload may replace (plan K-13): the card waits
//   until the user says yes (`LiveState.clockAckNeeded`). "Not now" holds
//   for the session; the Screen page's live tab can ask again.

import { AlertTriangle, Library, Radio, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { create } from "zustand";
import { useAaKeyboard, useWriteBlock } from "../../lib/aa";
import { useT } from "../../lib/i18n";
import { useLive } from "../../lib/live";
import { useScreen } from "../../lib/screen";
import { useActiveKeyboard, useApp } from "../../lib/store";
import { friendlyError } from "../../lib/errors";
import { DriverBackup, RetryUpload, useItemName } from "./common";

/** When this run started: an incomplete mark older than this is from an
 *  earlier run. */
const SESSION_START = Date.now();

/** The clock page question for the live screen: closed with "Not now"
 *  for the session (`later`), opened from the live tab (`open`). */
export const useClockAsk = create<{ later: boolean; open: boolean }>(() => ({ later: false, open: false }));

export function ScreenNotices() {
  const asking = useLive((s) => s.askingFactory !== null);
  const needed = useLive((s) => !!s.state?.enabled && !!s.state.clockAckNeeded);
  // By itself only when a card waits for it, so it comes when it matters.
  const pending = useLive((s) => !!s.state?.pending);
  const { later, open } = useClockAsk();
  // Not on a keyboard that takes no writes: no upload could follow the
  // yes (the live tab says why instead).
  const readOnly = useWriteBlock() !== null;
  const clock = needed && !readOnly && (open || (pending && !later));
  return (
    <>
      <IncompleteNotice />
      <AnimatePresence>
        {asking ? <FactoryLossDialog key="factory" /> : clock ? <ClockPageDialog key="clock" /> : null}
      </AnimatePresence>
    </>
  );
}

/** The live screen's first card may replace an "AA" keyboard's clock page:
 *  asked once, before it goes. The default button is "Not now". */
function ClockPageDialog() {
  const t = useT();
  const ack = useLive((s) => s.ackClock);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const later = () => useClockAsk.setState({ later: true, open: false });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") useClockAsk.setState({ later: true, open: false });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      await ack();
      useClockAsk.setState({ open: false });
    } catch (e) {
      setError(friendlyError(e, t));
      setBusy(false);
    }
  };
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center dlg-smoke p-6"
      role="dialog"
      aria-modal
      aria-labelledby="live-clock-title"
      data-live-clock
    >
      <motion.div initial={{ scale: 0.97, y: 8 }} animate={{ scale: 1, y: 0 }} className="dlg flex w-[480px] max-w-full flex-col gap-4 p-6">
        <h2 id="live-clock-title" className="subtitle">
          {t("sc.live.clock.title")}
        </h2>
        <p className="flex gap-2 text-[13px] text-ink-2">
          <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warn" />
          <span>{t("sc.live.clock.body")}</span>
        </p>
        <p className="pl-[22px] text-[13px] text-ink-2">{t("sc.confirm.clockOnce")}</p>
        {error && <p className="pl-[22px] text-[13px] text-danger">{error}</p>}
        <div className="flex justify-end gap-2 pt-1">
          <button
            type="button"
            autoFocus
            onClick={later}
            disabled={busy}
            className="h-10 rounded-lg border border-line px-4 text-[13px] text-ink-2 hover:bg-surface-2 disabled:opacity-40"
          >
            {t("sc.live.clock.later")}
          </button>
          <button
            type="button"
            onClick={() => void go()}
            disabled={busy}
            className="flex h-10 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink disabled:opacity-50"
            data-live-clock-go
          >
            <Radio size={15} /> {t("sc.live.clock.go")}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}

/** "Son yükleme yarım kaldı", offered at start-up (the Screen page shows it
 *  in place, so not there). */
function IncompleteNotice() {
  const t = useT();
  const name = useItemName();
  const incomplete = useScreen((s) => s.state?.incomplete ?? null);
  const items = useScreen((s) => s.state?.items);
  const upload = useScreen((s) => s.upload !== null);
  const page = useApp((s) => s.page);
  const openScreen = useApp((s) => s.openScreen);
  const kb = useActiveKeyboard();
  const [closed, setClosed] = useState(false);
  const usb = kb?.connection === "usb" && kb.hasCommandChannel && kb.features.includes("screen");
  const fromEarlier = !!incomplete && incomplete.atMs < SESSION_START;
  const visible = fromEarlier && !closed && !upload && page !== "screen";
  const item = items?.find((i) => i.id === incomplete?.id) ?? null;
  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="card fixed right-6 bottom-6 z-40 flex w-[360px] flex-col gap-2.5 p-4 shadow-xl"
          role="status"
          data-incomplete-notice
        >
          <div className="flex items-start justify-between gap-2">
            <span className="flex items-center gap-2 text-[13px] font-semibold">
              <AlertTriangle size={14} className="shrink-0 text-warn" />
              {t("sc.incomplete.title")}
            </span>
            <button
              type="button"
              onClick={() => setClosed(true)}
              aria-label={t("sc.close")}
              className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-ink-3 hover:bg-surface-2 hover:text-ink"
            >
              <X size={14} />
            </button>
          </div>
          <p className="text-[12px] text-ink-2">
            {t("sc.incomplete.body")} {item ? t("sc.incomplete.retry", { name: name(item) }) : t("sc.incomplete.pick")}
          </p>
          <div className="flex flex-wrap items-center gap-3">
            {item && <RetryUpload item={item} usb={usb} />}
            <button
              type="button"
              onClick={() => openScreen("visuals")}
              className="text-[12px] font-medium text-signal hover:underline"
            >
              {t("sc.incomplete.open")}
            </button>
          </div>
          {item && !usb && <p className="text-[12px] text-ink-3">{t("sc.needsUsbShort")}</p>}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/** Asked once before the live mode's first card would erase the factory
 *  animation, which has no backup. The default button is "Vazgeç". */
function FactoryLossDialog() {
  const t = useT();
  const answer = useLive((s) => s.answerFactory);
  // The backend asked, so no backup unless one was taken just now.
  const hasFactory = useScreen((s) => s.state?.items.some((i) => i.kind === "factory") ?? false);
  const aa = useAaKeyboard();
  const [busy, setBusy] = useState(false);
  const cancel = () => void answer(false).catch(console.error);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void answer(false).catch(console.error);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [answer]);
  const go = async () => {
    setBusy(true);
    try {
      await answer(true);
    } catch (e) {
      console.error(e);
      setBusy(false);
    }
  };
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center dlg-smoke p-6"
      role="dialog"
      aria-modal
      aria-labelledby="live-ack-title"
      data-live-ack
    >
      <motion.div initial={{ scale: 0.97, y: 8 }} animate={{ scale: 1, y: 0 }} className="dlg flex w-[480px] max-w-full flex-col gap-4 p-6">
        <h2 id="live-ack-title" className="subtitle">
          {t("sc.live.ack.title")}
        </h2>
        {hasFactory ? (
          // Backed up from the official driver just now.
          <p className="flex gap-2 text-[13px] text-ink-2" role="status">
            <Library size={14} className="mt-0.5 shrink-0 text-ok" />
            <span>{t("sc.confirm.factoryKept")}</span>
          </p>
        ) : (
          <>
            <p className="flex gap-2 text-[13px] text-ink-2">
              <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warn" />
              <span>
                {t("sc.live.ack.body")} {!aa && t("sc.factory.driverRestores")}
              </span>
            </p>
            {/* AJAZZ's old official driver doesn't know an "AA" keyboard's firmware. */}
            {!aa && (
              <div className="pl-[22px]">
                <DriverBackup />
              </div>
            )}
          </>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <button
            type="button"
            autoFocus
            onClick={cancel}
            disabled={busy}
            className="h-10 rounded-lg border border-line px-4 text-[13px] text-ink-2 hover:bg-surface-2 disabled:opacity-40"
          >
            {t("sc.cancel")}
          </button>
          <button
            type="button"
            onClick={go}
            disabled={busy}
            className="flex h-10 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink disabled:opacity-50"
          >
            <Radio size={15} /> {t("sc.live.ack.go")}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
