// The capture guard: while capture.mjs drives the real app, the page's calls
// to the backend that would write to the keyboard's memory, change another
// program's files, or open a window outside the app are refused. A step that
// tries one fails with a clear message instead of writing.

/** Backend commands the guard refuses (app/src-tauri/src/lib.rs). */
export const DENY = [
  // Flash writes and the keyboard's clock.
  "apply_effect",
  "apply_per_key",
  "lighting_save",
  "apply_keymap",
  "apply_macros",
  "screen_upload",
  "screen_backup_from_driver",
  "profiles_load",
  "sync_clock",
  "set_clock_auto",
  "confirm_firmware",
  // The live screen uploads its cards: never turned on, nothing handed over.
  "live_configure",
  "live_ack_factory",
  "live_submit",
  "live_send_now",
  // Profiles by app may write a profile when a program comes to the front.
  "autoprofile_configure",
  // Other programs' files and PATH (allowed for one step with --agents-home).
  "agents_connect",
  "agents_disconnect",
  // Files and windows outside the app.
  "backup_export",
  "backup_import",
  "profiles_export",
  "profiles_import",
  "update_install",
  "open_link",
  "open_issue",
  "open_windows_page",
  "open_folder",
  "games_clear_history",
];

/**
 * Runs in the page (`page.evaluate(guard, [DENY, allow])`): wraps Tauri's
 * `invoke` so the `deny` commands not in `allow` are refused and noted.
 * Called again it only changes `allow`. Returns "on", "no-tauri" (a
 * browser, nothing to guard) or "failed" (the guard could not be put in
 * place: capture.mjs stops).
 */
export function guard([deny, allow]) {
  const internals = window.__TAURI_INTERNALS__;
  if (!internals) return "no-tauri";
  if (internals.__ksGuard) {
    internals.__ksGuard.allow = allow;
    return internals.invoke === internals.__ksGuard.wrapped ? "on" : "failed";
  }
  const original = internals.invoke;
  const state = { allow, blocked: [], wrapped: null };
  state.wrapped = function (cmd, args, options) {
    if (deny.includes(cmd) && !state.allow.includes(cmd)) {
      state.blocked.push(cmd);
      return Promise.reject(new Error(`screenshot guard: "${cmd}" is not run while capturing`));
    }
    return original.call(this, cmd, args, options);
  };
  try {
    internals.invoke = state.wrapped;
    internals.__ksGuard = state;
  } catch {
    return "failed";
  }
  return internals.invoke === state.wrapped ? "on" : "failed";
}

/** Runs in the page: the commands the guard refused since the last call. */
export function blocked() {
  const state = window.__TAURI_INTERNALS__?.__ksGuard;
  if (!state) return [];
  return state.blocked.splice(0);
}
