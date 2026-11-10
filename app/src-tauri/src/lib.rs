//! Klavye Stüdyo desktop shell.
//!
//! This crate is deliberately thin: it wires Tauri commands and events to
//! the `kb-*` crates. All keyboard logic lives in those crates, and all
//! hardware access goes through the single device actor.

mod device;
mod dto;
mod gauges;
mod live;
mod now_playing;
mod pomodoro;
mod screen_lib;
mod settings;

use device::{DeviceHandle, Expected, LiveFrame, MirrorExpect, Request};
use dto::{
    Calibration, CalibrationInfo, ClockStatus, EffectInfo, EffectRequest, GaugesSettings, GaugesState, KeyAssignment,
    KeyboardInfo, KeymapCatalog, KeymapLayer, KeymapState, LedColor, LivePlan, LiveState, MacroInfo, MacrosState,
    MirrorHint, PomodoroSettings, PomodoroState, ScreenItem, ScreenItemKind, ScreenState, SongStop, ThemePreference,
    UiEnvironment,
};
use gauges::GaugesHandle;
use kb_core::color::{LedCalibration, GAMMA_MAX, GAMMA_MIN};
use live::LiveHandle;
use now_playing::NowPlaying;
use pomodoro::{PomodoroAction, PomodoroHandle};
use screen_lib::ScreenLibrary;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

/// Event asking the UI to confirm closing while changes are unsaved.
const EVENT_CLOSE_REQUESTED: &str = "close-requested";
/// If the UI does not acknowledge a close request in time (for example a
/// crashed webview), the window closes anyway so the app can always quit.
const CLOSE_ACK_TIMEOUT: Duration = Duration::from_secs(2);

/// Whether the UI holds changes that are not stored on the keyboard yet.
#[derive(Default)]
struct CloseGuard {
    unsaved: AtomicBool,
    acked: AtomicBool,
}

type Device<'a> = State<'a, Arc<DeviceHandle>>;
type Library<'a> = State<'a, Arc<Mutex<ScreenLibrary>>>;
type Live<'a> = State<'a, Arc<LiveHandle>>;
type Pomodoro<'a> = State<'a, Arc<PomodoroHandle>>;
type Gauges<'a> = State<'a, Arc<GaugesHandle>>;

fn lock_library(lib: &Mutex<ScreenLibrary>) -> Result<std::sync::MutexGuard<'_, ScreenLibrary>, String> {
    lib.lock().map_err(|_| "screen library unavailable".to_string())
}

/// The screen library and what this model's GIF page shows.
#[tauri::command]
fn screen_state(library: Library<'_>, model_id: String) -> Result<ScreenState, String> {
    Ok(lock_library(&library)?.state(&model_id))
}

/// Store an animation the UI prepared. The body is the blob (header +
/// RGB565 frames, kb-protocol `screen::blob`); headers `x-name` (URI
/// encoded) and `x-kind` (`image` | `animation`). Everything is validated
/// again here: the UI is not trusted with the keyboard's format.
#[tauri::command]
fn screen_add(library: Library<'_>, request: tauri::ipc::Request<'_>) -> Result<ScreenItem, String> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return Err("expected binary data".into());
    };
    let anim = kb_protocol::sonix::screen::from_blob(data).map_err(|e| e.to_string())?;
    let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
    let name = percent_decode(header("x-name"));
    let kind = match header("x-kind") {
        "animation" => ScreenItemKind::Animation,
        _ if anim.frames().len() > 1 => ScreenItemKind::Animation,
        _ => ScreenItemKind::Image,
    };
    tracing::debug!(frames = anim.frames().len(), %name, "ipc: screen_add");
    lock_library(&library)?.add(&name, kind, &anim).map_err(|e| e.to_string())
}

/// Decode `%XX` escapes (the UI sends names with `encodeURIComponent`).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Stored data of a library item (header + frames), at most `frames`
/// frames, as raw bytes for previews.
#[tauri::command]
fn screen_data(library: Library<'_>, id: String, frames: Option<u32>) -> Result<tauri::ipc::Response, String> {
    let mut data = lock_library(&library)?.data(&id)?;
    if let Some(n) = frames {
        use kb_protocol::sonix::screen::{FRAME_BYTES, HEADER_LEN};
        data.truncate(HEADER_LEN + n.max(1) as usize * FRAME_BYTES);
    }
    Ok(tauri::ipc::Response::new(data))
}

