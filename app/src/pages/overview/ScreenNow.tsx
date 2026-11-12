// "Klavye ekranında şimdi": the picture on the keyboard's GIF page, drawn
// from the same records as the Screen page, with the live screen's state
// and a card that waits to go.

import { Clock3, Monitor } from "lucide-react";
import { useT } from "../../lib/i18n";
import { useLive } from "../../lib/live";
import { useApp } from "../../lib/store";
import { GifPicture, useGifPage } from "../screen/common";
import { TileLink } from "./Tile";

export function ScreenNow() {
  const t = useT();
  const openScreen = useApp((s) => s.openScreen);
  const gif = useGifPage();
  const liveOn = useLive((s) => (s.state ? s.state.enabled : null));
  // Whole uploads the live screen may use now.
  const left = useLive((s) => (s.state ? Math.max(0, Math.floor(s.state.tokens + 1e-6)) : 0));
  const empty = !gif.item && !gif.showsLive;

  return (
    <section className="card flex min-w-0 flex-col gap-3 p-5" data-overview-screen>
      <header className="flex items-center gap-2.5">
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-line bg-bg-raise text-ink-2">
          <Monitor size={15} aria-hidden />
        </span>
        <h2 className="min-w-0 flex-1 truncate text-[13px] font-semibold">{t("overview.screen.title")}</h2>
      </header>
      <div className="px-1.5 pt-1.5">
        <div
          className={`relative aspect-[240/135] w-full overflow-hidden rounded-md ${
            empty ? "border border-dashed border-line-strong bg-surface-2/50" : "bg-black shadow-[0_0_0_5px_#0b0c0e,0_0_0_6px_var(--line)]"
          }`}
        >
          <GifPicture gif={gif} />
        </div>
      </div>
      <p className="truncate pt-1 text-[13px] font-medium" title={gif.caption} data-overview-screen-shows>
        {gif.caption}
      </p>
      {liveOn !== null && (
        <p className="flex items-center gap-2 text-[12px] text-ink-2" data-overview-live-screen={liveOn ? "on" : "off"}>
          <span className={`h-2 w-2 shrink-0 rounded-full ${liveOn ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
          <span className="num">{liveOn ? t("overview.screen.liveLeft", { count: left }) : t("overview.screen.liveOff")}</span>
        </p>
      )}
      {gif.waiting && (
        <p className="flex items-center gap-2 text-[12px] text-ink-2" data-overview-waiting>
          <Clock3 size={13} className="shrink-0 text-warn" aria-hidden />
          <span className="truncate">{t("overview.screen.waiting", { name: gif.waiting.name })}</span>
        </p>
      )}
      <TileLink text={t("overview.screen.go")} onClick={() => openScreen(liveOn ? "live" : "visuals")} />
    </section>
  );
}
