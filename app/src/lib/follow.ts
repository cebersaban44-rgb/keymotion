// Reading a backend value and following its change events without missing
// one. Reading first and listening after loses an event that comes in
// between, and a read's answer can arrive after a newer event and overwrite
// it. The installed app starts fast enough to hit this: it opened showing
// "Waiting for a keyboard" with the keyboard connected (docs/DURUM.md,
// lesson 61). So: listen first, then read, and drop the read's answer when
// an event came in while it was under way (the event is newer).

import type { UnlistenFn } from "@tauri-apps/api/event";

/** Follow a value: `apply` gets the current one, then every change. Returns
 *  the function that stops following. */
export function follow<T>(
  read: () => Promise<T>,
  listenTo: (cb: (value: T) => void) => Promise<UnlistenFn>,
  apply: (value: T) => void,
): () => void {
  let stopped = false;
  let events = 0;
  let off: UnlistenFn | undefined;
  listenTo((value) => {
    if (stopped) return;
    events += 1;
    apply(value);
  })
    .then(
      (unlisten) => {
        if (stopped) unlisten();
        else off = unlisten;
      },
      console.error,
    )
    .finally(() => {
      if (stopped) return;
      const seen = events;
      read().then((value) => {
        if (!stopped && events === seen) apply(value);
      }, console.error);
    });
  return () => {
    stopped = true;
    off?.();
  };
}