#[tauri::command]
fn screen_rename(library: Library<'_>, model_id: String, id: String, name: String) -> Result<ScreenState, String> {
    let mut lib = lock_library(&library)?;
    lib.rename(&id, &name).map_err(|e| e.to_string())?;
    Ok(lib.state(&model_id))
}

#[tauri::command]
fn screen_remove(library: Library<'_>, model_id: String, id: String) -> Result<ScreenState, String> {
    let mut lib = lock_library(&library)?;
    lib.remove(&id).map_err(|e| e.to_string())?;
    Ok(lib.state(&model_id))
}

/// Upload a library item to the USB keyboard's GIF page (flash write, up
/// to ~5 minutes; `screen-upload-progress` events report the pages).
#[tauri::command]
async fn screen_upload(
    device: Device<'_>,
    library: Library<'_>,
    live: Live<'_>,
    model_id: String,
    id: String,
) -> Result<ScreenState, String> {
    tracing::debug!(%id, "ipc: screen_upload");
    let anim = lock_library(&library)?.animation(&id)?;
    let handle = device.inner().clone();
    let (result, id) = tauri::async_runtime::spawn_blocking(move || (handle.upload_screen(id.clone(), anim), id))
        .await
        .map_err(|e| e.to_string())?;
    let mut lib = lock_library(&library)?;
    let model = result.model_id.clone().unwrap_or(model_id.clone());
    match &result.error {
        None => lib.uploaded(&model, &id).map_err(|e| e.to_string())?,
        Some(_) if result.flash_written => {
            lib.count_upload(true);
            if let Err(e) = lib.save() {
                tracing::warn!("upload count not saved: {e}");
            }
        }
        Some(_) => {}
    }
    let state = lib.state(&model_id);
    drop(lib);
    // The upload used the live budget.
    live.changed();
    match result.error {
        None => Ok(state),
        Some(e) => Err(e),
    }
}

/// Live mode, budget and sources.
#[tauri::command]
fn live_state(live: Live<'_>) -> Result<LiveState, String> {
    live.state()
}

/// Change the live mode switch, the budget plan or a source switch (fields
/// left out stay as they are). `restore` with `enabled: false` puts back
/// what the GIF page showed before the live mode (a flash write).
#[tauri::command]
fn live_configure(
    live: Live<'_>,
    enabled: Option<bool>,
    plan: Option<LivePlan>,
    now_playing: Option<bool>,
    restore: Option<bool>,
    song_stop: Option<SongStop>,
) -> Result<LiveState, String> {
    tracing::debug!(?enabled, ?plan, ?now_playing, ?restore, ?song_stop, "ipc: live_configure");
    live.configure(enabled, plan, now_playing, restore, song_stop)
}

/// Send the waiting live card now instead of waiting for the budget.
#[tauri::command]
fn live_send_now(live: Live<'_>) -> Result<LiveState, String> {
    tracing::debug!("ipc: live_send_now");
    live.send_now()
}

/// A finished live card from the UI: the blob as body (validated here),
/// headers `x-source` and `x-name` (URI encoded). It waits for the budget
/// and the cable; the newest card wins.
#[tauri::command]
fn live_submit(live: Live<'_>, request: tauri::ipc::Request<'_>) -> Result<LiveState, String> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return Err("expected binary data".into());
    };
    let anim = kb_protocol::sonix::screen::from_blob(data).map_err(|e| e.to_string())?;
    let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
    let source = header("x-source").to_string();
    if source.is_empty() || source.len() > 32 || !source.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
        return Err("bad live source".into());
    }
    live.submit(anim, &source, &percent_decode(header("x-name")))
}

/// Cover art of the announced track (JPEG or PNG bytes).
#[tauri::command]
fn now_playing_cover(now_playing: State<'_, Arc<NowPlaying>>) -> Result<tauri::ipc::Response, String> {
    now_playing.cover().map(tauri::ipc::Response::new).ok_or_else(|| "no cover".to_string())
}

/// The Pomodoro timer.
#[tauri::command]
fn pomodoro_state(pomodoro: Pomodoro<'_>) -> Result<PomodoroState, String> {
    pomodoro.state()
}

/// Start, pause, skip or stop the Pomodoro.
#[tauri::command]
fn pomodoro_control(pomodoro: Pomodoro<'_>, action: PomodoroAction) -> Result<PomodoroState, String> {
    tracing::debug!(?action, "ipc: pomodoro_control");
    pomodoro.control(action)
}

/// New Pomodoro settings (saved).
#[tauri::command]
fn pomodoro_configure(pomodoro: Pomodoro<'_>, settings: PomodoroSettings) -> Result<PomodoroState, String> {
    tracing::debug!(?settings, "ipc: pomodoro_configure");
    pomodoro.configure(settings)
}

