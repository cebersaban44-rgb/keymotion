//! Updates (roadmap stage 6). The release pipeline puts `latest.json` on
//! GitHub's release page; the updater plugin reads it, checks the download
//! against the public key in `tauri.conf.json` and runs the installer.
//!
//! Nothing installs by itself. If the user lets it, the app looks once a
//! day (the first look a little after it starts, then hourly whether a day
//! has passed, as it may run in the tray for days); the UI shows what it
//! found and the user chooses to install. Installing waits for a write to
//! the keyboard to finish; on Windows the app then closes, the installer
//! runs with a small progress window and opens the new version.
//!
//! A test version ("0.1.0-rc.1") also looks at GitHub's pre-releases, so
//! an update from one test version to the next can be tried for real; a
//! normal version only ever sees normal releases.

use crate::dto::{UpdateFailure, UpdateState, UpdateStatus};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

pub const EVENT_UPDATE: &str = "update-state";

const FILE: &str = "updates.json";
const VERSION: u32 = 1;
const DAY_MS: f64 = 24.0 * 3_600_000.0;
/// The first look waits until the app has settled after starting.
const FIRST_LOOK_AFTER: Duration = Duration::from_secs(30);
/// While the app runs, whether a day has passed is checked this often.
const LOOK_EVERY: Duration = Duration::from_secs(3600);
/// One request to GitHub, at most.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Download progress goes to the UI at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);
/// GitHub's list of releases, newest first (test versions only).
const RELEASES_API: &str = "https://api.github.com/repos/cebersaban44-rgb/keymotion/releases?per_page=10";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    auto_check: bool,
    last_check_ms: Option<f64>,
}

impl Default for Stored {
    fn default() -> Self {
        Self { auto_check: true, last_check_ms: None }
    }
}

/// A day has passed since the last look (or the clock went back), and the
/// user lets the app look by itself.
fn due(stored: Stored, now_ms: f64) -> bool {
    stored.auto_check && stored.last_check_ms.is_none_or(|t| now_ms - t >= DAY_MS || now_ms < t)
}

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// The words the UI can say about an error.
fn failure(e: &tauri_plugin_updater::Error) -> UpdateFailure {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Reqwest(r) if r.is_connect() || r.is_timeout() || r.is_request() => UpdateFailure::Offline,
        E::Network(_) => UpdateFailure::Offline,
        E::ReleaseNotFound => UpdateFailure::NoRelease,
        E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) => UpdateFailure::Signature,
        _ => UpdateFailure::Other,
    }
}

/// `latest.json` of the release with the highest version that is not a
/// draft, pre-releases included, from GitHub's list of releases (by version,
/// not by the list's order, which follows when each was created).
fn newest_manifest(releases_json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        draft: bool,
        assets: Vec<Asset>,
    }
    let releases: Vec<Release> = serde_json::from_str(releases_json).ok()?;
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = semver::Version::parse(r.tag_name.trim_start_matches('v')).ok()?;
            let url = r.assets.into_iter().find(|a| a.name == "latest.json")?.browser_download_url;
            Some((version, url))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, url)| url)
}

/// Updates (managed Tauri state).
pub struct Updates {
    app: AppHandle,
    dir: Option<PathBuf>,
    stored: Mutex<Stored>,
    status: Mutex<UpdateStatus>,
    /// The update found by the last look, ready to install.
    found: Mutex<Option<Update>>,
    /// The download running now (it can be cancelled).
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl Updates {
    /// Load the settings and start the daily look.
    pub fn spawn(app: AppHandle, dir: Option<PathBuf>) -> Arc<Self> {
        let stored = dir.as_deref().and_then(|d| settings::load::<Stored>(d, FILE, VERSION)).unwrap_or_default();
        let me = Arc::new(Self {
            app,
            dir,
            stored: Mutex::new(stored),
            status: Mutex::new(UpdateStatus::Idle),
            found: Mutex::new(None),
            task: Mutex::new(None),
        });
        let worker = me.clone();
        let spawned = std::thread::Builder::new().name("updates".into()).spawn(move || {
            std::thread::sleep(FIRST_LOOK_AFTER);
            loop {
                if due(worker.stored(), now_ms()) {
                    tauri::async_runtime::block_on(worker.look());
                }
                std::thread::sleep(LOOK_EVERY);
            }
        });
        if let Err(e) = spawned {
            tracing::warn!("no daily update look: {e}");
        }
        me
    }

    fn stored(&self) -> Stored {
        self.stored.lock().map(|s| *s).unwrap_or_default()
    }

    fn save(&self, next: Stored) {
        if let Some(dir) = &self.dir {
            if let Err(e) = settings::save(dir, FILE, VERSION, next) {
                tracing::warn!("update settings not saved: {e}");
            }
        }
        if let Ok(mut s) = self.stored.lock() {
            *s = next;
        }
    }

    pub fn state(&self) -> UpdateState {
        let stored = self.stored();
        UpdateState {
            current: self.app.package_info().version.to_string(),
            auto_check: stored.auto_check,
            last_check_ms: stored.last_check_ms,
            status: self.status.lock().map(|s| s.clone()).unwrap_or(UpdateStatus::Idle),
        }
    }

    fn set(&self, status: UpdateStatus) {
        if let Ok(mut s) = self.status.lock() {
            *s = status;
        }
        if let Err(e) = self.app.emit(EVENT_UPDATE, self.state()) {
            tracing::warn!("update state not sent: {e}");
        }
    }

    fn busy(&self) -> bool {
        self.status
            .lock()
            .map(|s| {
                matches!(
                    *s,
                    UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. }
                )
            })
            .unwrap_or(false)
    }

