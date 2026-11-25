//! Updates (roadmap stage 6). The release pipeline puts `latest.json` on
//! GitHub's release page; the updater plugin reads it, checks the download
//! against the public key in `tauri.conf.json` and runs the installer.
//!
//! What the app does by itself is the user's choice (Settings › About;
//! owner's decision after the preview of 2026-11-16):
//!
//! * **Notify** (the default): it looks once a day (the first look a little
//!   after it starts, then whether a day has passed, as it may run in the
//!   tray for days). A new version shows as a Windows notification with
//!   "Update now" and "Later" (once per version), the first line of the
//!   tray menu with a dot on the icon, a card at the bottom of the sidebar
//!   and the strip above the page. The user installs it.
//! * **Auto**: it looks the same way, downloads the new version in the
//!   background and installs it once the computer has not been used for a
//!   while. It never does so during a write to the keyboard, while the
//!   focus timer runs or with changes not yet on the keyboard.
//! * **Off**: it never looks by itself; the button still does.
//!
//! Installing waits for a write to the keyboard to finish; on Windows the
//! app then closes, the installer runs with a small progress window and
//! opens the new version. Before it closes, the app notes whether its
//! window was open, and the new version opens the same way. Once a new
//! version runs, a notification says so, with "What's new".
//!
//! Buttons on notifications open the app's own `keymotion:` addresses
//! (kb_winsys::protocol): Windows starts the app with the address, and the
//! running app gets it through its single-instance lock (lib.rs). "Update
//! now" carries a token made at start, so only this run's notification can
//! start an install; `keymotion://background` (keymotion-cli starting the
//! app for an agent) opens nothing; any other address only opens the
//! window.
//!
//! A test version ("0.1.0-rc.1") also looks at GitHub's pre-releases, so
//! an update from one test version to the next can be tried for real; a
//! normal version only ever sees normal releases.
//!
//! The Microsoft Store build never looks: the Store updates it (its rules
//! allow no other way), and the settings say so (channel.rs). Windows closes
//! the running app to install a Store update; the app asks to be started
//! again afterwards (hidden, in the tray) and then says it was updated.

use crate::dto::{TrayTexts, UpdateFailure, UpdateMode, UpdateState, UpdateStatus};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

pub const EVENT_UPDATE: &str = "update-state";
/// The UI opens the update dialog (a notification or the tray menu asked).
pub const EVENT_UPDATE_OPEN: &str = "update-open";

/// The app's own address scheme: `keymotion://…` (kb_winsys::protocol).
pub const SCHEME: &str = "keymotion";
const ADDRESS_OPEN: &str = "keymotion://open";
const ADDRESS_UPDATE: &str = "keymotion://update";

const FILE: &str = "updates.json";
const VERSION: u32 = 1;
const DAY_MS: f64 = 24.0 * 3_600_000.0;
/// The first look waits until the app has settled after starting.
const FIRST_LOOK_AFTER: Duration = Duration::from_secs(30);
/// While the app runs, whether to look or to install is checked this often.
const TICK: Duration = Duration::from_secs(60);
/// "Update by itself" installs after this long without mouse or keyboard
/// input (a locked screen counts as not in use).
const IDLE_BEFORE_INSTALL_MS: u64 = 10 * 60_000;
/// A background download that failed is tried again after this long.
const PREFETCH_RETRY: Duration = Duration::from_secs(3600);
/// The note left before installing is only believed this long: the new
/// version starts within seconds.
const RESTART_NOTE_MS: f64 = 10.0 * 60_000.0;
/// The "updated" notification waits this long at most for the UI's words.
const TEXTS_WAIT: Duration = Duration::from_secs(90);
/// One request to GitHub, at most.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Download progress goes to the UI at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);
/// GitHub's list of releases, newest first (test versions only).
const RELEASES_API: &str = "https://api.github.com/repos/cebersaban44-rgb/keymotion/releases?per_page=10";
/// The release notes on the documentation site (in the UI language's folder).
const NOTES_PAGE: &str = "about/release-notes/";
/// Notifications about updates replace each other.
const NOTICE_TAG: &str = "update";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    /// `None` in files from before the choice: `auto_check` decides.
    mode: Option<UpdateMode>,
    /// The older on/off switch. Still written, so an older version reads
    /// the same choice.
    auto_check: bool,
    last_check_ms: Option<f64>,
    /// The version the "ready" notification was shown for: once per version.
    notified: Option<String>,
    /// The version that ran last: a newer one says it was installed.
    last_run: Option<String>,
    /// Left just before installing, for the new version.
    restart: Option<RestartNote>,
}