/// The system gauges.
#[tauri::command]
fn gauges_state(gauges: Gauges<'_>) -> Result<GaugesState, String> {
    gauges.state()
}

/// New system gauges settings (saved).
#[tauri::command]
fn gauges_configure(gauges: Gauges<'_>, settings: GaugesSettings) -> Result<GaugesState, String> {
    tracing::debug!(?settings, "ipc: gauges_configure");
    gauges.configure(settings)
}

/// The gauges page opened or closed: it gets readings (`gauges` events)
/// while open, even with the gauges off.
#[tauri::command]
fn gauges_watch(gauges: Gauges<'_>, on: bool) {
    gauges.watch(on);
}

#[tauri::command]
fn keyboards(device: Device<'_>) -> Vec<KeyboardInfo> {
    device.snapshot.lock().map(|s| s.keyboards.clone()).unwrap_or_default()
}

#[tauri::command]
fn refresh_keyboards(device: Device<'_>) -> Result<(), String> {
    device.send(Request::Refresh)
}

#[tauri::command]
fn lighting_catalog() -> Vec<EffectInfo> {
    EffectInfo::catalog()
}

/// Store a firmware effect (flash write). Runs off the main thread.
#[tauri::command]
async fn apply_effect(device: Device<'_>, request: EffectRequest) -> Result<(), String> {
    tracing::debug!(?request, "ipc: apply_effect");
    let settings = request.to_settings()?;
    let handle = device.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handle.call(|reply| Request::ApplyEffect(settings, reply)))
        .await
        .map_err(|e| e.to_string())?
}

/// Store per-key colours (flash write). Runs off the main thread.
#[tauri::command]
async fn apply_per_key(device: Device<'_>, colors: Vec<LedColor>, brightness: u8) -> Result<(), String> {
    tracing::debug!(keys = colors.len(), brightness, "ipc: apply_per_key");
    let colors: Vec<(u8, [u8; 3])> = colors.into_iter().map(|c| (c.led, [c.r, c.g, c.b])).collect();
    let handle = device.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        handle.call(|reply| Request::ApplyPerKey { colors, brightness, reply })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Start/update (`Some`) or stop (`None`) the RAM-only live frame.
/// `brightness` (0–5, default 5) dims after calibration with the
/// firmware's own brightness scale, so a preview matches the saved design.
#[tauri::command]
fn set_live_frame(device: Device<'_>, colors: Option<Vec<LedColor>>, brightness: Option<u8>) -> Result<(), String> {
    tracing::trace!(keys = colors.as_ref().map(Vec::len), brightness, "ipc: set_live_frame");
    let level = kb_protocol::sonix::level_factor(brightness.unwrap_or(5));
    let frame = colors.map(|c| LiveFrame { colors: c.into_iter().map(|c| (c.led, [c.r, c.g, c.b])).collect(), level });
    device.send(Request::SetLive(frame))
}

/// Everything a key can be assigned.
#[tauri::command]
fn keymap_catalog() -> KeymapCatalog {
    KeymapCatalog::build()
}

/// What the app last wrote to this model's keymap. The keyboard cannot
/// report its keymap; layers never written are the factory default.
#[tauri::command]
fn keymap_state(device: Device<'_>, model_id: String) -> KeymapState {
    let snap = device.snapshot.lock();
    snap.ok().and_then(|s| s.keymaps.models.get(&model_id).map(KeymapState::from)).unwrap_or_default()
}

/// Write one keymap layer of the USB keyboard (flash write) and record it.
/// The whole layer is replaced: keys not listed go back to the default.
#[tauri::command]
async fn apply_keymap(
    device: Device<'_>,
    layer: KeymapLayer,
    assignments: Vec<KeyAssignment>,
) -> Result<KeymapState, String> {
    tracing::debug!(?layer, keys = assignments.len(), "ipc: apply_keymap");
    let entries = assignments.into_iter().map(|a| (a.led, a.action.into())).collect();
    let handle = device.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        handle.call(|reply| Request::ApplyKeymap { layer: layer.into(), entries, reply })
    })
    .await
    .map_err(|e| e.to_string())?
    .map(|k| KeymapState::from(&k))
}

/// What the app last wrote to this model's macro memory, with the limits.
#[tauri::command]
fn macros_state(device: Device<'_>, model_id: String) -> MacrosState {
    let snap = device.snapshot.lock();
    let stored = snap.ok().and_then(|s| s.macros.models.get(&model_id).cloned()).unwrap_or_default();
    MacrosState::from(&stored)
}

