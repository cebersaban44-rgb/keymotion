//! Keymotion desktop shell.
//!
//! This crate is deliberately thin: it wires Tauri commands and events to
//! the `kb-*` crates. All keyboard logic lives in those crates, and all
//! hardware access goes through the single device actor.

mod backup;
mod calendar;
mod device;
mod diagnostics;
mod dto;
mod gauges;
mod ics;
mod lighting;
mod live;
mod logging;
pub mod migrate;
mod notify;
mod now_playing;
mod official_driver;
mod pomodoro;
mod profiles;
mod reminders;
mod screen_lib;
mod settings;

use backup::{BackupExported, BackupImported, BackupSection, BackupSummary, PendingImport};
use calendar::CalendarHandle;
use device::{DeviceHandle, Expected, LiveFrame, MirrorExpect, Request, ScreenWriteStep};
use diagnostics::{AppFolder, LastExport};
use dto::{
    CalendarSettings, CalendarState, Calibration, CalibrationInfo, ClockStatus, EffectInfo, EffectRequest, FlashWrite,
    GaugesSettings, GaugesState, KeyAssignment, KeyboardInfo, KeymapCatalog, KeymapLayer, KeymapState, LedColor,
    LivePlan, LiveState, MacroInfo, MacrosState, MirrorHint, PomodoroSettings, PomodoroState, Reminder,
    ReminderSettings, ReminderState, ScreenItem, ScreenItemKind, ScreenState, SettingsReset, SongStop, ThemePreference,
    UiEnvironment,
};
use gauges::GaugesHandle;
use kb_core::color::{LedCalibration, GAMMA_MAX, GAMMA_MIN};
use live::LiveHandle;
use now_playing::NowPlaying;
use pomodoro::{PomodoroAction, PomodoroHandle};
use profiles::{PartLoaded, ProfilePart, ProfileParts, ProfileSaved, ProfileStore, ProfilesState};
use reminders::RemindersHandle;
use screen_lib::ScreenLibrary;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

/// Event asking the UI to confirm closing while changes are unsaved.
const EVENT_CLOSE_REQUESTED: &str = "close-requested";
/// If the UI does not acknowledge a close request in time (for example a
/// crashed webview), the window closes anyway so the app can always quit;
/// a flash write in progress is waited for first.
const CLOSE_ACK_TIMEOUT: Duration = Duration::from_secs(2);

/// Whether the UI holds changes that are not stored on the keyboard yet.
#[derive(Default)]
struct CloseGuard {
    unsaved: AtomicBool,
    acked: AtomicBool,
}

/// A flash write runs (closing asks first, as with unsaved changes).
fn flash_busy(app: &AppHandle) -> bool {
    app.try_state::<Arc<DeviceHandle>>().is_some_and(|d| d.flash.current().is_some())
}

/// The official driver's copy of the factory animation: searched once per
/// run, on first ask (`None` until then).
#[derive(Default)]
struct DriverGif(Mutex<Option<Option<PathBuf>>>);

/// Search for the official driver's GIF (blocking; the first call reads
/// the program folders, later calls return the answer kept).
fn driver_gif(found: &DriverGif) -> Option<PathBuf> {
    let mut kept = found.0.lock().ok()?;
    if let Some(path) = kept.as_ref() {
        return path.clone();
    }
    use kb_protocol::sonix::screen::{HEIGHT, WIDTH};
    let started = Instant::now();
    let path = official_driver::find_factory_gif(&official_driver::search_roots(), WIDTH as u16, HEIGHT as u16);
    tracing::info!(found = path.is_some(), ms = started.elapsed().as_millis() as u64, "official driver searched");
    *kept = Some(path.clone());
    path
}

type Device<'a> = State<'a, Arc<DeviceHandle>>;
type Library<'a> = State<'a, Arc<Mutex<ScreenLibrary>>>;
type Live<'a> = State<'a, Arc<LiveHandle>>;
type Pomodoro<'a> = State<'a, Arc<PomodoroHandle>>;
type Gauges<'a> = State<'a, Arc<GaugesHandle>>;
type Calendar<'a> = State<'a, Arc<CalendarHandle>>;
type Reminders<'a> = State<'a, Arc<RemindersHandle>>;
type Profiles<'a> = State<'a, Arc<Mutex<ProfileStore>>>;

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

