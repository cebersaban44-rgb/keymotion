//! Export and import of the app's own data in one file.
//!
//! # File format (version 1)
//!
//! One UTF-8 JSON object, written with indentation:
//!
//! ```text
//! {
//!   "kind": "kb-backup",            // always this
//!   "version": 1,                   // a newer number is refused
//!   "createdMs": 1794000000000,     // Unix ms
//!   "appVersion": "0.1.0",
//!   "calibration": { "gamma": [..], "gain": [..] },       // calibration.json
//!   "clock": { "autoSync": true },
//!   "keymaps": { "models": { … } },                         // keymap.json
//!   "macros": { "models": { … } },                          // macros.json
//!   "pomodoro": { … },                                      // pomodoro.json
//!   "gauges": { … },                                        // gauges.json
//!   "reminders": { "settings": { … }, "reminders": [ … ] },
//!   "calendar": { "settings": { … }, "sources": [ { "name", "url", "color", "enabled" } ] },
//!   "lighting": { … },                                      // lighting.json's state
//!   "screen": { "items": [ { "name", "kind", "createdMs", "data": "<base64>" } ] },
//!   "profiles": { "profiles": [ { "id", "name", "modelId", … } ] }  // profiles.json
//! }
//! ```
//!
//! Every section is optional; fields not known to this version are ignored.
//! `screen.items[].data` is the library item exactly as stored (the
//! 256-byte header and RGB565 frames, `kb-protocol` `screen::blob`), base64
//! (standard alphabet, padded); it is validated again on import. The
//! factory animation backup is never exported. `calendar.sources` exists
//! only when the user ticked "include calendar addresses": the addresses
//! are secrets and stand in the open in the file.
//!
//! # Import
//!
//! The file is read and checked first ([`inspect`]); the UI shows what it
//! holds and the user picks the parts to restore. Nothing is written to the
//! keyboard: keymaps and macros come back as unsaved edits in their
//! editors, the lighting design as an unsaved edit on the Lighting page
//! (this computer's record of what the keyboard has stays). Settings,
//! reminders and calendars replace this computer's; library items are added
//! to the library (the same picture is stored once). Profiles are added to
//! this computer's, and one with the same id replaces it. A profile's
//! screen picture is a library item, by id: it comes with the screen part.

use crate::calendar::{CalendarBackup, CalendarHandle};
use crate::device::{DeviceHandle, Request};
use crate::dto::{
    GaugesSettings, KeymapState, MacroInfo, PomodoroSettings, Reminder, ReminderSettings, ScreenItemKind,
};
use crate::gauges::GaugesHandle;
use crate::pomodoro::PomodoroHandle;
use crate::profiles::{Profile, ProfileStore};
use crate::reminders::RemindersHandle;
use crate::screen_lib::ScreenLibrary;
use crate::settings::{self, ClockSettings, Keymaps, MacroRecord};
use base64::Engine;
use kb_core::color::LedCalibration;
use kb_protocol::sonix::screen::{self, ScreenAnimation};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use ts_rs::TS;

pub const KIND: &str = "kb-backup";
pub const VERSION: u32 = 1;
/// Larger files are refused before they are read.
pub const MAX_BYTES: usize = 256 * 1024 * 1024;
/// Library items a file may hold.
const MAX_ITEMS: usize = 500;

/// Errors the UI words itself (`Settings.tsx`).
pub const ERR_NOT_BACKUP: &str = "not a backup";
pub const ERR_NEWER: &str = "backup from a newer version";
pub const ERR_DAMAGED: &str = "backup damaged";
pub const ERR_TOO_LARGE: &str = "backup too large";
pub const ERR_NOTHING_PENDING: &str = "no backup to import";

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

