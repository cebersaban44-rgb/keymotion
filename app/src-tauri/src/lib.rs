//! Klavye Stüdyo desktop shell.
//!
//! This crate is deliberately thin: it wires Tauri commands and events to
//! the `kb-*` crates. All keyboard logic lives in those crates, and all
//! hardware access goes through the single device actor.

mod device;
mod dto;

use device::{DeviceHandle, Request};
use dto::{EffectInfo, EffectRequest, KeyboardInfo, LedColor, ThemePreference, UiEnvironment};
use std::sync::Arc;
use tauri::{Manager, State, WebviewWindow};

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
    tracing::info!(?request, "ipc: apply_effect");
    let settings = request.to_settings()?;
    let handle = device.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handle.call(|reply| Request::ApplyEffect(settings, reply)))
        .await
        .map_err(|e| e.to_string())?
}

/// Store per-key colours (flash write). Runs off the main thread.
#[tauri::command]
async fn apply_per_key(device: Device<'_>, colors: Vec<LedColor>, brightness: u8) -> Result<(), String> {
    tracing::info!(keys = colors.len(), brightness, "ipc: apply_per_key");
    let colors: Vec<(u8, [u8; 3])> = colors.into_iter().map(|c| (c.led, [c.r, c.g, c.b])).collect();
    let handle = device.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        handle.call(|reply| Request::ApplyPerKey { colors, brightness, reply })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Start/update (`Some`) or stop (`None`) the RAM-only live frame.
#[tauri::command]
fn set_live_frame(device: Device<'_>, colors: Option<Vec<LedColor>>) -> Result<(), String> {
    tracing::debug!(keys = colors.as_ref().map(Vec::len), "ipc: set_live_frame");
    let frame = colors.map(|c| c.into_iter().map(|c| (c.led, [c.r, c.g, c.b])).collect());
    device.send(Request::SetLive(frame))
}

/// Mirror the colours shown on the keyboard into the UI (`lighting-mirror` events).
#[tauri::command]
fn set_mirror(device: Device<'_>, enabled: bool) -> Result<(), String> {
    tracing::info!(enabled, "ipc: set_mirror");
    device.send(Request::SetMirror(enabled))
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
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("main window is declared in tauri.conf.json");
            let build = os_build();
            let mica = apply_backdrop(&window, build);
            app.manage(UiEnvironment {
                app_version: app.package_info().version.to_string(),
                mica,
                os_build: build,
            });
            app.manage(Arc::new(device::spawn(app.handle().clone())));
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
            ui_environment,
            set_window_theme
        ])
        .run(tauri::generate_context!())
        .expect("error while running Klavye Stüdyo");
}