/// Whether AJAZZ's official driver left a copy of the factory animation on
/// this PC. The first call searches the program folders (off the UI
/// thread); the answer is kept for the run.
#[tauri::command]
async fn screen_find_driver_backup(found: State<'_, Arc<DriverGif>>) -> Result<bool, String> {
    let found = found.inner().clone();
    tauri::async_runtime::spawn_blocking(move || driver_gif(&found).is_some()).await.map_err(|e| e.to_string())
}

/// Back up the factory animation from the official driver's copy: kept as
/// the factory backup and added to the library as the factory item.
#[tauri::command]
async fn screen_backup_from_driver(
    found: State<'_, Arc<DriverGif>>,
    library: Library<'_>,
    live: Live<'_>,
    model_id: String,
) -> Result<ScreenState, String> {
    tracing::debug!("ipc: screen_backup_from_driver");
    let found = found.inner().clone();
    let (gif, anim) = tauri::async_runtime::spawn_blocking(move || {
        let path = driver_gif(&found).ok_or("no official driver copy found")?;
        let gif = std::fs::read(&path).map_err(|e| e.to_string())?;
        let anim = screen_lib::factory_animation(&gif)?;
        Ok::<_, String>((gif, anim))
    })
    .await
    .map_err(|e| e.to_string())??;
    let state = {
        let mut lib = lock_library(&library)?;
        lib.store_factory(&gif, &anim)?;
        lib.state(&model_id)
    };
    // The live mode no longer needs to ask about the factory animation.
    live.changed();
    Ok(state)
}

/// The flash write in progress, if any (`flash-write` events follow it).
#[tauri::command]
fn flash_write_state(device: Device<'_>) -> Option<FlashWrite> {
    device.flash.current()
}

/// The text Windows shows if the user shuts down during a flash write, in
/// the UI language.
#[tauri::command]
fn set_write_reason(device: Device<'_>, reason: String) {
    device.flash.set_reason(&reason);
}

/// Live mode, budget and sources.
#[tauri::command]
fn live_state(live: Live<'_>) -> Result<LiveState, String> {
    live.state()
}