    pub fn set_auto(&self, on: bool) -> UpdateState {
        let mut next = self.stored();
        if next.auto_check != on {
            next.auto_check = on;
            self.save(next);
            tracing::info!(auto_check = on, "update setting");
        }
        self.state()
    }

    /// Look now (the "check for updates" button).
    pub async fn check(&self) -> UpdateState {
        self.look().await;
        self.state()
    }

    /// The updater: GitHub's latest release, or for a test version the
    /// newest release pre-releases included.
    async fn updater(&self) -> tauri_plugin_updater::Result<Updater> {
        let mut builder = self.app.updater_builder().timeout(REQUEST_TIMEOUT).on_before_exit(|| {
            tracing::info!("the update's installer starts; the app closes");
        });
        let current = self.app.package_info().version.clone();
        if !current.pre.is_empty() {
            let list = tauri::async_runtime::spawn_blocking(|| crate::calendar::http_get(RELEASES_API)).await;
            match list.ok().map(|r| r.map(|text| newest_manifest(&text))) {
                Some(Ok(Some(url))) => match url.parse() {
                    Ok(url) => builder = builder.endpoints(vec![url])?,
                    Err(e) => tracing::warn!("pre-release manifest address not valid: {e}"),
                },
                Some(Ok(None)) => tracing::info!("no release with an update manifest on GitHub"),
                Some(Err(e)) => tracing::warn!("GitHub's release list not read: {e}"),
                None => tracing::warn!("GitHub's release list not read"),
            }
        }
        builder.build()
    }

    async fn look(&self) {
        if self.busy() {
            return;
        }
        self.set(UpdateStatus::Checking);
        let result = match self.updater().await {
            Ok(updater) => updater.check().await,
            Err(e) => Err(e),
        };
        let mut stored = self.stored();
        stored.last_check_ms = Some(now_ms());
        self.save(stored);
        match result {
            Ok(Some(update)) => {
                tracing::info!(version = %update.version, "update available");
                let status = UpdateStatus::Available {
                    version: update.version.clone(),
                    notes: update.body.clone(),
                    date_ms: update.date.map(|d| d.unix_timestamp() as f64 * 1000.0),
                };
                if let Ok(mut f) = self.found.lock() {
                    *f = Some(update);
                }
                self.set(status);
            }
            Ok(None) => {
                tracing::info!("no newer version");
                if let Ok(mut f) = self.found.lock() {
                    *f = None;
                }
                self.set(UpdateStatus::UpToDate);
            }
            Err(e) => {
                tracing::warn!("update look failed: {e}");
                self.set(UpdateStatus::Failed { reason: failure(&e) });
            }
        }
    }

    /// Download the update found and install it (the app closes).
    pub fn install(self: &Arc<Self>) -> Result<UpdateState, String> {
        if self.busy() {
            return Ok(self.state());
        }
        let update = self.found.lock().ok().and_then(|f| f.clone()).ok_or("no update to install")?;
        let me = self.clone();
        let task = tauri::async_runtime::spawn(async move { me.download_and_install(update).await });
        if let Ok(mut t) = self.task.lock() {
            *t = Some(task);
        }
        Ok(self.state())
    }