impl Default for Stored {
    fn default() -> Self {
        Self { mode: None, auto_check: true, last_check_ms: None, notified: None, last_run: None, restart: None }
    }
}

impl Stored {
    fn mode(&self) -> UpdateMode {
        self.mode.unwrap_or(if self.auto_check { UpdateMode::Notify } else { UpdateMode::Off })
    }
}

/// How the app was when it closed to install a new version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestartNote {
    version: String,
    /// The window was not on screen: the new version starts in the tray.
    hidden: bool,
    /// Installed by "update by itself", not by the user's click.
    by_itself: bool,
    at_ms: f64,
}

/// The note, if it is about `version` and was left a moment ago.
fn fresh_note(note: Option<RestartNote>, version: &str, now_ms: f64) -> Option<RestartNote> {
    note.filter(|n| n.version == version && n.at_ms <= now_ms && now_ms - n.at_ms < RESTART_NOTE_MS)
}

/// `current` is a newer version than `last`.
fn newer(current: &str, last: &str) -> bool {
    match (semver::Version::parse(current), semver::Version::parse(last)) {
        (Ok(c), Ok(l)) => c > l,
        _ => false,
    }
}

/// A day has passed since the last look (or the clock went back), and the
/// user lets the app look by itself.
fn due(stored: &Stored, now_ms: f64) -> bool {
    stored.mode() != UpdateMode::Off && stored.last_check_ms.is_none_or(|t| now_ms - t >= DAY_MS || now_ms < t)
}

/// "Update by itself" may install now: the computer has not been used for
/// a while and the app is not in the middle of something.
fn quiet_enough(idle_ms: Option<u64>, app_in_use: bool) -> bool {
    idle_ms.is_some_and(|ms| ms >= IDLE_BEFORE_INSTALL_MS) && !app_in_use
}

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

fn fill(template: &str, version: &str) -> String {
    template.replace("{version}", version)
}