// ------------------------------------------------------------------ format

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub kind: String,
    pub version: u32,
    #[serde(default)]
    pub created_ms: f64,
    #[serde(default)]
    pub app_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<LedCalibration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<ClockBackup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymaps: Option<Keymaps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macros: Option<MacroRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pomodoro: Option<PomodoroSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gauges: Option<GaugesSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminders: Option<RemindersBackup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<CalendarBackup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenBackup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<ProfilesBackup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockBackup {
    pub auto_sync: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RemindersBackup {
    pub settings: ReminderSettings,
    pub reminders: Vec<Reminder>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScreenBackup {
    pub items: Vec<ScreenItemBackup>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfilesBackup {
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenItemBackup {
    pub name: String,
    pub kind: ScreenItemKind,
    #[serde(default)]
    pub created_ms: f64,
    /// Base64 of the stored blob (header + frames).
    pub data: String,
}

impl Backup {
    pub fn new(app_version: &str, created_ms: f64) -> Self {
        Self { kind: KIND.into(), version: VERSION, created_ms, app_version: app_version.into(), ..Self::default() }
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec_pretty(self).map_err(|e| e.to_string())
    }
}

impl ScreenItemBackup {
    pub fn new(name: &str, kind: ScreenItemKind, created_ms: f64, blob: &[u8]) -> Self {
        Self { name: name.into(), kind, created_ms, data: B64.encode(blob) }
    }
}

/// Read a backup file: its kind and version first (so a newer file says
/// so even when its sections changed), then the whole of it.
pub fn parse(bytes: &[u8]) -> Result<Backup, String> {
    if bytes.len() > MAX_BYTES {
        return Err(ERR_TOO_LARGE.into());
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ERR_NOT_BACKUP.to_string())?;
    if value.get("kind").and_then(Value::as_str) != Some(KIND) {
        return Err(ERR_NOT_BACKUP.into());
    }
    match value.get("version").and_then(Value::as_u64) {
        Some(v) if v == u64::from(VERSION) => {}
        Some(v) if v > u64::from(VERSION) => return Err(ERR_NEWER.into()),
        _ => return Err(ERR_DAMAGED.into()),
    }
    serde_json::from_value(value).map_err(|e| {
        tracing::info!("backup not read: {e}");
        ERR_DAMAGED.to_string()
    })
}

// ------------------------------------------------------------------ for the UI

/// A part of a backup the user can choose to restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum BackupSection {
    Lighting,
    Keymap,
    Macros,
    Screen,
    Calibration,
    Clock,
    Focus,
    Gauges,
    Reminders,
    Calendar,
    Profiles,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupSectionInfo {
    pub section: BackupSection,
    /// Macros, library items, reminders, calendars or profiles in it (0 for
    /// the others; for the calendar, 0 means its settings only).
    pub count: u32,
}

/// What a backup file holds (`backup_inspect`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupSummary {
    pub created_ms: f64,
    pub app_version: String,
    pub sections: Vec<BackupSectionInfo>,
    /// Library items not in this computer's library yet.
    pub screen_new: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportedKeymap {
    pub model_id: String,
    pub state: KeymapState,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportedMacros {
    pub model_id: String,
    pub slots: Vec<Option<MacroInfo>>,
}

/// The outcome of `backup_import`. Keymaps and macros are for the UI to
/// open as unsaved edits; nothing went to the keyboard.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupImported {
    pub keymaps: Vec<ImportedKeymap>,
    pub macros: Vec<ImportedMacros>,
    /// Parts that could not be restored.
    pub failed: Vec<BackupSection>,
    /// Reminders, calendars, pictures or profiles left out (invalid, over a
    /// limit, or a one-time reminder whose time has passed).
    pub skipped: u32,
}

/// Where an exported file went (`backup_export`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupExported {
    /// File name only.
    pub file: String,
    /// It is in the user's Downloads folder (else Documents or home).
    pub in_downloads: bool,
}

// ------------------------------------------------------------------ checking

struct ReadyItem {
    name: String,
    kind: ScreenItemKind,
    anim: ScreenAnimation,
}

/// A checked backup waiting for the user's choice.
pub struct Pending {
    backup: Backup,
    items: Vec<ReadyItem>,
}

/// The backup waiting for `backup_import` (managed state).
#[derive(Default)]
pub struct PendingImport(pub Mutex<Option<Pending>>);

/// Check everything an import would use, so it rarely fails half-way:
/// pictures decode and pass the keyboard's limits, keymaps and macros
/// belong to known models, the lighting state has the right shape.
/// `in_library` says whether a picture id is stored here already.
pub fn check(mut backup: Backup, in_library: &dyn Fn(&str) -> bool) -> Result<(Pending, BackupSummary), String> {
    let known = |id: &str| kb_devices::catalogue().is_ok_and(|c| c.iter().any(|m| m.id == id));
    if let Some(k) = backup.keymaps.as_mut() {
        k.models.retain(|id, _| known(id));
    }
    if let Some(m) = backup.macros.as_mut() {
        m.models.retain(|id, _| known(id));
    }
    if let Some(l) = backup.lighting.take() {
        backup.lighting = Some(crate::lighting::check(Value::Object(l)).map_err(|_| ERR_DAMAGED.to_string())?);
    }
    if let Some(p) = backup.profiles.take() {
        backup.profiles = Some(ProfilesBackup { profiles: crate::profiles::check_backup(p.profiles)? });
    }
    let mut items = Vec::new();
    let mut screen_new = 0;
    if let Some(s) = backup.screen.take() {
        if s.items.len() > MAX_ITEMS {
            return Err(ERR_TOO_LARGE.into());
        }
        for item in s.items {
            if item.kind.built_in() {
                continue;
            }
            let blob = B64.decode(item.data.as_bytes()).map_err(|_| ERR_DAMAGED.to_string())?;
            let anim = screen::from_blob(&blob).map_err(|_| ERR_DAMAGED.to_string())?;
            if !in_library(&crate::screen_lib::content_id(&screen::blob_unpadded(&anim))) {
                screen_new += 1;
            }
            items.push(ReadyItem { name: item.name, kind: item.kind, anim });
        }
    }

    let mut sections = Vec::new();
    let mut add = |section, count: usize| sections.push(BackupSectionInfo { section, count: count as u32 });
    if backup.lighting.is_some() {
        add(BackupSection::Lighting, 0);
    }
    if let Some(k) = &backup.keymaps {
        let layers = k.models.values().flat_map(|m| [&m.base, &m.fn_layer]).filter(|l| l.written_ms.is_some()).count();
        if layers > 0 {
            add(BackupSection::Keymap, layers);
        }
    }
    if let Some(m) = &backup.macros {
        let count = m.models.values().map(|m| m.slots.iter().flatten().count()).sum::<usize>();
        if m.models.values().any(|m| m.written_ms.is_some()) {
            add(BackupSection::Macros, count);
        }
    }
    if !items.is_empty() {
        add(BackupSection::Screen, items.len());
    }
    if backup.calibration.is_some() {
        add(BackupSection::Calibration, 0);
    }
    if backup.clock.is_some() {
        add(BackupSection::Clock, 0);
    }
    if backup.pomodoro.is_some() {
        add(BackupSection::Focus, 0);
    }
    if backup.gauges.is_some() {
        add(BackupSection::Gauges, 0);
    }
    if let Some(r) = &backup.reminders {
        add(BackupSection::Reminders, r.reminders.len());
    }
    if let Some(c) = &backup.calendar {
        add(BackupSection::Calendar, c.sources.as_ref().map_or(0, Vec::len));
    }
    if let Some(p) = backup.profiles.as_ref().filter(|p| !p.profiles.is_empty()) {
        add(BackupSection::Profiles, p.profiles.len());
    }
    let summary = BackupSummary {
        created_ms: backup.created_ms,
        app_version: backup.app_version.chars().take(32).collect(),
        sections,
        screen_new,
    };
    Ok((Pending { backup, items }, summary))
}

// ------------------------------------------------------------------ export

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_millis() as f64)
}

/// Everything this computer keeps, as one backup.
pub fn collect(app: &AppHandle, with_addresses: bool) -> Result<Backup, String> {
    let mut b = Backup::new(&app.package_info().version.to_string(), now_ms());
    {
        let device = app.state::<Arc<DeviceHandle>>();
        let snap = device.snapshot.lock().map_err(|_| "device state unavailable")?;
        b.calibration = Some(snap.calibration);
        b.clock = Some(ClockBackup { auto_sync: snap.clock.auto_sync });
        b.keymaps = Some(snap.keymaps.clone());
        b.macros = Some(snap.macros.clone());
    }
    b.pomodoro = Some(app.state::<Arc<PomodoroHandle>>().state()?.settings);
    b.gauges = Some(app.state::<Arc<GaugesHandle>>().state()?.settings);
    let reminders = app.state::<Arc<RemindersHandle>>().state()?;
    b.reminders = Some(RemindersBackup {
        settings: reminders.settings,
        reminders: reminders.reminders.into_iter().map(|r| r.reminder).collect(),
    });
    b.calendar = Some(app.state::<Arc<CalendarHandle>>().export(with_addresses)?);
    if let Ok(dir) = app.path().app_config_dir() {
        b.lighting = crate::lighting::load(&dir);
    }
    let library = app.state::<Arc<Mutex<ScreenLibrary>>>();
    let lib = library.lock().map_err(|_| "screen library unavailable")?;
    let mut items = Vec::new();
    // Built-in items stay out: the app has them already.
    for item in lib.state("").items.iter().filter(|i| !i.kind.built_in()) {
        match lib.data(&item.id) {
            Ok(blob) => items.push(ScreenItemBackup::new(&item.name, item.kind, item.created_ms, &blob)),
            Err(e) => tracing::warn!(id = %item.id, "library item left out of the backup: {e}"),
        }
    }
    b.screen = Some(ScreenBackup { items });
    drop(lib);
    let profiles = app.state::<Arc<Mutex<ProfileStore>>>();
    let profiles = profiles.lock().map_err(|_| "profiles unavailable")?.profiles().to_vec();
    b.profiles = Some(ProfilesBackup { profiles });
    Ok(b)
}

/// A file name the user's system accepts: no path or reserved characters,
/// not empty, `.json` at the end.
pub fn file_name(stem: &str) -> String {
    let stem = stem.trim();
    let stem = stem.strip_suffix(".json").unwrap_or(stem);
    let cleaned: String =
        stem.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '-' } else { c }).take(100).collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    let reserved = RESERVED.contains(&cleaned.to_ascii_uppercase().as_str())
        || (cleaned.len() == 4
            && ["COM", "LPT"].iter().any(|p| cleaned.to_ascii_uppercase().starts_with(p))
            && cleaned.as_bytes()[3].is_ascii_digit());
    match (cleaned.is_empty(), reserved) {
        (true, _) => "backup.json".into(),
        (false, true) => format!("{cleaned}-backup.json"),
        (false, false) => format!("{cleaned}.json"),
    }
}