    async fn download_and_install(&self, update: Update) {
        let version = update.version.clone();
        tracing::info!(%version, "update download starting");
        self.set(UpdateStatus::Downloading { version: version.clone(), done: 0.0, total: None });
        let mut done = 0usize;
        let mut sent = Instant::now();
        let bytes = update
            .download(
                |chunk, total| {
                    done += chunk;
                    if sent.elapsed() >= PROGRESS_EVERY {
                        sent = Instant::now();
                        self.set(UpdateStatus::Downloading {
                            version: version.clone(),
                            done: done as f64,
                            total: total.map(|t| t as f64),
                        });
                    }
                },
                || {},
            )
            .await;
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(%version, "update download failed: {e}");
                self.set(UpdateStatus::Failed { reason: failure(&e) });
                return;
            }
        };
        // Never cut a write to the keyboard short (the installer closes the app).
        let app = self.app.clone();
        let free = tauri::async_runtime::spawn_blocking(move || {
            let since = Instant::now();
            while crate::flash_busy(&app) {
                if since.elapsed() >= crate::device::SCREEN_UPLOAD_TIMEOUT {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            true
        })
        .await
        .unwrap_or(false);
        if !free {
            tracing::warn!(%version, "a write to the keyboard did not finish; update not installed");
            self.set(UpdateStatus::Failed { reason: UpdateFailure::KeyboardBusy });
            return;
        }
        tracing::info!(%version, bytes = bytes.len(), "update downloaded; installing");
        self.set(UpdateStatus::Installing { version: version.clone() });
        if let Err(e) = update.install(bytes) {
            tracing::warn!(%version, "update not installed: {e}");
            self.set(UpdateStatus::Failed { reason: failure(&e) });
        }
    }

    /// Stop the download; the update stays available.
    pub fn cancel(&self) -> UpdateState {
        let downloading = self.status.lock().map(|s| matches!(*s, UpdateStatus::Downloading { .. })).unwrap_or(false);
        if downloading {
            if let Some(task) = self.task.lock().ok().and_then(|mut t| t.take()) {
                task.abort();
            }
            tracing::info!("update download cancelled");
            let found = self.found.lock().ok().and_then(|f| f.clone());
            self.set(match found {
                Some(u) => UpdateStatus::Available {
                    version: u.version.clone(),
                    notes: u.body.clone(),
                    date_ms: u.date.map(|d| d.unix_timestamp() as f64 * 1000.0),
                },
                None => UpdateStatus::Idle,
            });
        }
        self.state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_once_a_day_when_allowed() {
        let day = DAY_MS;
        let never = Stored { auto_check: true, last_check_ms: None };
        assert!(due(never, 0.0));
        let today = Stored { auto_check: true, last_check_ms: Some(10.0 * day) };
        assert!(!due(today, 10.5 * day));
        assert!(due(today, 11.0 * day));
        // The clock went back: look again rather than wait for days.
        assert!(due(today, 9.0 * day));
        let off = Stored { auto_check: false, last_check_ms: None };
        assert!(!due(off, 0.0));
    }

    #[test]
    fn old_or_empty_settings_look_by_default() {
        assert_eq!(serde_json::from_str::<Stored>("{}").unwrap(), Stored::default());
        assert!(Stored::default().auto_check);
    }

    #[test]
    fn a_test_version_takes_the_highest_published_manifest() {
        let list = r#"[
            {"tag_name": "v0.2.0", "draft": true, "assets": [{"name": "latest.json", "browser_download_url": "https://x/draft/latest.json"}]},
            {"tag_name": "v0.1.0-rc.1", "draft": false, "assets": [
                {"name": "Keymotion_0.1.0-rc.1_x64-setup.exe", "browser_download_url": "https://x/rc1/setup.exe"},
                {"name": "latest.json", "browser_download_url": "https://x/rc1/latest.json"}
            ]},
            {"tag_name": "v0.1.0-rc.3", "draft": false, "assets": [{"name": "Keymotion_0.1.0-rc.3_x64-setup.exe", "browser_download_url": "https://x/rc3/setup.exe"}]},
            {"tag_name": "v0.1.0-rc.2", "draft": false, "assets": [{"name": "latest.json", "browser_download_url": "https://x/rc2/latest.json"}]},
            {"tag_name": "nightly", "draft": false, "assets": [{"name": "latest.json", "browser_download_url": "https://x/nightly/latest.json"}]}
        ]"#;
        // Listed after rc.1 but higher; rc.3 has no manifest; drafts and tags
        // that are not versions do not count.
        assert_eq!(newest_manifest(list).as_deref(), Some("https://x/rc2/latest.json"));
        assert_eq!(newest_manifest("[]"), None);
        assert_eq!(newest_manifest("not json"), None);
    }
}