/// The user accepted, once, that live cards erase the factory animation
/// (there is no backup of it).
#[tauri::command]
fn live_ack_factory(live: Live<'_>) -> Result<LiveState, String> {
    tracing::debug!("ipc: live_ack_factory");
    live.ack_factory()
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

/// The calendars and the meetings ahead.
#[tauri::command]
fn calendar_state(calendar: Calendar<'_>) -> Result<CalendarState, String> {
    calendar.state()
}

/// New calendar settings (saved).
#[tauri::command]
fn calendar_configure(calendar: Calendar<'_>, settings: CalendarSettings) -> Result<CalendarState, String> {
    tracing::debug!(?settings, "ipc: calendar_configure");
    calendar.configure(settings)
}

/// Add a calendar by its published (secret) address. The address is not
/// logged.
#[tauri::command]
fn calendar_add(calendar: Calendar<'_>, url: String, name: String) -> Result<CalendarState, String> {
    tracing::debug!("ipc: calendar_add");
    calendar.add(&url, &name)
}

/// Rename, recolour, or turn a calendar on or off.
#[tauri::command]
fn calendar_update(
    calendar: Calendar<'_>,
    id: String,
    name: Option<String>,
    color: Option<[u8; 3]>,
    enabled: Option<bool>,
) -> Result<CalendarState, String> {
    tracing::debug!(%id, ?enabled, "ipc: calendar_update");
    calendar.update(&id, name, color, enabled)
}

#[tauri::command]
fn calendar_remove(calendar: Calendar<'_>, id: String) -> Result<CalendarState, String> {
    tracing::debug!(%id, "ipc: calendar_remove");
    calendar.remove(&id)
}

/// Read every calendar now.
#[tauri::command]
fn calendar_refresh(calendar: Calendar<'_>) -> Result<CalendarState, String> {
    calendar.refresh()
}

/// Blink the reminder key for a few seconds.
#[tauri::command]
fn calendar_try_reminder(calendar: Calendar<'_>) {
    calendar.try_reminder();
}

/// A card the UI just handed to the live mode is a try: send it now and
/// bring the own picture back after a minute.
#[tauri::command]
fn calendar_try_card(calendar: Calendar<'_>) {
    calendar.try_card();
}

/// The reminders and when each goes off next.
#[tauri::command]
fn reminders_state(reminders: Reminders<'_>) -> Result<ReminderState, String> {
    reminders.state()
}

/// New reminder settings (saved).
#[tauri::command]
fn reminders_configure(reminders: Reminders<'_>, settings: ReminderSettings) -> Result<ReminderState, String> {
    tracing::debug!(?settings, "ipc: reminders_configure");
    reminders.configure(settings)
}

/// Add a reminder (empty id) or change one.
#[tauri::command]
fn reminders_save(reminders: Reminders<'_>, reminder: Reminder) -> Result<ReminderState, String> {
    reminders.save_reminder(reminder)
}

#[tauri::command]
fn reminders_remove(reminders: Reminders<'_>, id: String) -> Result<ReminderState, String> {
    reminders.remove(&id)
}

/// "Dene": the reminder's wave on the keys (and the sound), now.
#[tauri::command]
fn reminders_try(reminders: Reminders<'_>, color: [u8; 3], sound: bool) {
    reminders.try_light(color, sound);
}

/// A card the UI just handed to the live mode is a try: send it now and
/// bring the own picture back after a minute.
#[tauri::command]
fn reminders_try_card(reminders: Reminders<'_>) {
    reminders.try_card();
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

/// The user said to write on this untested firmware version ("1.07") for
/// the rest of the session (asked after a `firmware-unverified` refusal).
#[tauri::command]
fn confirm_firmware(device: Device<'_>, firmware: String) -> Result<(), String> {
    tracing::debug!(%firmware, "ipc: confirm_firmware");
    device.send(Request::ConfirmFirmware(firmware))
}

/// Settings files that could not be read at start-up and went back to
/// their defaults, not yet reported to the user.
#[tauri::command]
fn settings_resets() -> Vec<SettingsReset> {
    settings::unshown_resets().into_iter().map(SettingsReset::from).collect()
}

/// The UI showed the notice about the resets; do not report them again.
#[tauri::command]
fn dismiss_settings_resets() {
    settings::mark_resets_shown();
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

/// Open one of the project's own pages (the support list) in the browser.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    notify::open_project_page(&url)
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

// ------------------------------------------------------ data and diagnostics

/// The Lighting page's saved state (`lighting.ts`); `null` when none was
/// saved here yet (the UI then moves its old local copy over).
#[tauri::command]
fn lighting_state(app: AppHandle) -> Option<serde_json::Value> {
    let dir = app.path().app_config_dir().ok()?;
    lighting::load(&dir).map(serde_json::Value::Object)
}

#[tauri::command]
fn lighting_save(app: AppHandle, state: serde_json::Value) -> Result<(), String> {
    let state = lighting::check(state)?;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    lighting::save(&dir, state).map_err(|e| format!("lighting not saved: {e}"))
}

/// A UI failure (a page that crashed, an action that failed) for the log.
/// Situations the user can fix arrive as "action refused: …" and are logged
/// as warnings (lib/errors.ts `EXPECTED`).
#[tauri::command]
fn log_ui_error(message: String) {
    let message: String = message.chars().take(4_000).collect();
    if message.starts_with("action refused:") {
        tracing::warn!(target: "ui", "{message}");
    } else {
        tracing::error!(target: "ui", "{message}");
    }
}

/// The diagnostics report, plain text for the clipboard.
#[tauri::command]
async fn diagnostics_report(app: AppHandle, language: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let keyboards: Vec<String> = app
            .state::<Arc<DeviceHandle>>()
            .snapshot
            .lock()
            .map(|s| s.keyboards.iter().map(diagnostics::keyboard_line).collect())
            .unwrap_or_default();
        let facts = diagnostics::Facts {
            app_name: app.package_info().name.clone(),
            app_version: app.package_info().version.to_string(),
            time: chrono::Local::now().to_rfc3339(),
            windows: kb_winsys::os::windows_release()
                .map_or_else(|| format!("unknown (build {})", os_build()), |r| r.describe()),
            webview: tauri::webview_version().unwrap_or_else(|e| format!("unknown ({e})")),
            ui_language: language.chars().take(8).collect(),
            keyboards,
            data_files: app.path().app_config_dir().map(|d| diagnostics::data_files(&d)).unwrap_or_default(),
            log_lines: app
                .path()
                .app_log_dir()
                .map(|d| logging::recent_lines(&d, diagnostics::LOG_LINES))
                .unwrap_or_default(),
        };
        diagnostics::report(&facts)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Open the log or the data folder in Explorer.
#[tauri::command]
fn open_folder(app: AppHandle, folder: AppFolder) -> Result<(), String> {
    let dir = match folder {
        AppFolder::Logs => app.path().app_log_dir(),
        AppFolder::Data => app.path().app_config_dir(),
    }
    .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("folder not opened: {e}"))?;
    diagnostics::reveal(&dir, false)
}

/// Write a backup file into the Downloads folder (`backup.rs` has the
/// format). Calendar addresses go in only with `with_addresses`.
#[tauri::command]
async fn backup_export(app: AppHandle, with_addresses: bool, file_name: String) -> Result<BackupExported, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = backup::collect(&app, with_addresses)?.encode()?;
        let downloads = app.path().download_dir().ok();
        let in_downloads = downloads.is_some();
        let dir = downloads
            .or_else(|| app.path().document_dir().ok())
            .or_else(|| app.path().home_dir().ok())
            .ok_or("no folder to save the backup in")?;
        let path = backup::write_new(&dir, &file_name, &bytes).map_err(|e| format!("backup not saved: {e}"))?;
        tracing::info!(bytes = bytes.len(), with_addresses, "backup exported");
        let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Ok(mut last) = app.state::<LastExport>().0.lock() {
            *last = Some(path);
        }
        Ok(BackupExported { file, in_downloads })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Show the last exported file in Explorer.
#[tauri::command]
fn backup_reveal(last: State<'_, LastExport>) -> Result<(), String> {
    let path = last.0.lock().map_err(|_| "export unavailable")?.clone().ok_or("nothing exported yet")?;
    diagnostics::reveal(&path, true)
}

/// Read and check a backup file (the body is its bytes) and keep it for
/// `backup_import`. Nothing changes yet.
#[tauri::command]
async fn backup_inspect(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<BackupSummary, String> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return Err("expected binary data".into());
    };
    if data.len() > backup::MAX_BYTES {
        return Err(backup::ERR_TOO_LARGE.into());
    }
    let data = data.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let parsed = backup::parse(&data)?;
        drop(data);
        let library = app.state::<Arc<Mutex<ScreenLibrary>>>();
        let ids: std::collections::HashSet<String> =
            lock_library(&library)?.state("").items.into_iter().map(|i| i.id).collect();
        let (pending, summary) = backup::check(parsed, &|id| ids.contains(id))?;
        *app.state::<PendingImport>().0.lock().map_err(|_| "import unavailable")? = Some(pending);
        tracing::info!(sections = summary.sections.len(), "backup read");
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Restore the chosen parts of the backup `backup_inspect` read. Nothing is
/// written to the keyboard (see `backup.rs`).
#[tauri::command]
async fn backup_import(app: AppHandle, sections: Vec<BackupSection>) -> Result<BackupImported, String> {
    let pending = app
        .state::<PendingImport>()
        .0
        .lock()
        .map_err(|_| "import unavailable")?
        .take()
        .ok_or(backup::ERR_NOTHING_PENDING)?;
    tauri::async_runtime::spawn_blocking(move || backup::apply(&app, pending, &sections))
        .await
        .map_err(|e| e.to_string())
}

/// The user closed the import without restoring anything.
#[tauri::command]
fn backup_cancel(pending: State<'_, PendingImport>) {
    if let Ok(mut p) = pending.0.lock() {
        *p = None;
    }
}

// ----------------------------------------------------------------- profiles

fn lock_profiles(p: &Mutex<ProfileStore>) -> Result<std::sync::MutexGuard<'_, ProfileStore>, String> {
    p.lock().map_err(|_| "profiles unavailable".to_string())
}

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_millis() as f64)
}

