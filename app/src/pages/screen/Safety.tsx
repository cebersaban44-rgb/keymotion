// Screen safety notices shown over any page:
//
// * An upload that stopped partway in an earlier run (a quit, a crash, the
//   cable): offered again once, on the next start.
// * Turning the live mode on while the factory animation has no backup: its
//   first card erases the animation for good, so the user is asked once.

import { AlertTriangle, Library, Radio, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { useT } from "../../lib/i18n";
import { useLive } from "../../lib/live";
import { useScreen } from "../../lib/screen";
import { useActiveKeyboard, useApp } from "../../lib/store";
import { DriverBackup, RetryUpload, useItemName } from "./common";

/** When this run started: an incomplete mark older than this is from an
 *  earlier run. */
const SESSION_START = Date.now();

export function ScreenNotices() {
  const asking = useLive((s) => s.askingFactory !== null);
  return (
    <>
      <IncompleteNotice />
      <AnimatePresence>{asking && <FactoryLossDialog />}</AnimatePresence>
    </>
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
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      role="dialog"
      aria-modal
      aria-labelledby="live-ack-title"
      data-live-ack
    >
      <motion.div initial={{ scale: 0.97, y: 8 }} animate={{ scale: 1, y: 0 }} className="card flex w-[480px] flex-col gap-4 p-6">
        <h2 id="live-ack-title" className="font-display text-[22px] font-semibold">
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
                {t("sc.live.ack.body")} {t("sc.factory.driverRestores")}
              </span>
            </p>
            <div className="pl-[22px]">
              <DriverBackup />
            </div>
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
