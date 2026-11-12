//! The screen library: every animation the user prepared, stored exactly as
//! the keyboard gets it (validated blob), plus what each model's GIF page
//! shows and how many uploads were made.
//!
//! The keyboard cannot give its animation back, so this is the only copy.
//! Files live in `<config>/screen/`: `library.json` and `library/<id>.bin`.
//! The id is a hash of the data, so the same content is stored once.
//!
//! An upload that starts writing the GIF page is marked "incomplete" on
//! disk until it finishes, so a failure, a quit or a crash midway is known
//! afterwards and the page is not shown as holding the old picture.

use crate::dto::{LiveItem, LiveRestore, ScreenIncomplete, ScreenItem, ScreenItemKind, ScreenLimits, ScreenState};
use crate::live::LiveStored;
use kb_protocol::sonix::screen::{self, from_blob, ScreenAnimation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const INDEX_FILE: &str = "library.json";
const INDEX_VERSION: u32 = 1;
/// Factory animation backup taken from the official driver
/// (`official_driver.rs`).
const FACTORY_FILE: &str = "factory/ak980-pro-factory.gif";
/// Name stored for the factory backup. The UI shows its own, translated
/// label for the factory item (older libraries hold a Turkish name).
pub const FACTORY_NAME: &str = "AJAZZ factory animation";
/// The last card the live mode uploaded (not a library item).
const LIVE_FILE: &str = "live.bin";
/// What `shown` holds while the GIF page shows a live card.
pub const LIVE_ID: &str = "live";
/// Upload counts are kept per day for this long.
const KEEP_DAYS: usize = 60;
/// Longest stored name.
pub const NAME_MAX: usize = 60;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Index {
    items: Vec<ScreenItem>,
    /// Model id → id of the item its GIF page shows (as far as the app knows).
    shown: BTreeMap<String, String>,
    total_uploads: u32,
    /// Local date (YYYY-MM-DD) → uploads that day.
    uploads_by_day: BTreeMap<String, u32>,
    /// Live mode settings, budget and its last card.
    #[serde(default)]
    live: LiveStored,
    /// An upload that started writing the GIF page and has not finished
    /// (set before the first page, cleared when one finishes).
    #[serde(default)]
    incomplete: Option<IncompleteWrite>,
}

/// See [`Index::incomplete`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncompleteWrite {
    /// Model written to; empty when not known.
    model_id: String,
    /// Library item to offer for uploading again.
    id: Option<String>,
    at_ms: f64,
}

pub struct ScreenLibrary {
    dir: Option<PathBuf>,
    index: Index,
    /// `Some` while an upload writes the GIF page, holding the mark from
    /// before it (put back if the flash was not touched after all).
    writing: Option<Option<IncompleteWrite>>,
}

/// Decode the factory animation GIF (the official driver's copy) into the
/// keyboard's format; only a 240×135 GIF the keyboard can play passes.
pub fn factory_animation(gif: &[u8]) -> Result<ScreenAnimation, String> {
    kb_core::screen_import::decode_gif(gif)
        .and_then(|d| kb_core::screen_import::exact_size_animation(&d, None))
        .map_err(|e| e.to_string())
}