/// What the keyboard of `model_id` has, as this computer recorded it: the
/// keymap and macro records, the Lighting page's record, the library item
/// on the GIF page (profiles.rs). Editors' unsaved drafts are not in it.
fn profile_current(app: &AppHandle, model_id: &str) -> Result<profiles::Current, String> {
    let (keymap, macros) = {
        let device = app.state::<Arc<DeviceHandle>>();
        let snap = device.snapshot.lock().map_err(|_| "device state unavailable")?;
        (
            profiles::ProfileKeymap::from_record(snap.keymaps.models.get(model_id)),
            snap.macros.models.get(model_id).map(|m| m.slots.clone()).unwrap_or_default(),
        )
    };
    let lighting = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|dir| lighting::load(&dir))
        .and_then(|state| profiles::ProfileLighting::from_record(&state));
    let screen = {
        let library = app.state::<Arc<Mutex<ScreenLibrary>>>();
        let state = lock_library(&library)?.state(model_id);
        let shown = state.shown.as_deref();
        state
            .items
            .iter()
            .find(|i| Some(i.id.as_str()) == shown)
            .map(|i| profiles::ProfileScreen { id: i.id.clone(), name: i.name.clone() })
    };
    Ok(profiles::Current {
        model_id: model_id.to_string(),
        lighting,
        keymap,
        macros: profiles::trimmed(macros),
        screen,
    })
}