/// Write every macro of the USB keyboard (flash write) and record them.
/// Slot `i` is macro `i`; `null` leaves a slot unused so later slots keep
/// their numbers.
#[tauri::command]
async fn apply_macros(device: Device<'_>, slots: Vec<Option<MacroInfo>>) -> Result<MacrosState, String> {
    tracing::debug!(slots = slots.len(), "ipc: apply_macros");
    let slots = slots.into_iter().map(|m| m.map(Into::into)).collect();
    let handle = device.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handle.call(|reply| Request::ApplyMacros { slots, reply }))
        .await
        .map_err(|e| e.to_string())?
        .map(|m| MacrosState::from(&m))
}

/// Keyboard clock sync state (`clock-changed` events report updates).
#[tauri::command]
fn clock_status(device: Device<'_>) -> ClockStatus {
    ClockStatus::from(device.snapshot.lock().map(|s| s.clock).unwrap_or_default())
}

/// Set the keyboard clock to the PC's local time now (USB only). The
/// keyboard's screen switches to its clock page.
#[tauri::command]
async fn sync_clock(device: Device<'_>) -> Result<ClockStatus, String> {
    tracing::debug!("ipc: sync_clock");
    let handle = device.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handle.call(Request::SyncClock))
        .await
        .map_err(|e| e.to_string())?
        .map(ClockStatus::from)
}

/// Turn automatic clock sync (on USB connect, at most once a day) on or off.
#[tauri::command]
fn set_clock_auto(device: Device<'_>, enabled: bool) -> Result<ClockStatus, String> {
    tracing::debug!(enabled, "ipc: set_clock_auto");
    device.call(|reply| Request::SetClockAuto(enabled, reply)).map(ClockStatus::from)
}

/// Mirror the colours shown on the keyboard into the UI (`lighting-mirror`
/// events). `hint` says what the app last saved to the keyboard; it
/// resolves the read-back's ambiguous red channel.
#[tauri::command]
fn set_mirror(device: Device<'_>, enabled: bool, hint: Option<MirrorHint>) -> Result<(), String> {
    tracing::debug!(enabled, ?hint, "ipc: set_mirror");
    if !enabled {
        return device.send(Request::SetMirror(None));
    }
    let expect = match hint {
        None => MirrorExpect { level: kb_protocol::sonix::MAX_LEVEL, colors: Expected::Unknown },
        Some(h) => {
            let colors = if let Some(keys) = h.keys {
                Expected::PerKey(keys.into_iter().map(|c| (c.led, [c.r, c.g, c.b])).collect())
            } else if let Some(hex) = h.color {
                Expected::Uniform(dto::parse_hex(&hex)?)
            } else {
                Expected::Unknown
            };
            MirrorExpect { level: h.brightness, colors }
        }
    };
    device.send(Request::SetMirror(Some(expect)))
}

#[tauri::command]
fn calibration(device: Device<'_>) -> CalibrationInfo {
    let current = device.snapshot.lock().map(|s| s.calibration).unwrap_or_default();
    CalibrationInfo {
        current: current.into(),
        factory: LedCalibration::DEFAULT.into(),
        gamma_min: GAMMA_MIN,
        gamma_max: GAMMA_MAX,
    }
}

/// Use a new calibration at once; with `persist` also save it to disk.
/// The UI previews while a slider moves and persists when it settles.
#[tauri::command]
fn set_calibration(app: AppHandle, device: Device<'_>, calibration: Calibration, persist: bool) -> Result<(), String> {
    let cal = LedCalibration::from(calibration);
    if let Ok(mut s) = device.snapshot.lock() {
        s.calibration = cal;
    }
    device.send(Request::SetCalibration(cal))?;
    if persist {
        let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
        settings::save_calibration(&dir, cal).map_err(|e| format!("calibration not saved: {e}"))?;
        tracing::info!(?cal, "calibration saved");
    }
    Ok(())
}

/// The UI reports whether any page has changes not stored on the keyboard.
#[tauri::command]
fn set_unsaved(guard: State<'_, CloseGuard>, unsaved: bool) {
    guard.unsaved.store(unsaved, Ordering::SeqCst);
}

/// The UI is showing the unsaved-changes dialog for a close request.
#[tauri::command]
fn ack_close_request(guard: State<'_, CloseGuard>) {
    guard.acked.store(true, Ordering::SeqCst);
}