/// The UI's words, once it has sent them; English if it has not within
/// `TEXTS_WAIT`.
fn ui_texts(app: &AppHandle) -> TrayTexts {
    let since = Instant::now();
    loop {
        match app.try_state::<Arc<crate::tray::Background>>() {
            Some(bg) if bg.texts_received() || since.elapsed() >= TEXTS_WAIT => return bg.texts(),
            None if since.elapsed() >= TEXTS_WAIT => return TrayTexts::default(),
            _ => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// What a `keymotion:` address asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// Open the window.
    Open,
    /// Open the window on the update dialog.
    Update,
    /// Install the update found now ("Update now" on the notification). The
    /// token must be this run's.
    Install { token: String },
    /// Run without opening the window (`keymotion://background`, what
    /// keymotion-cli opens through Explorer to start the app for an agent).
    /// A running app does nothing.
    Background,
}

/// The address among a program's arguments, if any.
pub fn link<S: AsRef<str>>(args: &[S]) -> Option<Link> {
    args.iter().find_map(|a| parse_link(a.as_ref()))
}

fn parse_link(arg: &str) -> Option<Link> {
    let (scheme, rest) = arg.split_once(':')?;
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let rest = rest.trim_start_matches('/');
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    Some(match path.trim_end_matches('/').to_ascii_lowercase().as_str() {
        "update" => Link::Update,
        "background" => Link::Background,
        "update/install" => {
            Link::Install { token: query.split('&').find_map(|p| p.strip_prefix("t=")).unwrap_or_default().to_string() }
        }
        // Anything else only opens the window.
        _ => Link::Open,
    })
}

/// The words of the error the UI can say.
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

/// The status for an update found: ready once it is downloaded.
fn found_status(update: &Update, ready: bool) -> UpdateStatus {
    let (version, notes) = (update.version.clone(), update.body.clone());
    let date_ms = update.date.map(|d| d.unix_timestamp() as f64 * 1000.0);
    if ready {
        UpdateStatus::Ready { version, notes, date_ms }
    } else {
        UpdateStatus::Available { version, notes, date_ms }
    }
}

/// Updates (managed Tauri state).
pub struct Updates {
    app: AppHandle,
    dir: Option<PathBuf>,
    stored: Mutex<Stored>,
    status: Mutex<UpdateStatus>,
    /// The update found by the last look, ready to install.
    found: Mutex<Option<Update>>,
    /// Its installer, downloaded in the background ("update by itself").
    ready: Mutex<Option<(String, Vec<u8>)>>,
    /// The download the user started (it can be cancelled).
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// The background download, and when it last started.
    prefetch: Mutex<Option<(tauri::async_runtime::JoinHandle<()>, Instant)>>,
    /// Only this run's "Update now" notification carries it.
    token: String,
    /// From the last run's note: start with the window hidden (`Some(true)`)
    /// or shown, whatever the command line says.
    start_hidden: Option<bool>,
}

impl Updates {
    /// Load the settings, tell about an update just installed and start the
    /// daily look.
    pub fn spawn(app: AppHandle, dir: Option<PathBuf>) -> Arc<Self> {
        let mut stored = dir.as_deref().and_then(|d| settings::load::<Stored>(d, FILE, VERSION)).unwrap_or_default();
        let current = app.package_info().version.to_string();
        let note = fresh_note(stored.restart.take(), &current, now_ms());
        let updated = stored.last_run.as_deref().is_some_and(|last| newer(&current, last));
        let changed = stored.last_run.as_deref() != Some(current.as_str()) || note.is_some();
        stored.last_run = Some(current.clone());
        let token = format!("{:016x}", std::collections::hash_map::RandomState::new().build_hasher().finish());
        let me = Arc::new(Self {
            app,
            dir,
            stored: Mutex::new(stored.clone()),
            status: Mutex::new(UpdateStatus::Idle),
            found: Mutex::new(None),
            ready: Mutex::new(None),
            task: Mutex::new(None),
            prefetch: Mutex::new(None),
            token,
            start_hidden: note.as_ref().map(|n| n.hidden),
        });
        if changed {
            me.save(stored);
        }
        if updated {
            tracing::info!(version = %current, by_itself = note.as_ref().is_some_and(|n| n.by_itself), "updated");
            me.tell_installed(&current, note.is_some_and(|n| n.by_itself));
        }
        // The keyboard simulator is a development run beside the installed
        // copy: it neither takes over the `keymotion:` link (the installed
        // copy's, or a later start of this debug program would drive a real
        // keyboard) nor looks for, downloads or installs an update.
        if crate::simulated() {
            tracing::info!("keyboard simulator: no update look, the keymotion: link left alone");
            return me;
        }
        if crate::channel::store() {
            tracing::info!("updates come from the Microsoft Store");
            // Windows closes the app to install a Store update: come back
            // afterwards, in the tray.
            if let Err(e) = kb_winsys::package::restart_after_update(crate::tray::ARG_HIDDEN) {
                tracing::warn!("{e}");
            }
            return me;
        }
        match std::env::current_exe() {
            Ok(exe) => {
                if let Err(e) = kb_winsys::protocol::register(SCHEME, "Keymotion", &exe) {
                    tracing::warn!("{e}");
                }
            }
            Err(e) => tracing::warn!("own program not found; notification buttons do nothing: {e}"),
        }
        let worker = me.clone();
        let spawned = std::thread::Builder::new().name("updates".into()).spawn(move || {
            std::thread::sleep(FIRST_LOOK_AFTER);
            loop {
                if due(&worker.stored(), now_ms()) {
                    tauri::async_runtime::block_on(worker.look(false));
                }
                worker.keep_prefetching();
                worker.install_when_quiet();
                std::thread::sleep(TICK);
            }
        });
        if let Err(e) = spawned {
            tracing::warn!("no daily update look: {e}");
        }
        me
    }

    /// The window as the last run left it before an update: `Some(true)`,
    /// keep it hidden; `Some(false)`, show it; `None`, as usual.
    pub fn start_hidden(&self) -> Option<bool> {
        self.start_hidden
    }

    fn stored(&self) -> Stored {
        self.stored.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn save(&self, next: Stored) {
        if let Some(dir) = &self.dir {
            if let Err(e) = settings::save(dir, FILE, VERSION, &next) {
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
            mode: stored.mode(),
            last_check_ms: stored.last_check_ms,
            status: self.status(),
            store: crate::channel::store(),
            github_copy: crate::handoff::github_copy_beside(),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or(UpdateStatus::Idle)
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
        matches!(
            self.status(),
            UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. }
        )
    }

    fn found(&self) -> Option<Update> {
        self.found.lock().ok().and_then(|f| f.clone())
    }

    pub fn set_mode(self: &Arc<Self>, mode: UpdateMode) -> UpdateState {
        if crate::channel::store() {
            return self.state();
        }
        let mut next = self.stored();
        if next.mode() != mode || next.mode.is_none() {
            next.mode = Some(mode);
            next.auto_check = mode != UpdateMode::Off;
            self.save(next);
            tracing::info!(?mode, "update setting");
        }
        // A version already found starts downloading now.
        if mode == UpdateMode::Auto && matches!(self.status(), UpdateStatus::Available { .. }) {
            if let Some(update) = self.found() {
                self.start_prefetch(update);
            }
        }
        self.state()
    }

    /// Look now (the "check for updates" button).
    pub async fn check(self: &Arc<Self>) -> UpdateState {
        self.look(true).await;
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

    /// Ask GitHub. `by_hand`: the user pressed the button, so the window is
    /// in front and needs no notification.
    async fn look(self: &Arc<Self>, by_hand: bool) {
        if crate::simulated() {
            return;
        }
        // A downloaded update waits to be installed: nothing to ask.
        if self.busy() || crate::channel::store() || matches!(self.status(), UpdateStatus::Ready { .. }) {
            return;
        }
        self.set(UpdateStatus::Checking);
        let result = match self.updater().await {
            Ok(updater) => updater.check().await,
            Err(e) => Err(e),
        };
        let mut stored = self.stored();
        stored.last_check_ms = Some(now_ms());
        self.save(stored.clone());
        match result {
            Ok(Some(update)) => {
                let version = update.version.clone();
                tracing::info!(%version, "update available");
                let status = found_status(&update, false);
                if let Ok(mut f) = self.found.lock() {
                    *f = Some(update.clone());
                }
                self.set(status);
                match stored.mode() {
                    UpdateMode::Auto => self.start_prefetch(update),
                    UpdateMode::Notify if !by_hand => self.tell_ready(&version),
                    _ => {}
                }
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
                // A daily look that fails keeps what an earlier one found;
                // the button says what went wrong.
                self.set(match self.found().filter(|_| !by_hand) {
                    Some(u) => found_status(&u, false),
                    None => UpdateStatus::Failed { reason: failure(&e) },
                });
            }
        }
    }

    // ------------------------------------------------------- notifications

    /// Show a notification in the UI's words. They may take a moment (a
    /// start with Windows loads the page in the hidden window), so not on
    /// the caller's thread.
    fn notify(&self, what: &'static str, notice: impl FnOnce(&TrayTexts, &str) -> Result<(), String> + Send + 'static) {
        let app = self.app.clone();
        std::thread::spawn(move || {
            let texts = ui_texts(&app);
            let app_id = crate::notify::app_id(&app.config().identifier);
            if let Err(e) = notice(&texts, &app_id) {
                tracing::warn!("{what} notification not shown: {e}");
            }
        });
    }

    /// A new version is ready: say so once, unless the window is in front
    /// (the strip says it there).
    fn tell_ready(&self, version: &str) {
        let mut stored = self.stored();
        if stored.notified.as_deref() == Some(version) {
            return;
        }
        stored.notified = Some(version.to_string());
        self.save(stored);
        let in_front = self
            .app
            .get_webview_window("main")
            .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
        if in_front {
            return;
        }
        let version = version.to_string();
        let install = format!("{ADDRESS_UPDATE}/install?t={}", self.token);
        tracing::info!(%version, "telling about the update");
        self.notify("update", move |t, app_id| {
            let title = fill(&t.notice_ready_title, &version);
            crate::notify::show_notice(
                app_id,
                &crate::notify::Notice {
                    title: &title,
                    body: &t.notice_ready_body,
                    open: Some(ADDRESS_UPDATE),
                    buttons: vec![(&t.notice_install, Some(&install)), (&t.notice_later, None)],
                    tag: Some(NOTICE_TAG),
                },
            )
        });
    }

    /// The new version runs: say so, with the notes on the site.
    fn tell_installed(&self, version: &str, by_itself: bool) {
        let version = version.to_string();
        let store = crate::channel::store();
        self.notify("updated", move |t, app_id| {
            let title = fill(&t.notice_installed_title, &version);
            let notes = crate::i18n::docs_page(&t.locale, NOTES_PAGE);
            crate::notify::show_notice(
                app_id,
                &crate::notify::Notice {
                    title: &title,
                    body: if by_itself { &t.notice_installed_auto } else { "" },
                    // The Store build has no `keymotion:` address: Windows
                    // starts it by itself when the notification is clicked.
                    open: (!store).then_some(ADDRESS_OPEN),
                    buttons: vec![(&t.notice_whats_new, Some(&notes))],
                    tag: Some(NOTICE_TAG),
                },
            )
        });
    }

    // ------------------------------------------------------------- install

    /// "Update by itself": download the update found in the background.
    fn start_prefetch(self: &Arc<Self>, update: Update) {
        let Ok(mut slot) = self.prefetch.lock() else { return };
        if slot.as_ref().is_some_and(|(t, _)| !t.inner().is_finished()) {
            return;
        }
        let me = self.clone();
        let task = tauri::async_runtime::spawn(async move {
            let version = update.version.clone();
            tracing::info!(%version, "downloading the update in the background");
            let bytes = match update.download(|_, _| {}, || {}).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(%version, "background download failed: {e}");
                    return;
                }
            };
            if let Err(e) = update.verify(&bytes) {
                tracing::warn!(%version, "background download does not match its signature: {e}");
                return;
            }
            // Still the update found, and nobody started installing meanwhile.
            let same = me.found().is_some_and(|f| f.version == version);
            if !same || !matches!(me.status(), UpdateStatus::Available { .. }) {
                return;
            }
            tracing::info!(%version, bytes = bytes.len(), "update downloaded; it installs when the computer is not in use");
            if let Ok(mut r) = me.ready.lock() {
                *r = Some((version, bytes));
            }
            me.set(found_status(&update, true));
        });
        *slot = Some((task, Instant::now()));
    }

    /// A background download that failed is tried again now and then.
    fn keep_prefetching(self: &Arc<Self>) {
        if self.stored().mode() != UpdateMode::Auto || !matches!(self.status(), UpdateStatus::Available { .. }) {
            return;
        }
        let waiting = self
            .prefetch
            .lock()
            .map(|p| p.as_ref().is_some_and(|(t, at)| !t.inner().is_finished() || at.elapsed() < PREFETCH_RETRY))
            .unwrap_or(true);
        if !waiting {
            if let Some(update) = self.found() {
                self.start_prefetch(update);
            }
        }
    }

    /// "Update by itself": install the downloaded update once the computer
    /// has not been used for a while and the app is free.
    fn install_when_quiet(self: &Arc<Self>) {
        if self.stored().mode() != UpdateMode::Auto || !matches!(self.status(), UpdateStatus::Ready { .. }) {
            return;
        }
        if !quiet_enough(kb_winsys::idle_ms(), crate::busy_for_update(&self.app)) {
            return;
        }
        let (Some(update), Some((version, bytes))) = (self.found(), self.ready.lock().ok().and_then(|mut r| r.take()))
        else {
            return;
        };
        if update.version != version {
            return;
        }
        tracing::info!(%version, "the computer is not in use; installing the update");
        tauri::async_runtime::block_on(self.install_bytes(update, bytes, true));
    }

    /// Install the update found (the app closes): at once when it is
    /// downloaded already, otherwise after downloading it.
    pub fn install(self: &Arc<Self>) -> Result<UpdateState, String> {
        if self.busy() || crate::channel::store() {
            return Ok(self.state());
        }
        let update = self.found().ok_or("no update to install")?;
        let ready = self.ready.lock().ok().and_then(|mut r| r.take()).filter(|(v, _)| *v == update.version);
        if let Some((task, _)) = self.prefetch.lock().ok().and_then(|mut p| p.take()) {
            task.abort();
        }
        let me = self.clone();
        let task = tauri::async_runtime::spawn(async move {
            match ready {
                Some((_, bytes)) => me.install_bytes(update, bytes, false).await,
                None => me.download_and_install(update).await,
            }
        });
        if let Ok(mut t) = self.task.lock() {
            *t = Some(task);
        }
        Ok(self.state())
    }

    /// "Update now" on the notification: install when the token is this
    /// run's and an update is found; otherwise open the update dialog. A
    /// failure shows in the window.
    pub fn install_from_notice(self: &Arc<Self>, token: &str) {
        let found = matches!(self.status(), UpdateStatus::Available { .. } | UpdateStatus::Ready { .. });
        if token != self.token || !found {
            tracing::info!("update address without this run's token; opening the window");
            self.open_dialog();
            return;
        }
        tracing::info!("update chosen on the notification");
        if let Err(e) = self.install() {
            tracing::warn!("update not started: {e}");
            self.open_dialog();
            return;
        }
        // The download runs without a window; a failure needs one.
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            match me.status() {
                UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. } => {}
                UpdateStatus::Failed { .. } => {
                    me.open_dialog();
                    break;
                }
                _ => break,
            }
        });
    }

    /// The window in front, on the update dialog.
    pub fn open_dialog(&self) {
        crate::tray::show_main(&self.app);
        let _ = self.app.emit(EVENT_UPDATE_OPEN, ());
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
        match bytes {
            Ok(bytes) => self.install_bytes(update, bytes, false).await,
            Err(e) => {
                tracing::warn!(%version, "update download failed: {e}");
                self.set(UpdateStatus::Failed { reason: failure(&e) });
            }
        }
    }

    /// Run the installer: the app closes, and the new version opens the
    /// way this one is now (window shown or not).
    async fn install_bytes(&self, update: Update, bytes: Vec<u8>, by_itself: bool) {
        let version = update.version.clone();
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
        let hidden = !self.app.get_webview_window("main").is_some_and(|w| w.is_visible().unwrap_or(false));
        let mut stored = self.stored();
        stored.restart = Some(RestartNote { version: version.clone(), hidden, by_itself, at_ms: now_ms() });
        self.save(stored);
        tracing::info!(%version, bytes = bytes.len(), hidden, by_itself, "installing the update");
        self.set(UpdateStatus::Installing { version: version.clone() });
        if let Err(e) = update.install(bytes) {
            tracing::warn!(%version, "update not installed: {e}");
            let mut stored = self.stored();
            stored.restart = None;
            self.save(stored);
            self.set(UpdateStatus::Failed { reason: failure(&e) });
        }
    }

    /// Stop the download; the update stays available.
    pub fn cancel(&self) -> UpdateState {
        if matches!(self.status(), UpdateStatus::Downloading { .. }) {
            if let Some(task) = self.task.lock().ok().and_then(|mut t| t.take()) {
                task.abort();
            }
            tracing::info!("update download cancelled");
            self.set(match self.found() {
                Some(u) => found_status(&u, false),
                None => UpdateStatus::Idle,
            });
        }
        self.state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(mode: Option<UpdateMode>, auto_check: bool, last: Option<f64>) -> Stored {
        Stored { mode, auto_check, last_check_ms: last, ..Stored::default() }
    }

    #[test]
    fn looks_once_a_day_unless_off() {
        let day = DAY_MS;
        assert!(due(&with(None, true, None), 0.0));
        let today = with(Some(UpdateMode::Notify), true, Some(10.0 * day));
        assert!(!due(&today, 10.5 * day));
        assert!(due(&today, 11.0 * day));
        // The clock went back: look again rather than wait for days.
        assert!(due(&today, 9.0 * day));
        assert!(due(&with(Some(UpdateMode::Auto), true, None), 0.0));
        assert!(!due(&with(Some(UpdateMode::Off), false, None), 0.0));
    }

    #[test]
    fn the_older_switch_becomes_the_choice() {
        assert_eq!(serde_json::from_str::<Stored>("{}").unwrap(), Stored::default());
        assert_eq!(Stored::default().mode(), UpdateMode::Notify);
        let older: Stored = serde_json::from_str(r#"{"autoCheck": false, "lastCheckMs": 5}"#).unwrap();
        assert_eq!(older.mode(), UpdateMode::Off);
        let older: Stored = serde_json::from_str(r#"{"autoCheck": true}"#).unwrap();
        assert_eq!(older.mode(), UpdateMode::Notify);
        // The choice wins over the switch once there is one.
        assert_eq!(with(Some(UpdateMode::Auto), true, None).mode(), UpdateMode::Auto);
        let saved = serde_json::to_string(&with(Some(UpdateMode::Off), false, None)).unwrap();
        assert!(saved.contains(r#""mode":"off""#) && saved.contains(r#""autoCheck":false"#), "{saved}");
    }

    #[test]
    fn only_a_fresh_note_about_this_version_counts() {
        let note = |version: &str, at_ms: f64| {
            Some(RestartNote { version: version.into(), hidden: true, by_itself: true, at_ms })
        };
        let now = 100.0 * 60_000.0;
        assert!(fresh_note(note("1.1.0", now - 30_000.0), "1.1.0", now).is_some());
        // The install failed and an old note lingers, or another version runs.
        assert!(fresh_note(note("1.1.0", now - 11.0 * 60_000.0), "1.1.0", now).is_none());
        assert!(fresh_note(note("1.1.0", now - 30_000.0), "1.0.0", now).is_none());
        assert!(fresh_note(note("1.1.0", now + 60_000.0), "1.1.0", now).is_none());
        assert!(fresh_note(None, "1.1.0", now).is_none());
    }

    #[test]
    fn a_newer_version_than_the_last_run_says_it_was_installed() {
        assert!(newer("1.1.0", "1.0.0"));
        assert!(newer("1.0.0", "1.0.0-rc.2"));
        assert!(!newer("1.0.0", "1.0.0"));
        // Going back (a test version, an older copy) says nothing.
        assert!(!newer("1.0.0", "1.1.0"));
        assert!(!newer("1.0.0", "not a version"));
    }

    #[test]
    fn installs_by_itself_only_when_the_computer_is_quiet() {
        assert!(quiet_enough(Some(IDLE_BEFORE_INSTALL_MS), false));
        assert!(!quiet_enough(Some(IDLE_BEFORE_INSTALL_MS - 1), false));
        // A write to the keyboard, the focus timer or unsaved changes.
        assert!(!quiet_enough(Some(IDLE_BEFORE_INSTALL_MS * 6), true));
        assert!(!quiet_enough(None, false));
    }

    #[test]
    fn notification_addresses() {
        assert_eq!(link(&["--hidden"]), None);
        assert_eq!(link(&["C:\\x\\keymotion.exe", "keymotion://update"]), Some(Link::Update));
        assert_eq!(link(&["keymotion://update/"]), Some(Link::Update));
        assert_eq!(link(&["KEYMOTION://Update/Install?t=00ab"]), Some(Link::Install { token: "00ab".into() }));
        assert_eq!(link(&["keymotion://update/install"]), Some(Link::Install { token: String::new() }));
        assert_eq!(link(&["keymotion://open"]), Some(Link::Open));
        assert_eq!(link(&["C:\\x\\keymotion.exe", "keymotion://background"]), Some(Link::Background));
        assert_eq!(link(&["KEYMOTION://Background/"]), Some(Link::Background));
        assert_eq!(link(&["--background"]), None);
        // Anything else of ours only opens the window; other schemes are not ours.
        assert_eq!(link(&["keymotion:whatever/../x"]), Some(Link::Open));
        assert_eq!(link(&["https://example.com/keymotion://update"]), None);
        assert_eq!(link(&["C:\\Users\\a\\file.txt"]), None);
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