/// The profiles for the keyboard of `model_id`: which one it has, what
/// loading each would write.
#[tauri::command]
fn profiles_state(app: AppHandle, profiles: Profiles<'_>, model_id: String) -> Result<ProfilesState, String> {
    let current = profile_current(&app, &model_id)?;
    let items: std::collections::HashMap<String, (String, u32)> = {
        let library = app.state::<Arc<Mutex<ScreenLibrary>>>();
        let state = lock_library(&library)?.state("");
        state.items.into_iter().map(|i| (i.id, (i.name, i.pages))).collect()
    };
    let lookup =
        |id: &str| items.get(id).map(|(name, pages)| profiles::LibraryItem { name: name.clone(), pages: *pages });
    Ok(lock_profiles(&profiles)?.state(&current, &lookup))
}

/// A new profile from what the keyboard has (the chosen parts).
#[tauri::command]
fn profiles_create(
    app: AppHandle,
    profiles: Profiles<'_>,
    model_id: String,
    name: String,
    parts: ProfileParts,
) -> Result<ProfileSaved, String> {
    tracing::debug!(?parts, "ipc: profiles_create");
    let current = profile_current(&app, &model_id)?;
    let p = lock_profiles(&profiles)?.create(&name, parts, &current, now_ms())?;
    tracing::info!(parts = ?p.parts(), "profile created");
    Ok(ProfileSaved { id: p.id, name: p.name })
}

/// The profile's parts from what the keyboard has now.
#[tauri::command]
fn profiles_update(app: AppHandle, profiles: Profiles<'_>, model_id: String, id: String) -> Result<(), String> {
    tracing::debug!(%id, "ipc: profiles_update");
    let current = profile_current(&app, &model_id)?;
    lock_profiles(&profiles)?.update(&id, &current, now_ms())
}

#[tauri::command]
fn profiles_rename(profiles: Profiles<'_>, id: String, name: String) -> Result<(), String> {
    tracing::debug!(%id, "ipc: profiles_rename");
    lock_profiles(&profiles)?.rename(&id, &name, now_ms())
}

/// A copy named `name` (made unique when taken).
#[tauri::command]
fn profiles_duplicate(profiles: Profiles<'_>, id: String, name: String) -> Result<ProfileSaved, String> {
    tracing::debug!(%id, "ipc: profiles_duplicate");
    let p = lock_profiles(&profiles)?.duplicate(&id, &name, now_ms())?;
    Ok(ProfileSaved { id: p.id, name: p.name })
}

