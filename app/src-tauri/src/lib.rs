//! Klavye Stüdyo desktop shell.
//!
//! This crate is deliberately thin: it wires Tauri commands and events to
//! the `kb-*` crates. All keyboard logic lives in those crates, and all
//! hardware access goes through the single device actor.

mod device;
mod dto;
mod settings;

use device::{DeviceHandle, Expected, LiveFrame, MirrorExpect, Request};
use dto::{
    Calibration, CalibrationInfo, EffectInfo, EffectRequest, KeyboardInfo, LedColor, MirrorHint, ThemePreference,
    UiEnvironment,
};
use kb_core::color::{LedCalibration, GAMMA_MAX, GAMMA_MIN};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
            let calibration = match app.path().app_config_dir() {
                Ok(dir) => settings::load_calibration(&dir),
                Err(e) => {
                    tracing::warn!("no config directory: {e}");
                    LedCalibration::default()
                }
            };
            app.manage(Arc::new(device::spawn(app.handle().clone(), calibration)));
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
            set_window_theme
        ])
        .run(tauri::generate_context!())
        .expect("error while running Klavye Stüdyo");
}