/// Close the window after the user chose to save or discard. `destroy`
/// skips the close-requested hook, so the dialog does not come back.
#[tauri::command]
fn close_window(window: WebviewWindow) -> Result<(), String> {
    window.destroy().map_err(|e| e.to_string())
}

#[tauri::command]
fn ui_environment(env: State<'_, UiEnvironment>) -> UiEnvironment {
    env.inner().clone()
}

#[tauri::command]
fn set_window_theme(window: WebviewWindow, theme: ThemePreference) -> Result<(), String> {
    let t = match theme {
        ThemePreference::System => None,
        ThemePreference::Dark => Some(tauri::Theme::Dark),
        ThemePreference::Light => Some(tauri::Theme::Light),
    };
    window.set_theme(t).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn os_build() -> u32 {
    windows_version::OsVersion::current().build
}

#[cfg(not(windows))]
fn os_build() -> u32 {
    0
}

/// Apply the Windows 11 Mica backdrop when the OS supports it.
fn apply_backdrop(window: &WebviewWindow, build: u32) -> bool {
    if !cfg!(windows) || build < 22000 {
        return false;
    }
    use tauri::window::{Effect, EffectsBuilder};
    match window.set_effects(EffectsBuilder::new().effect(Effect::Mica).build()) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("Mica unavailable: {e}");
            false
        }
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .manage(CloseGuard::default())
        .on_window_event(|window, event| {
            let tauri::WindowEvent::CloseRequested { api, .. } = event else { return };
            let guard = window.state::<CloseGuard>();
            if !guard.unsaved.load(Ordering::SeqCst) {
                return;
            }
            api.prevent_close();
            guard.acked.store(false, Ordering::SeqCst);
            if let Err(e) = window.emit(EVENT_CLOSE_REQUESTED, ()) {
                tracing::warn!("close request not delivered: {e}");
            }
            let window = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(CLOSE_ACK_TIMEOUT);
                if !window.state::<CloseGuard>().acked.load(Ordering::SeqCst) {
                    tracing::warn!("UI did not answer the close request; closing anyway");
                    let _ = window.destroy();
                }
            });
        })
        .setup(|app| {
            let window = app.get_webview_window("main").expect("main window is declared in tauri.conf.json");
            let build = os_build();
            let mica = apply_backdrop(&window, build);
            app.manage(UiEnvironment { app_version: app.package_info().version.to_string(), mica, os_build: build });
            let config_dir = app.path().app_config_dir().map_err(|e| tracing::warn!("no config directory: {e}")).ok();
            let calibration = config_dir.as_deref().map(settings::load_calibration).unwrap_or_default();
            let library = Arc::new(Mutex::new(ScreenLibrary::open(config_dir.as_deref())));
            let device = Arc::new(device::spawn(app.handle().clone(), calibration, config_dir.clone()));
            let live = LiveHandle::spawn(app.handle().clone(), library.clone(), device.clone());
            app.manage(now_playing::spawn(app.handle().clone(), live.clone()));
            app.manage(PomodoroHandle::spawn(app.handle().clone(), live.clone(), device.clone(), config_dir.clone()));
            app.manage(GaugesHandle::spawn(app.handle().clone(), device.clone(), config_dir));
            app.manage(library);
            app.manage(device);
            app.manage(live);
            window.show()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            keyboards,
            refresh_keyboards,
            lighting_catalog,
            apply_effect,
            apply_per_key,
            set_live_frame,
            set_mirror,
            calibration,
            set_calibration,
            set_unsaved,
            ack_close_request,
            close_window,
            ui_environment,
            set_window_theme,
            clock_status,
            sync_clock,
            set_clock_auto,
            keymap_catalog,
            keymap_state,
            apply_keymap,
            macros_state,
            apply_macros,
            screen_state,
            screen_add,
            screen_data,
            screen_rename,
            screen_remove,
            screen_upload,
            live_state,
            live_configure,
            live_submit,
            live_send_now,
            now_playing_cover,
            pomodoro_state,
            pomodoro_control,
            pomodoro_configure,
            gauges_state,
            gauges_configure,
            gauges_watch
        ])
        .run(tauri::generate_context!())
        .expect("error while running Klavye Stüdyo");
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn decodes_names_from_the_ui() {
        // encodeURIComponent("Kedi ğüşıöç 1/2") in the UI.
        assert_eq!(percent_decode("Kedi%20%C4%9F%C3%BC%C5%9F%C4%B1%C3%B6%C3%A7%201%2F2"), "Kedi ğüşıöç 1/2");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("bad%zz%4"), "bad%zz%4", "broken escapes stay as they are");
    }
}