#[tauri::command]
fn profiles_delete(profiles: Profiles<'_>, id: String) -> Result<(), String> {
    tracing::debug!(%id, "ipc: profiles_delete");
    lock_profiles(&profiles)?.delete(&id)
}

/// Write one profile to a file in the Downloads folder, its screen picture
/// with it (profiles.rs has the format).
#[tauri::command]
async fn profiles_export(
    app: AppHandle,
    profiles: Profiles<'_>,
    id: String,
    file_name: String,
) -> Result<BackupExported, String> {
    if id == profiles::DEFAULT_ID {
        return Err(profiles::ERR_BUILT_IN.into());
    }
    let profile = lock_profiles(&profiles)?.get(&id).ok_or(profiles::ERR_UNKNOWN)?;
    tauri::async_runtime::spawn_blocking(move || {
        let picture = profile.screen.as_ref().and_then(|s| {
            let library = app.state::<Arc<Mutex<ScreenLibrary>>>();
            let lib = library.lock().ok()?;
            let item = lib.state("").items.into_iter().find(|i| i.id == s.id)?;
            let data = lib.data(&s.id).map_err(|e| tracing::warn!("profile picture left out: {e}")).ok()?;
            Some((item, data))
        });
        let screen = picture.as_ref().map(|(i, data)| (i.name.as_str(), i.kind, i.created_ms, data.as_slice()));
        let bytes = profiles::export(&profile, screen, &app.package_info().version.to_string(), now_ms())?;
        let downloads = app.path().download_dir().ok();
        let in_downloads = downloads.is_some();
        let dir = downloads
            .or_else(|| app.path().document_dir().ok())
            .or_else(|| app.path().home_dir().ok())
            .ok_or("no folder to save the profile in")?;
        let path = backup::write_new(&dir, &file_name, &bytes).map_err(|e| format!("profile not saved: {e}"))?;
        tracing::info!(bytes = bytes.len(), "profile exported");
        let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Ok(mut last) = app.state::<LastExport>().0.lock() {
            *last = Some(path);
        }
        Ok(BackupExported { file, in_downloads })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Add the profile in a file (the body is its bytes). It is checked whole
/// first; its screen picture goes into the library.
#[tauri::command]
async fn profiles_import(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<ProfileSaved, String> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return Err("expected binary data".into());
    };
    if data.len() > profiles::MAX_FILE_BYTES {
        return Err(profiles::ERR_TOO_LARGE.into());
    }
    let data = data.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let imported = profiles::check_import(profiles::parse(&data)?)?;
        drop(data);
        let mut profile = imported.profile;
        if let Some((name, kind, anim)) = imported.picture {
            let library = app.state::<Arc<Mutex<ScreenLibrary>>>();
            let item =
                lock_library(&library)?.add(&name, kind, &anim).map_err(|e| format!("picture not saved: {e}"))?;
            profile.screen = Some(profiles::ProfileScreen { id: item.id, name: item.name });
        }
        let store = app.state::<Arc<Mutex<ProfileStore>>>();
        let p = lock_profiles(&store)?.add(profile, now_ms())?;
        tracing::info!(parts = ?p.parts(), "profile imported");
        Ok(ProfileSaved { id: p.id, name: p.name })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Writes of a profile load, through the device actor's requests: every
/// check before a write applies (several keyboards, other programs,
/// untested firmware) and each is announced as a flash write.
struct DeviceWriter<'a> {
    device: &'a DeviceHandle,
    config_dir: Option<PathBuf>,
}

impl profiles::Writer for DeviceWriter<'_> {
    fn effect(&mut self, effect: kb_protocol::sonix::EffectSettings) -> Result<(), String> {
        self.device.call(|reply| Request::ApplyEffect(effect, reply))
    }

    fn per_key(&mut self, colors: Vec<(u8, [u8; 3])>, brightness: u8) -> Result<(), String> {
        self.device.call(|reply| Request::ApplyPerKey { colors, brightness, reply })
    }

    fn keymap(
        &mut self,
        layer: kb_protocol::sonix::keymap::Layer,
        entries: Vec<(u8, kb_protocol::sonix::keymap::KeyAction)>,
    ) -> Result<(), String> {
        self.device.call(|reply| Request::ApplyKeymap { layer, entries, reply }).map(|_| ())
    }

    fn macros(&mut self, slots: Vec<Option<settings::StoredMacro>>) -> Result<(), String> {
        self.device.call(|reply| Request::ApplyMacros { slots, reply }).map(|_| ())
    }

    fn lighting_stored(&mut self, l: &profiles::ProfileLighting) -> Result<(), String> {
        let dir = self.config_dir.as_deref().ok_or("no config directory")?;
        let state = lighting::after_store(lighting::load(dir), l.stored());
        lighting::save(dir, state).map_err(|e| format!("lighting not saved: {e}"))
    }
}

/// Put one part of a profile on the USB keyboard (flash writes), unless it
/// has it already. The UI runs the parts in order and stops at a failure;
/// the screen part is a library upload (`screen_upload`).
#[tauri::command]
async fn profiles_load(
    app: AppHandle,
    device: Device<'_>,
    profiles: Profiles<'_>,
    model_id: String,
    id: String,
    part: ProfilePart,
) -> Result<PartLoaded, String> {
    tracing::debug!(%id, ?part, "ipc: profiles_load");
    let profile = lock_profiles(&profiles)?.get(&id).ok_or(profiles::ERR_UNKNOWN)?;
    let handle = device.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let current = profile_current(&app, &model_id)?;
        let mut writer = DeviceWriter { device: &handle, config_dir: app.path().app_config_dir().ok() };
        let result = profiles::load_part(&profile, part, &current, &mut writer);
        match &result {
            Ok(r) => tracing::info!(?part, ?r, "profile part loaded"),
            Err(e) => tracing::warn!(?part, "profile part not loaded: {e}"),
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn os_build() -> u32 {
    windows_version::OsVersion::current().build
}

#[cfg(not(windows))]
fn os_build() -> u32 {
    0
}

/// Shrink the (still hidden) main window when its configured size does not
/// fit the screen's work area, as on 1920×1080 at 125 % or 150 % scale,
/// then center it again. The configured size stays the upper bound and the
/// minimum size still applies.
fn fit_to_screen(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else { return };
    let (Ok(outer), Ok(inner)) = (window.outer_size(), window.inner_size()) else { return };
    let area = monitor.work_area().size;
    // The frame (title bar, borders) sits around the inner size.
    let frame_w = outer.width.saturating_sub(inner.width);
    let frame_h = outer.height.saturating_sub(inner.height);
    let margin = (monitor.scale_factor() * 16.0) as u32;
    let max_w = area.width.saturating_sub(frame_w + 2 * margin);
    let max_h = area.height.saturating_sub(frame_h + 2 * margin);
    if inner.width <= max_w && inner.height <= max_h {
        return;
    }
    let size = tauri::PhysicalSize::new(inner.width.min(max_w), inner.height.min(max_h));
    tracing::info!(width = size.width, height = size.height, "window shrunk to fit the screen");
    let _ = window.set_size(size);
    let _ = window.center();
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

/// Keeps the library's "incomplete" mark in step with the device actor's
/// screen uploads. The item to offer again: the one uploaded, or for a
/// live card the user's own picture.
fn screen_journal(library: &Arc<Mutex<ScreenLibrary>>) -> device::ScreenWriteHook {
    let library = library.clone();
    Box::new(move |step: ScreenWriteStep<'_>| {
        let Ok(mut lib) = library.lock() else { return };
        match step {
            ScreenWriteStep::Starting { label, model_id } => {
                let retry =
                    if label.starts_with("live:") { lib.live_own().map(|o| o.id) } else { Some(label.to_string()) };
                lib.write_starting(model_id, retry);
            }
            ScreenWriteStep::Ended { flash_written, ok } => lib.write_ended(flash_written, ok),
        }
    })
}

pub fn run() {
    logging::init();

    tauri::Builder::default()
        // One app at a time: two would send commands to the keyboard at
        // once. A second launch brings the open window forward instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(CloseGuard::default())
        .manage(Arc::new(DriverGif::default()))
        .manage(PendingImport::default())
        .manage(LastExport::default())
        .on_window_event(|window, event| {
            let tauri::WindowEvent::CloseRequested { api, .. } = event else { return };
            let guard = window.state::<CloseGuard>();
            // A flash write (a live card, going back to the own picture)
            // asks too, even if the UI has not heard of it yet.
            let writing = flash_busy(window.app_handle());
            if !guard.unsaved.load(Ordering::SeqCst) && !writing {
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
                    // Never cut a write short: wait for it, then close.
                    let since = Instant::now();
                    while flash_busy(window.app_handle()) && since.elapsed() < device::SCREEN_UPLOAD_TIMEOUT {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                    tracing::warn!("UI did not answer the close request; closing anyway");
                    let _ = window.destroy();
                }
            });
        })
        .setup(|app| {
            let version = app.package_info().version.to_string();
            match app.path().app_log_dir() {
                Ok(dir) => logging::set_dir(&dir, &version),
                Err(e) => tracing::warn!("no log folder: {e}"),
            }
            let window = app.get_webview_window("main").expect("main window is declared in tauri.conf.json");
            fit_to_screen(&window);
            let build = os_build();
            tracing::info!(%version, os_build = build, "app started");
            let mica = apply_backdrop(&window, build);
            app.manage(UiEnvironment { app_version: app.package_info().version.to_string(), mica, os_build: build });
            let config_dir = app.path().app_config_dir().map_err(|e| tracing::warn!("no config directory: {e}")).ok();
            let calibration = config_dir.as_deref().map(settings::load_calibration).unwrap_or_default();
            let library = Arc::new(Mutex::new(ScreenLibrary::open(config_dir.as_deref())));
            app.manage(Arc::new(Mutex::new(ProfileStore::open(config_dir.as_deref()))));
            let device = Arc::new(device::spawn(
                app.handle().clone(),
                calibration,
                config_dir.clone(),
                screen_journal(&library),
            ));
            let live = LiveHandle::spawn(app.handle().clone(), library.clone(), device.clone());
            app.manage(now_playing::spawn(app.handle().clone(), live.clone()));
            app.manage(PomodoroHandle::spawn(app.handle().clone(), live.clone(), device.clone(), config_dir.clone()));
            app.manage(GaugesHandle::spawn(app.handle().clone(), device.clone(), config_dir.clone()));
            app.manage(CalendarHandle::spawn(app.handle().clone(), live.clone(), device.clone(), config_dir.clone()));
            app.manage(RemindersHandle::spawn(app.handle().clone(), live.clone(), device.clone(), config_dir));
            app.manage(library);
            app.manage(device);
            app.manage(live);
            window.show()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            keyboards,
            refresh_keyboards,
            confirm_firmware,
            settings_resets,
            dismiss_settings_resets,
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
            open_link,
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
            screen_find_driver_backup,
            screen_backup_from_driver,
            flash_write_state,
            set_write_reason,
            live_state,
            live_ack_factory,
            live_configure,
            live_submit,
            live_send_now,
            now_playing_cover,
            pomodoro_state,
            pomodoro_control,
            pomodoro_configure,
            gauges_state,
            gauges_configure,
            gauges_watch,
            calendar_state,
            calendar_configure,
            calendar_add,
            calendar_update,
            calendar_remove,
            calendar_refresh,
            calendar_try_reminder,
            calendar_try_card,
            reminders_state,
            reminders_configure,
            reminders_save,
            reminders_remove,
            reminders_try,
            reminders_try_card,
            lighting_state,
            lighting_save,
            log_ui_error,
            diagnostics_report,
            open_folder,
            backup_export,
            backup_reveal,
            backup_inspect,
            backup_import,
            backup_cancel,
            profiles_state,
            profiles_create,
            profiles_update,
            profiles_rename,
            profiles_duplicate,
            profiles_delete,
            profiles_export,
            profiles_import,
            profiles_load
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keymotion");
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
