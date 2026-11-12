// The end of the launch animation (index.html, #intro). It plays from the
// window's first frame; once the app is ready and the animation has run
// (the "K" drawn, the name open), the logo moves onto the sidebar's and the
// animation fades into the app. It never holds the app back longer than
// that, a click or a key ends it at once, and when the app started hidden
// (with Windows, in the tray) nobody saw it, so it just goes.

import { isWindowShown } from "./ipc";

/** The animation's own length (index.html: the last letter lands then);
 *  it is waited for at most three times as long. */
const PLAYS_MS = 950;
/** The move onto the sidebar's logo and the fade into the app. */
const LEAVE_MS = 280;

let ending = false;

function remove(el: HTMLElement) {
  el.remove();
}

/** End the animation; call once the app has drawn its first page. */
export async function finishIntro() {
  const el = document.getElementById("intro");
  if (!el || ending) return;
  ending = true;
  const shown = await isWindowShown().catch(() => true);
  if (!shown) return remove(el);
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  // Let it finish playing (its own animations, however late the first frame
  // came), unless the user clicks or presses a key.
  const playing = el.getAnimations({ subtree: true }).filter((a) => a.playState !== "finished");
  if (playing.length && !reduced) {
    await new Promise<void>((done) => {
      const id = window.setTimeout(stop, PLAYS_MS * 3);
      void Promise.all(playing.map((a) => a.finished.catch(() => undefined))).then(stop);
      function stop() {
        window.clearTimeout(id);
        el!.removeEventListener("pointerdown", stop);
        window.removeEventListener("keydown", stop);
        done();
      }
      el.addEventListener("pointerdown", stop);
      window.addEventListener("keydown", stop);
    });
  }
  const fade = el.animate([{ opacity: 1 }, { opacity: 0 }], {
    duration: reduced ? 150 : LEAVE_MS,
    easing: "cubic-bezier(.5,0,.2,1)",
    fill: "forwards",
  });
  // The logo flies to the sidebar's (when the sidebar shows one).
  const from = el.querySelector("svg")?.getBoundingClientRect();
  const to = document.querySelector("aside img[src='/app-icon.svg']")?.getBoundingClientRect();
  const stage = el.querySelector<HTMLElement>(".intro-stage");
  if (!reduced && from && to && to.width > 0 && stage) {
    const k = to.width / from.width;
    const dx = to.left + to.width / 2 - (from.left + from.width / 2);
    const dy = to.top + to.height / 2 - (from.top + from.height / 2);
    // Scaled around the logo's centre, not the stage's.
    const box = stage.getBoundingClientRect();
    stage.style.transformOrigin = `${from.left + from.width / 2 - box.left}px ${from.top + from.height / 2 - box.top}px`;
    stage.animate([{ transform: "none" }, { transform: `translate(${dx}px, ${dy}px) scale(${k})` }], {
      duration: LEAVE_MS,
      easing: "cubic-bezier(.5,0,.2,1)",
      fill: "forwards",
    });
  }
  await fade.finished.catch(() => undefined);
  remove(el);
}