/// `dir/name`, or `dir/name (2)` … when taken.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.strip_suffix(".json").unwrap_or(name);
    (2..1000).map(|n| dir.join(format!("{stem} ({n}).json"))).find(|p| !p.exists()).unwrap_or(first)
}

/// Write `bytes` to a new file in `dir` (whole or not at all).
pub fn write_new(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = unique_path(dir, &file_name(name));
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = fs::write(&tmp, bytes).and_then(|()| fs::rename(&tmp, &path)) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(path)
}

// ------------------------------------------------------------------ import

/// Restore the chosen parts of a checked backup. A part that fails is
/// reported and the others still go ahead.
pub fn apply(app: &AppHandle, pending: Pending, sections: &[BackupSection]) -> BackupImported {
    let Pending { backup: b, items } = pending;
    let mut out = BackupImported::default();
    let want = |s: BackupSection| sections.contains(&s);
    let run = |section: BackupSection, out: &mut BackupImported, f: &mut dyn FnMut() -> Result<u32, String>| {
        if !want(section) {
            return;
        }
        match f() {
            Ok(skipped) => out.skipped += skipped,
            Err(e) => {
                tracing::warn!(?section, "backup part not restored: {e}");
                out.failed.push(section);
            }
        }
    };
    let config_dir = app.path().app_config_dir().ok();
    let device = app.state::<Arc<DeviceHandle>>();

    if let Some(state) = b.lighting {
        run(BackupSection::Lighting, &mut out, &mut || {
            let dir = config_dir.as_deref().ok_or("no config directory")?;
            let merged = crate::lighting::merge_import(crate::lighting::load(dir), state.clone());
            crate::lighting::save(dir, merged).map_err(|e| format!("lighting not saved: {e}"))?;
            Ok(0)
        });
    }
    if let Some(cal) = b.calibration {
        run(BackupSection::Calibration, &mut out, &mut || {
            let cal = cal.sanitized();
            if let Ok(mut s) = device.snapshot.lock() {
                s.calibration = cal;
            }
            device.send(Request::SetCalibration(cal))?;
            if let Some(dir) = &config_dir {
                settings::save_calibration(dir, cal).map_err(|e| format!("calibration not saved: {e}"))?;
            }
            Ok(0)
        });
    }
    if let Some(clock) = b.clock {
        run(BackupSection::Clock, &mut out, &mut || {
            device.call(|reply| Request::SetClockAuto(clock.auto_sync, reply)).map(|_: ClockSettings| 0)
        });
    }
    if let Some(p) = b.pomodoro {
        run(BackupSection::Focus, &mut out, &mut || app.state::<Arc<PomodoroHandle>>().configure(p).map(|_| 0));
    }
    if let Some(g) = b.gauges.clone() {
        run(BackupSection::Gauges, &mut out, &mut || app.state::<Arc<GaugesHandle>>().configure(g.clone()).map(|_| 0));
    }
    if let Some(r) = b.reminders.clone() {
        run(BackupSection::Reminders, &mut out, &mut || {
            let handle = app.state::<Arc<RemindersHandle>>();
            handle.configure(r.settings.clone())?;
            for info in handle.state()?.reminders {
                handle.remove(&info.reminder.id)?;
            }
            let mut skipped = 0;
            for mut reminder in r.reminders.clone() {
                reminder.id.clear();
                if handle.save_reminder(reminder).is_err() {
                    skipped += 1;
                }
            }
            Ok(skipped)
        });
    }
    if let Some(c) = b.calendar.clone() {
        run(BackupSection::Calendar, &mut out, &mut || app.state::<Arc<CalendarHandle>>().import(c.clone()));
    }
    if !items.is_empty() {
        run(BackupSection::Screen, &mut out, &mut || {
            let library = app.state::<Arc<Mutex<ScreenLibrary>>>();
            let mut lib = library.lock().map_err(|_| "screen library unavailable")?;
            let mut skipped = 0;
            for item in &items {
                if let Err(e) = lib.add(&item.name, item.kind, &item.anim) {
                    tracing::warn!("library item not restored: {e}");
                    skipped += 1;
                }
            }
            Ok(skipped)
        });
    }
    if let Some(p) = b.profiles {
        run(BackupSection::Profiles, &mut out, &mut || {
            let store = app.state::<Arc<Mutex<ProfileStore>>>();
            let mut store = store.lock().map_err(|_| "profiles unavailable")?;
            store.merge(p.profiles.clone(), now_ms())
        });
    }
    if want(BackupSection::Keymap) {
        out.keymaps = b
            .keymaps
            .iter()
            .flat_map(|k| &k.models)
            .map(|(id, k)| ImportedKeymap { model_id: id.clone(), state: KeymapState::from(k) })
            .collect();
    }
    if want(BackupSection::Macros) {
        out.macros = b
            .macros
            .iter()
            .flat_map(|m| &m.models)
            .map(|(id, m)| ImportedMacros {
                model_id: id.clone(),
                slots: m.slots.iter().map(|s| s.as_ref().map(MacroInfo::from)).collect(),
            })
            .collect();
    }
    tracing::info!(?sections, failed = ?out.failed, skipped = out.skipped, "backup imported");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::CalendarSourceBackup;
    use crate::dto::CalendarSettings;
    use crate::settings::{StoredKeymap, StoredLayer, StoredMacro, StoredMacros, StoredStep};
    use kb_core::keymap::LayerMap;
    use kb_protocol::sonix::keymap::KeyAction;
    use kb_protocol::sonix::macros::{MacroStep, StepKind};
    use kb_protocol::sonix::screen::{rgb565, test_pattern, Frame};

    const MODEL: &str = "ajazz-ak980-pro";

    fn sample() -> Backup {
        let mut b = Backup::new("0.1.0", 1_794_000_000_000.0);
        b.calibration = Some(LedCalibration { gamma: [2.0, 2.4, 1.8], gain: [0.85, 1.0, 0.9] });
        b.clock = Some(ClockBackup { auto_sync: false });
        let mut keymaps = Keymaps::default();
        keymaps.models.insert(
            MODEL.into(),
            StoredKeymap {
                fn_layer: StoredLayer::new(&LayerMap::from([(33, KeyAction::key(0x04))]), 1),
                ..StoredKeymap::default()
            },
        );
        keymaps.models.insert("unknown-model".into(), StoredKeymap::default());
        b.keymaps = Some(keymaps);
        let mut macros = MacroRecord::default();
        macros.models.insert(
            MODEL.into(),
            StoredMacros {
                slots: vec![
                    Some(StoredMacro {
                        name: "imza".into(),
                        steps: vec![StoredStep(MacroStep::new(StepKind::KeyDown, 0x04, 20))],
                    }),
                    None,
                ],
                written_ms: Some(1),
            },
        );
        b.macros = Some(macros);
        b.pomodoro = Some(PomodoroSettings::default());
        b.gauges = Some(GaugesSettings::default());
        b.reminders = Some(RemindersBackup {
            settings: ReminderSettings::default(),
            reminders: vec![Reminder { title: "Su iç".into(), ..Reminder::default() }],
        });
        b.calendar = Some(CalendarBackup { settings: CalendarSettings::default(), sources: None });
        b.lighting = serde_json::json!({ "tab": "effects", "perKey": { "33": "#ff8a3d" } }).as_object().cloned();
        let still = ScreenAnimation::still(test_pattern());
        let anim = ScreenAnimation::new(vec![(Frame::filled(rgb565(255, 0, 0)), 50), (Frame::filled(0), 50)]).unwrap();
        b.screen = Some(ScreenBackup {
            items: vec![
                ScreenItemBackup::new("Desen", ScreenItemKind::Image, 1.0, &screen::blob_unpadded(&still)),
                ScreenItemBackup::new("Kırmızı", ScreenItemKind::Animation, 2.0, &screen::blob_unpadded(&anim)),
                ScreenItemBackup::new("AJAZZ", ScreenItemKind::Factory, 0.0, &screen::blob_unpadded(&still)),
            ],
        });
        let profile = Profile {
            id: "p1".into(),
            name: "Oyun".into(),
            created_ms: 1.0,
            updated_ms: 1.0,
            model_id: MODEL.into(),
            lighting: None,
            keymap: None,
            macros: Some(Vec::new()),
            screen: None,
        };
        let alien = Profile { id: "p2".into(), model_id: "unknown-model".into(), ..profile.clone() };
        b.profiles = Some(ProfilesBackup { profiles: vec![profile, alien] });
        b
    }

    #[test]
    fn a_backup_reads_back_as_written() {
        let b = sample();
        let bytes = b.encode().unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains(r#""kind": "kb-backup""#) && text.contains(r#""version": 1"#), "{text}");
        assert_eq!(parse(&bytes).unwrap(), b);
        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice(&bytes);
        assert_eq!(parse(&bom).unwrap(), b, "a byte order mark from an editor is fine");
        // Sections are optional; unknown fields are ignored.
        let small = parse(br#"{"kind":"kb-backup","version":1,"later":{"x":1}}"#).unwrap();
        assert_eq!(small.keymaps, None);
    }

    #[test]
    fn other_files_and_versions_are_refused() {
        assert_eq!(parse(b"not json"), Err(ERR_NOT_BACKUP.into()));
        assert_eq!(parse(br#"{"version":1}"#), Err(ERR_NOT_BACKUP.into()));
        assert_eq!(parse(br#"{"kind":"kb-backup","version":2,"keymaps":"changed"}"#), Err(ERR_NEWER.into()));
        assert_eq!(parse(br#"{"kind":"kb-backup"}"#), Err(ERR_DAMAGED.into()));
        assert_eq!(parse(br#"{"kind":"kb-backup","version":1,"clock":"x"}"#), Err(ERR_DAMAGED.into()));
    }

    #[test]
    fn checking_counts_what_can_be_restored() {
        let b = sample();
        let still_id = crate::screen_lib::content_id(&screen::blob_unpadded(&ScreenAnimation::still(test_pattern())));
        let (pending, summary) = check(b, &|id| id == still_id).unwrap();
        let count = |s| summary.sections.iter().find(|i| i.section == s).map(|i| i.count);
        assert_eq!(count(BackupSection::Keymap), Some(1));
        assert_eq!(count(BackupSection::Macros), Some(1));
        assert_eq!(count(BackupSection::Screen), Some(2), "the factory animation never comes in");
        assert_eq!(count(BackupSection::Reminders), Some(1));
        assert_eq!(count(BackupSection::Calendar), Some(0), "settings only: no addresses in the file");
        assert_eq!(count(BackupSection::Lighting), Some(0));
        assert_eq!(count(BackupSection::Profiles), Some(1), "a profile for an unknown model is left out");
        assert_eq!(summary.screen_new, 1, "one picture is in the library already");
        assert_eq!(pending.items.len(), 2);
        assert!(!pending.backup.keymaps.as_ref().unwrap().models.contains_key("unknown-model"));
        assert!(pending.backup.screen.is_none(), "the encoded copy is not kept");
    }

    #[test]
    fn damaged_pictures_and_lighting_are_refused() {
        let mut b = sample();
        b.screen.as_mut().unwrap().items[0].data = B64.encode([1u8, 2, 3]);
        assert_eq!(check(b, &|_| false).err(), Some(ERR_DAMAGED.into()));
        let mut b = sample();
        b.screen.as_mut().unwrap().items[0].data = "***".into();
        assert_eq!(check(b, &|_| false).err(), Some(ERR_DAMAGED.into()));
        let mut b = sample();
        b.lighting = serde_json::json!({ "perKey": "x".repeat(crate::lighting::MAX_BYTES) }).as_object().cloned();
        assert_eq!(check(b, &|_| false).err(), Some(ERR_DAMAGED.into()));
        let mut b = sample();
        b.profiles.as_mut().unwrap().profiles[0].macros = None;
        assert_eq!(check(b, &|_| false).err(), Some(ERR_DAMAGED.into()), "a profile with nothing in it");
        // A backup without the section is fine.
        let mut b = sample();
        b.profiles = None;
        let (_, summary) = check(b, &|_| false).unwrap();
        assert!(!summary.sections.iter().any(|i| i.section == BackupSection::Profiles));
    }

    #[test]
    fn calendar_addresses_are_in_the_file_only_when_asked() {
        let mut b = sample();
        let text = String::from_utf8(b.encode().unwrap()).unwrap();
        assert!(!text.contains("sources") && !text.contains("https://"), "{text}");
        b.calendar = Some(CalendarBackup {
            settings: CalendarSettings::default(),
            sources: Some(vec![CalendarSourceBackup {
                name: "İş".into(),
                url: "https://calendar.google.com/calendar/ical/x/private-abc/basic.ics".into(),
                color: [1, 2, 3],
                enabled: true,
            }]),
        });
        let parsed = parse(&b.encode().unwrap()).unwrap();
        let (_, summary) = check(parsed, &|_| false).unwrap();
        assert_eq!(summary.sections.iter().find(|i| i.section == BackupSection::Calendar).map(|i| i.count), Some(1));
    }

    #[test]
    fn export_files_get_safe_unique_names() {
        assert_eq!(file_name("Keymotion yedeği 2026-11-11"), "Keymotion yedeği 2026-11-11.json");
        assert_eq!(file_name(r"..\a/b:c*?.json"), "..-a-b-c--.json");
        assert_eq!(file_name("  "), "backup.json");
        assert_eq!(file_name("con"), "con-backup.json");
        assert_eq!(file_name("COM1"), "COM1-backup.json");
        assert_eq!(file_name("name. "), "name.json");
        let dir = std::env::temp_dir().join(format!("klavye-backup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let a = write_new(&dir, "Yedek", b"{}").unwrap();
        let b = write_new(&dir, "Yedek", b"{}").unwrap();
        assert_eq!(a.file_name().unwrap(), "Yedek.json");
        assert_eq!(b.file_name().unwrap(), "Yedek (2).json");
        assert_eq!(fs::read(&b).unwrap(), b"{}");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2, "no temporary file left");
        let _ = fs::remove_dir_all(&dir);
    }
}