/// FNV-1a 64: a stable content id (std's hasher may change between Rust
/// versions; ids end up in file names).
pub(crate) fn content_id(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

pub(crate) fn clean_name(name: &str) -> String {
    let n: String = name.chars().filter(|c| !c.is_control()).take(NAME_MAX).collect();
    let n = n.trim();
    if n.is_empty() {
        "Adsız".to_string()
    } else {
        n.to_string()
    }
}

impl ScreenLibrary {
    /// Open the library in `config_dir/screen` (in memory only without a
    /// directory). Items whose data file is missing are dropped. The factory
    /// backup is added once if it exists.
    pub fn open(config_dir: Option<&Path>) -> Self {
        let dir = config_dir.map(|d| d.join("screen"));
        let mut index: Index =
            dir.as_deref().and_then(|d| crate::settings::load(d, INDEX_FILE, INDEX_VERSION)).unwrap_or_default();
        if let Some(d) = &dir {
            index.items.retain(|i| d.join("library").join(format!("{}.bin", i.id)).is_file());
        }
        let mut lib = Self { dir, index, writing: None };
        lib.import_factory();
        lib
    }

    pub fn has_factory(&self) -> bool {
        self.index.items.iter().any(|i| i.kind == ScreenItemKind::Factory)
    }

    /// No factory backup, and the app never wrote the GIF page: the keyboard
    /// probably still shows its factory animation, and the next upload
    /// erases it for good.
    pub fn factory_at_risk(&self) -> bool {
        !self.has_factory() && self.index.shown.is_empty() && self.index.incomplete.is_none()
    }

    fn import_factory(&mut self) {
        if self.has_factory() {
            return;
        }
        let Some(path) = self.dir.as_ref().map(|d| d.join(FACTORY_FILE)) else { return };
        let Ok(bytes) = fs::read(&path) else { return };
        match factory_animation(&bytes) {
            Ok(anim) => match self.add_factory(&anim) {
                Ok(item) => {
                    tracing::info!(id = %item.id, frames = item.frames, "factory animation added to the library")
                }
                Err(e) => tracing::warn!("factory animation not stored: {e}"),
            },
            Err(e) => tracing::warn!("factory backup unreadable: {e}"),
        }
    }

    /// Add the factory item. The same picture may be in the library already
    /// (the user imported the GIF by hand): it becomes the factory item.
    fn add_factory(&mut self, anim: &ScreenAnimation) -> io::Result<ScreenItem> {
        let item = self.add(FACTORY_NAME, ScreenItemKind::Factory, anim)?;
        if item.kind == ScreenItemKind::Factory {
            return Ok(item);
        }
        let Some(stored) = self.index.items.iter_mut().find(|i| i.id == item.id) else { return Ok(item) };
        stored.kind = ScreenItemKind::Factory;
        stored.name = FACTORY_NAME.to_string();
        let item = stored.clone();
        self.save_index()?;
        Ok(item)
    }

    /// Keep `gif`, the official driver's copy of the factory animation
    /// (`anim` is it decoded, [`factory_animation`]), where [`Self::open`]
    /// looks for the backup, and add it to the library as the factory item.
    /// A library that has the factory item already keeps it.
    pub fn store_factory(&mut self, gif: &[u8], anim: &ScreenAnimation) -> Result<ScreenItem, String> {
        if let Some(item) = self.index.items.iter().find(|i| i.kind == ScreenItemKind::Factory) {
            return Ok(item.clone());
        }
        let path = self.dir.as_ref().map(|d| d.join(FACTORY_FILE)).ok_or("library has no storage")?;
        let write = || -> io::Result<()> {
            fs::create_dir_all(path.parent().expect("factory dir"))?;
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, gif)?;
            fs::rename(&tmp, &path)
        };
        write().map_err(|e| format!("factory backup not saved: {e}"))?;
        let item = self.add_factory(anim).map_err(|e| format!("factory backup not saved: {e}"))?;
        tracing::info!(id = %item.id, frames = item.frames, "factory animation backed up from the official driver");
        Ok(item)
    }

    /// An upload is about to write the GIF page: until it ends the page
    /// counts as incomplete, on disk too (a crash or a quit leaves it so).
    /// `retry` is the library item to offer for uploading again.
    pub fn write_starting(&mut self, model_id: Option<&str>, retry: Option<String>) {
        let before = self.index.incomplete.take();
        self.writing = Some(before);
        self.index.incomplete =
            Some(IncompleteWrite { model_id: model_id.unwrap_or_default().to_string(), id: retry, at_ms: now_ms() });
        if let Err(e) = self.save_index() {
            tracing::warn!("upload mark not saved: {e}");
        }
    }

    /// How the upload ended. Finished: the page is whole. Failed before
    /// touching the flash: the mark from before comes back. Failed after:
    /// the mark stays, the page holds part of the upload.
    pub fn write_ended(&mut self, flash_written: bool, ok: bool) {
        let before = self.writing.take();
        if ok {
            self.index.incomplete = None;
        } else if !flash_written {
            if let Some(before) = before {
                self.index.incomplete = before;
            }
        } else if let Some(w) = &self.index.incomplete {
            tracing::warn!(id = ?w.id, "GIF page left incomplete");
        }
        if let Err(e) = self.save_index() {
            tracing::warn!("upload mark not saved: {e}");
        }
    }

    /// The cut-short upload for this model, if any (not while one runs).
    fn incomplete_for(&self, model_id: &str) -> Option<&IncompleteWrite> {
        if self.writing.is_some() {
            return None;
        }
        self.index.incomplete.as_ref().filter(|w| w.model_id.is_empty() || w.model_id == model_id)
    }

    fn data_path(&self, id: &str) -> Option<PathBuf> {
        // Ids are our own hex hashes; refuse anything else (path safety).
        if id.len() != 16 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        self.dir.as_ref().map(|d| d.join("library").join(format!("{id}.bin")))
    }

    fn save_index(&self) -> io::Result<()> {
        match &self.dir {
            Some(d) => crate::settings::save(d, INDEX_FILE, INDEX_VERSION, &self.index),
            None => Ok(()),
        }
    }

    /// Store a validated animation. The same content keeps its first entry
    /// (the name is updated).
    pub fn add(&mut self, name: &str, kind: ScreenItemKind, anim: &ScreenAnimation) -> io::Result<ScreenItem> {
        // One Keymotion animation per library: a second one (a backup or
        // a profile from another computer) is the one already here.
        if kind == ScreenItemKind::Keymotion {
            if let Some(item) = self.index.items.iter().find(|i| i.kind == ScreenItemKind::Keymotion) {
                return Ok(item.clone());
            }
        }
        let data = screen::blob_unpadded(anim);
        let id = content_id(&data);
        let name = clean_name(name);
        if let Some(item) = self.index.items.iter_mut().find(|i| i.id == id) {
            if kind == ScreenItemKind::Keymotion && !item.kind.built_in() {
                // The same picture stored as an ordinary item (by hand, or by
                // a backend that did not know the kind yet) becomes it.
                item.kind = kind;
                item.name = name;
            } else if !item.kind.built_in() {
                item.name = name;
            }
            let item = item.clone();
            self.save_index()?;
            return Ok(item);
        }
        if let Some(path) = self.data_path(&id) {
            fs::create_dir_all(path.parent().expect("library dir"))?;
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, &data)?;
            fs::rename(&tmp, &path)?;
        }
        let item = ScreenItem {
            id,
            name,
            kind,
            frames: anim.frames().len() as u32,
            loop_ms: anim.loop_ms(),
            pages: anim.pages() as u32,
            created_ms: now_ms(),
            uploaded_ms: None,
        };
        self.index.items.push(item.clone());
        self.save_index()?;
        Ok(item)
    }

    /// The stored data, validated again on the way out.
    pub fn animation(&self, id: &str) -> Result<ScreenAnimation, String> {
        let data = self.data(id)?;
        from_blob(&data).map_err(|e| e.to_string())
    }

    /// Raw stored data (header + frames), optionally only the first frames.
    /// `live` gives the last live card.
    pub fn data(&self, id: &str) -> Result<Vec<u8>, String> {
        if id == LIVE_ID {
            let path = self.dir.as_ref().map(|d| d.join(LIVE_FILE)).ok_or("library has no storage")?;
            return fs::read(path).map_err(|e| e.to_string());
        }
        if !self.index.items.iter().any(|i| i.id == id) {
            return Err("unknown library item".into());
        }
        let path = self.data_path(id).ok_or("library has no storage")?;
        fs::read(path).map_err(|e| e.to_string())
    }

    pub fn rename(&mut self, id: &str, name: &str) -> io::Result<()> {
        if let Some(item) = self.index.items.iter_mut().find(|i| i.id == id && !i.kind.built_in()) {
            item.name = clean_name(name);
            self.save_index()?;
        }
        Ok(())
    }

    /// Remove an item (the factory backup and Keymotion's animation
    /// cannot be removed).
    pub fn remove(&mut self, id: &str) -> io::Result<()> {
        let Some(pos) = self.index.items.iter().position(|i| i.id == id) else { return Ok(()) };
        if self.index.items[pos].kind.built_in() {
            return Err(io::Error::other("a built-in item cannot be removed"));
        }
        self.index.items.remove(pos);
        if let Some(path) = self.data_path(id) {
            let _ = fs::remove_file(path);
        }
        self.save_index()
    }

    /// Record an upload the keyboard confirmed.
    pub fn uploaded(&mut self, model_id: &str, id: &str) -> io::Result<()> {
        let now = now_ms();
        if let Some(item) = self.index.items.iter_mut().find(|i| i.id == id) {
            item.uploaded_ms = Some(now);
        }
        self.index.shown.insert(model_id.to_string(), id.to_string());
        self.count_upload(true);
        self.save_index()
    }

    /// Count an upload that reached the keyboard's flash, finished or not.
    /// `charge`: also take it from the live budget (manual uploads; the live
    /// mode takes its token before uploading).
    pub fn count_upload(&mut self, charge: bool) {
        if charge {
            self.index.live.budget.charge(crate::live::now_ms());
        }
        self.index.total_uploads += 1;
        *self.index.uploads_by_day.entry(today()).or_default() += 1;
        while self.index.uploads_by_day.len() > KEEP_DAYS {
            let oldest = self.index.uploads_by_day.keys().next().cloned().expect("non-empty");
            self.index.uploads_by_day.remove(&oldest);
        }
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_index()
    }

    pub fn live(&self) -> &LiveStored {
        &self.index.live
    }

    pub fn live_mut(&mut self) -> &mut LiveStored {
        &mut self.index.live
    }

    /// The library item to put back when the live mode is turned off: what
    /// the GIF page showed before a live card replaced it.
    pub fn live_restore_target(&self) -> Option<LiveRestore> {
        if !self.index.shown.values().any(|v| v == LIVE_ID) {
            return None;
        }
        let id = self.index.live.before.as_deref()?;
        let item = self.index.items.iter().find(|i| i.id == id)?;
        Some(LiveRestore { id: item.id.clone(), name: item.name.clone(), frames: item.frames, pages: item.pages })
    }

    /// The user's own picture: what the live mode replaced while a live
    /// card shows, else the library item on the GIF page now. Unknown
    /// means the factory animation, as in `live_uploaded`.
    pub fn live_own(&self) -> Option<LiveRestore> {
        if let Some(r) = self.live_restore_target() {
            return Some(r);
        }
        let shown = self.index.shown.values().find(|v| v.as_str() != LIVE_ID);
        let item = match shown {
            Some(id) => self.index.items.iter().find(|i| &i.id == id),
            None => self.index.items.iter().find(|i| i.kind == ScreenItemKind::Factory),
        }?;
        Some(LiveRestore { id: item.id.clone(), name: item.name.clone(), frames: item.frames, pages: item.pages })
    }

    /// The GIF page shows this live card already (as far as the app knows;
    /// not while it holds part of an upload).
    pub fn live_is_showing(&self, content: &str) -> bool {
        self.index.live.last_content.as_deref() == Some(content)
            && self.index.shown.values().any(|v| v == LIVE_ID)
            && self.index.incomplete.is_none()
    }

    /// The GIF page shows a live card from this source (as far as the app
    /// knows).
    pub fn live_source_showing(&self, source: &str) -> bool {
        self.index.live.last_source.as_deref() == Some(source) && self.index.shown.values().any(|v| v == LIVE_ID)
    }

    /// Record a live card the keyboard confirmed and keep its data for
    /// the preview of the GIF page.
    pub fn live_uploaded(
        &mut self,
        model_id: &str,
        anim: &ScreenAnimation,
        content: &str,
        item: &LiveItem,
    ) -> io::Result<()> {
        if let Some(d) = &self.dir {
            fs::create_dir_all(d)?;
            let path = d.join(LIVE_FILE);
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, screen::blob_unpadded(anim))?;
            fs::rename(&tmp, &path)?;
        }
        if !model_id.is_empty() {
            // Remember what the live mode replaced (the first time only), so
            // it can be put back. Unknown means the factory animation the
            // keyboard shipped with, as far as the app can tell.
            let prev = self.index.shown.get(model_id).cloned();
            if prev.as_deref() != Some(LIVE_ID) {
                let factory = self.index.items.iter().find(|i| i.kind == ScreenItemKind::Factory).map(|i| i.id.clone());
                self.index.live.before = prev.or(factory);
            }
            self.index.shown.insert(model_id.to_string(), LIVE_ID.to_string());
        }
        let live = &mut self.index.live;
        live.last_content = Some(content.to_string());
        live.last_source = Some(item.source.clone());
        live.last_name = Some(item.name.clone());
        live.last_ms = Some(now_ms());
        self.count_upload(false);
        self.save_index()
    }

    pub fn state(&self, model_id: &str) -> ScreenState {
        let mut items = self.index.items.clone();
        // Factory first, then Keymotion's own, then newest first.
        let rank = |k: ScreenItemKind| match k {
            ScreenItemKind::Factory => 2,
            ScreenItemKind::Keymotion => 1,
            _ => 0,
        };
        items.sort_by(|a, b| rank(b.kind).cmp(&rank(a.kind)).then(b.created_ms.total_cmp(&a.created_ms)));
        let incomplete = self.incomplete_for(model_id).map(|w| ScreenIncomplete {
            id: w.id.clone().filter(|id| items.iter().any(|i| &i.id == id)),
            at_ms: w.at_ms,
        });
        ScreenState {
            // A page holding part of an upload does not show the old item.
            shown: if incomplete.is_some() { None } else { self.index.shown.get(model_id).cloned() },
            items,
            uploads_today: self.index.uploads_by_day.get(&today()).copied().unwrap_or(0),
            uploads_total: self.index.total_uploads,
            limits: ScreenLimits::new(),
            incomplete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kb_protocol::sonix::screen::{rgb565, test_pattern, Frame};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("klavye-screen-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn add_dedupe_reload_and_remove() {
        let dir = temp_dir("lib");
        let mut lib = ScreenLibrary::open(Some(&dir));
        assert!(lib.state("m").items.is_empty(), "no factory backup in a fresh dir");
        let still = ScreenAnimation::still(test_pattern());
        let a = lib.add("Desen", ScreenItemKind::Image, &still).unwrap();
        let again = lib.add("Desen 2", ScreenItemKind::Image, &still).unwrap();
        assert_eq!(a.id, again.id, "same content, same entry");
        assert_eq!(lib.state("m").items.len(), 1);
        assert_eq!(lib.state("m").items[0].name, "Desen 2");
        let anim = ScreenAnimation::new(vec![(Frame::filled(rgb565(255, 0, 0)), 50), (Frame::filled(0), 50)]).unwrap();
        let b = lib.add("  ", ScreenItemKind::Animation, &anim).unwrap();
        assert_eq!(b.name, "Adsız");
        assert_eq!((b.frames, b.loop_ms, b.pages), (2, 200, 32));

        lib.uploaded("m", &b.id).unwrap();
        let reopened = ScreenLibrary::open(Some(&dir));
        let s = reopened.state("m");
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.shown.as_deref(), Some(b.id.as_str()));
        assert_eq!((s.uploads_today, s.uploads_total), (1, 1));
        assert_eq!(reopened.animation(&b.id).unwrap(), anim, "stored bytes round-trip");
        assert!(reopened.state("other").shown.is_none());

        let mut lib = reopened;
        lib.remove(&a.id).unwrap();
        assert_eq!(lib.state("m").items.len(), 1);
        assert!(lib.animation(&a.id).is_err());
        assert!(lib.data("../../etc").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_cards_remember_what_they_replaced() {
        let dir = temp_dir("live");
        let mut lib = ScreenLibrary::open(Some(&dir));
        let mine = lib.add("Benim", ScreenItemKind::Image, &ScreenAnimation::still(test_pattern())).unwrap();
        lib.uploaded("m", &mine.id).unwrap();
        assert!(lib.live_restore_target().is_none(), "no live card on the page yet");
        assert_eq!(lib.live_own().map(|o| o.id), Some(mine.id.clone()), "own picture: the one on the page");

        let card = ScreenAnimation::still(Frame::filled(rgb565(0, 0, 255)));
        let item = LiveItem { source: "now-playing".into(), name: "Şarkı".into() };
        lib.live_uploaded("m", &card, "c1", &item).unwrap();
        lib.live_uploaded("m", &card, "c2", &item).unwrap();
        let back = lib.live_restore_target().expect("the user's picture can be put back");
        assert_eq!(back.id, mine.id, "the second card does not overwrite what the first replaced");
        assert_eq!(lib.live_own().map(|o| o.id), Some(mine.id.clone()), "own picture: the one replaced");
        assert!(lib.live_source_showing("now-playing") && !lib.live_source_showing("pomodoro"));
        assert_eq!(lib.state("m").shown.as_deref(), Some(LIVE_ID));
        assert!(lib.data(LIVE_ID).is_ok(), "the live card is kept for the preview");

        // Putting it back (or any manual upload) leaves nothing to restore.
        let tokens = lib.live_mut().budget.tokens(now_ms());
        lib.uploaded("m", &mine.id).unwrap();
        assert!(lib.live_restore_target().is_none());
        assert!(!lib.live_source_showing("now-playing"), "the song card is gone from the page");
        assert!((tokens - lib.live_mut().budget.tokens(now_ms()) - 1.0).abs() < 0.01, "counted once");
        assert_eq!(lib.live_own().map(|o| o.id), Some(mine.id.clone()));
        let reopened = ScreenLibrary::open(Some(&dir));
        assert_eq!(reopened.state("m").uploads_today, 4);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A one-frame 240×135 GIF.
    fn factory_gif() -> Vec<u8> {
        let mut gif = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut gif, 240, 135, &[0, 0, 0]).unwrap();
            let f = gif::Frame {
                width: 240,
                height: 135,
                buffer: std::borrow::Cow::Owned(vec![0; 240 * 135]),
                ..Default::default()
            };
            enc.write_frame(&f).unwrap();
        }
        gif
    }

    #[test]
    fn a_cut_upload_marks_the_page_incomplete_until_one_finishes() {
        let dir = temp_dir("incomplete");
        let mut lib = ScreenLibrary::open(Some(&dir));
        assert!(lib.factory_at_risk(), "nothing uploaded, no backup");
        let a = lib.add("A", ScreenItemKind::Image, &ScreenAnimation::still(test_pattern())).unwrap();
        let b = lib.add("B", ScreenItemKind::Image, &ScreenAnimation::still(Frame::filled(rgb565(0, 0, 255)))).unwrap();
        lib.write_starting(Some("m"), Some(a.id.clone()));
        lib.write_ended(true, true);
        lib.uploaded("m", &a.id).unwrap();
        assert!(!lib.factory_at_risk(), "the app wrote the GIF page already");
        assert_eq!(lib.state("m").shown.as_deref(), Some(a.id.as_str()));

        // B starts: not shown as incomplete while it runs.
        lib.write_starting(Some("m"), Some(b.id.clone()));
        let s = lib.state("m");
        assert!(s.incomplete.is_none() && s.shown.as_deref() == Some(a.id.as_str()), "running, not cut");
        // It fails after the first page: the page holds part of B.
        lib.write_ended(true, false);
        let s = lib.state("m");
        assert_eq!(s.incomplete.as_ref().and_then(|i| i.id.as_deref()), Some(b.id.as_str()));
        assert!(s.shown.is_none(), "A is not on the keyboard any more");
        assert!(lib.state("other").incomplete.is_none(), "another model's page is fine");

        // Kept on disk; a try that never touched the flash changes nothing.
        let mut lib = ScreenLibrary::open(Some(&dir));
        assert_eq!(lib.state("m").incomplete.and_then(|i| i.id), Some(b.id.clone()));
        lib.write_starting(Some("m"), Some(a.id.clone()));
        lib.write_ended(false, false);
        assert_eq!(lib.state("m").incomplete.and_then(|i| i.id), Some(b.id.clone()));

        // Quit or crash midway: the mark written before the first page stays.
        lib.write_starting(Some("m"), Some(a.id.clone()));
        drop(lib);
        let mut lib = ScreenLibrary::open(Some(&dir));
        assert_eq!(lib.state("m").incomplete.and_then(|i| i.id), Some(a.id.clone()), "offered on the next start");

        // A finished upload clears it.
        lib.write_starting(Some("m"), Some(a.id.clone()));
        lib.write_ended(true, true);
        lib.uploaded("m", &a.id).unwrap();
        let s = ScreenLibrary::open(Some(&dir)).state("m");
        assert!(s.incomplete.is_none());
        assert_eq!(s.shown.as_deref(), Some(a.id.as_str()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_drivers_copy_becomes_the_factory_backup() {
        let dir = temp_dir("driver");
        let gif = factory_gif();
        let anim = factory_animation(&gif).unwrap();
        assert!(factory_animation(b"GIF89a\x10\0\x10\0").is_err(), "a broken file is refused");
        let mut lib = ScreenLibrary::open(Some(&dir));
        // The user imported the same GIF by hand before.
        let mine = lib.add("logo", ScreenItemKind::Animation, &anim).unwrap();
        let item = lib.store_factory(&gif, &anim).unwrap();
        assert_eq!((item.id.as_str(), item.kind), (mine.id.as_str(), ScreenItemKind::Factory));
        assert_eq!(item.name, FACTORY_NAME);
        assert_eq!(lib.state("m").items.len(), 1, "one entry, now the factory item");
        assert!(dir.join("screen").join(FACTORY_FILE).is_file(), "kept where the backup is looked for");
        assert!(!lib.factory_at_risk());
        assert_eq!(lib.store_factory(&gif, &anim).unwrap().id, item.id, "a second backup changes nothing");
        let lib = ScreenLibrary::open(Some(&dir));
        assert_eq!(lib.state("m").items.iter().filter(|i| i.kind == ScreenItemKind::Factory).count(), 1);
        assert!(ScreenLibrary::open(None).store_factory(&gif, &anim).is_err(), "nowhere to keep it");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keymotion_animation_is_one_and_kept() {
        let mut lib = ScreenLibrary::open(None);
        let a = ScreenAnimation::new(vec![(Frame::filled(1), 50), (Frame::filled(2), 50)]).unwrap();
        let b = ScreenAnimation::new(vec![(Frame::filled(3), 50), (Frame::filled(4), 50)]).unwrap();
        let first = lib.add("Keymotion", ScreenItemKind::Keymotion, &a).unwrap();
        assert_eq!(first.kind, ScreenItemKind::Keymotion);
        // Another one (drawn by a newer app, or from a backup) is the same item.
        assert_eq!(lib.add("Keymotion", ScreenItemKind::Keymotion, &b).unwrap().id, first.id);
        assert_eq!(lib.state("m").items.len(), 1);
        lib.rename(&first.id, "Benim").unwrap();
        assert_eq!(lib.state("m").items[0].name, "Keymotion", "not renamed");
        assert!(lib.remove(&first.id).is_err(), "not removed");
        // The same picture added by hand does not take its name.
        assert_eq!(lib.add("kopya", ScreenItemKind::Animation, &a).unwrap().name, "Keymotion");

        // Stored as an ordinary item first: it becomes the Keymotion item.
        let mut lib = ScreenLibrary::open(None);
        let plain = lib.add("Keymotion", ScreenItemKind::Animation, &a).unwrap();
        let item = lib.add("Keymotion", ScreenItemKind::Keymotion, &a).unwrap();
        assert_eq!((item.id, item.kind), (plain.id, ScreenItemKind::Keymotion));
        assert_eq!(lib.state("m").items.len(), 1);
    }

    #[test]
    fn factory_backup_is_imported_once_and_kept() {
        let dir = temp_dir("factory");
        let factory = dir.join("screen").join(FACTORY_FILE);
        fs::create_dir_all(factory.parent().unwrap()).unwrap();
        fs::write(&factory, factory_gif()).unwrap();
        let mut lib = ScreenLibrary::open(Some(&dir));
        let items = lib.state("m").items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ScreenItemKind::Factory);
        assert!(lib.remove(&items[0].id).is_err(), "factory backup stays");
        let lib = ScreenLibrary::open(Some(&dir));
        assert_eq!(lib.state("m").items.len(), 1, "imported once");
        let _ = fs::remove_dir_all(&dir);
    }
}
