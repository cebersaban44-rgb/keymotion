//! "Çalan şarkı" source: what Windows reports as playing (Global System
//! Media Transport Controls), polled once a second while the live mode and
//! this source are on.
//!
//! A track is announced (`now-playing` event) once it has played for
//! `SETTLE` without pause, so skipping through songs costs no uploads. A
//! paused session is never chosen over one that plays.

use crate::dto::TrackInfo;
use crate::live::LiveHandle;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const EVENT_NOW_PLAYING: &str = "now-playing";
const POLL: Duration = Duration::from_secs(1);
const IDLE_POLL: Duration = Duration::from_secs(2);
/// Played this long without pause before it is worth an upload. Most skips
/// happen in the first seconds; longer only delays the card (owner's
/// feedback 2026-11-10: 12 s felt late).
pub const SETTLE: Duration = Duration::from_secs(4);
/// Covers often arrive after the title: wait this many more polls for one.
const COVER_WAITS: u8 = 3;
/// Largest cover accepted.
const COVER_MAX: u64 = 8 * 1024 * 1024;

#[derive(Default)]
pub struct NowPlaying {
    cover: Mutex<Option<Vec<u8>>>,
}

impl NowPlaying {
    /// Cover art of the last announced track.
    pub fn cover(&self) -> Option<Vec<u8>> {
        self.cover.lock().ok().and_then(|c| c.clone())
    }
}

/// Friendly name for the app that plays.
fn app_name(aumid: &str) -> String {
    let l = aumid.to_ascii_lowercase();
    let known = [
        ("spotify", "Spotify"),
        ("msedge", "Edge"),
        ("chrome", "Chrome"),
        ("firefox", "Firefox"),
        ("opera", "Opera"),
        ("brave", "Brave"),
        ("zunemusic", "Medya Oynatıcı"),
        ("mediaplayer", "Medya Oynatıcı"),
        ("applemusic", "Apple Music"),
        ("itunes", "iTunes"),
        ("vlc", "VLC"),
        ("foobar2000", "foobar2000"),
        ("tidal", "TIDAL"),
        ("deezer", "Deezer"),
        ("youtube", "YouTube Music"),
    ];
    if let Some((_, name)) = known.iter().find(|(k, _)| l.contains(k)) {
        return (*name).to_string();
    }
    let base = aumid.rsplit(['\\', '/', '!']).next().unwrap_or(aumid);
    base.trim_end_matches(".exe").trim_end_matches(".EXE").to_string()
}

fn track_key(app: &str, title: &str, artist: &str) -> String {
    format!("{app}\u{1f}{title}\u{1f}{artist}")
}

struct Current {
    info: TrackInfo,
    played: Duration,
    cover_waits: u8,
}

pub fn spawn(app: AppHandle, live: Arc<LiveHandle>) -> Arc<NowPlaying> {
    let state = Arc::new(NowPlaying::default());
    let out = state.clone();
    std::thread::Builder::new()
        .name("now-playing".into())
        .spawn(move || run(app, live, state))
        .expect("spawn now-playing thread");
    out
}

#[cfg(windows)]
fn run(app: AppHandle, live: Arc<LiveHandle>, state: Arc<NowPlaying>) {
    let mut source = win::Source::default();
    let mut current: Option<Current> = None;
    let mut last = Instant::now();
    loop {
        if !live.now_playing_wanted() {
            if current.take().is_some() {
                live.set_track(None);
            }
            source = win::Source::default();
            std::thread::sleep(IDLE_POLL);
            last = Instant::now();
            continue;
        }
        let elapsed = last.elapsed();
        last = Instant::now();
        match source.poll() {
            Ok(Some(t)) => {
                let name = app_name(&t.aumid);
                let key = track_key(&name, &t.title, &t.artist);
                let changed = current.as_ref().is_none_or(|c| c.info.key != key);
                if changed {
                    // A new track starts its settle time from zero.
                    current = Some(Current {
                        info: TrackInfo {
                            key,
                            title: t.title,
                            artist: t.artist,
                            album: t.album,
                            app: name,
                            playing: t.playing,
                            settled: false,
                            has_cover: false,
                        },
                        played: Duration::ZERO,
                        cover_waits: 0,
                    });
                    live.set_track(current.as_ref().map(|c| c.info.clone()));
                } else {
                    let c = current.as_mut().expect("same track");
                    let was_playing = c.info.playing;
                    c.info.playing = t.playing;
                    if t.playing {
                        c.played += elapsed.min(POLL * 3);
                    }
                    if !c.info.settled && t.playing && c.played >= SETTLE && !c.info.title.trim().is_empty() {
                        let cover = source.cover().unwrap_or_else(|e| {
                            tracing::debug!("no cover: {e}");
                            None
                        });
                        if cover.is_none() && c.cover_waits < COVER_WAITS {
                            // Give a late cover a moment before the card is made.
                            c.cover_waits += 1;
                        } else {
                            c.info.has_cover = cover.is_some();
                            c.info.settled = true;
                            if let Ok(mut slot) = state.cover.lock() {
                                *slot = cover;
                            }
                            tracing::info!(app = %c.info.app, "track settled");
                            let _ = app.emit(EVENT_NOW_PLAYING, c.info.clone());
                            live.set_track(Some(c.info.clone()));
                        }
                    } else if was_playing != t.playing {
                        live.set_track(Some(c.info.clone()));
                    }
                }
            }
            Ok(None) => {
                if let Some(c) = current.as_mut() {
                    if c.info.playing {
                        c.info.playing = false;
                        live.set_track(Some(c.info.clone()));
                    }
                }
            }
            Err(e) => {
                tracing::debug!("media sessions unavailable: {e}");
                // Ask Windows again next time (Explorer restarts, etc.).
                source = win::Source::default();
            }
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(not(windows))]
fn run(_app: AppHandle, _live: Arc<LiveHandle>, _state: Arc<NowPlaying>) {}

#[cfg(windows)]
mod win {
    use super::COVER_MAX;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Storage::Streams::DataReader;

    pub struct Track {
        pub aumid: String,
        pub title: String,
        pub artist: String,
        pub album: String,
        pub playing: bool,
    }

    #[derive(Default)]
    pub struct Source {
        manager: Option<Manager>,
        session: Option<Session>,
    }

    impl Source {
        fn manager(&mut self) -> windows::core::Result<&Manager> {
            if self.manager.is_none() {
                self.manager = Some(Manager::RequestAsync()?.join()?);
            }
            Ok(self.manager.as_ref().expect("set above"))
        }

        /// The session to show: one that plays (Windows' current session
        /// first), else the current one, paused.
        pub fn poll(&mut self) -> windows::core::Result<Option<Track>> {
            let manager = self.manager()?.clone();
            let current = manager.GetCurrentSession().ok();
            let is_playing = |s: &Session| {
                s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(|st| st == Status::Playing).unwrap_or(false)
            };
            let mut chosen = current.clone().filter(is_playing);
            if chosen.is_none() {
                if let Ok(sessions) = manager.GetSessions() {
                    chosen = sessions.into_iter().find(is_playing);
                }
            }
            let playing = chosen.is_some();
            let Some(session) = chosen.or(current) else {
                self.session = None;
                return Ok(None);
            };
            let props = session.TryGetMediaPropertiesAsync()?.join()?;
            let track = Track {
                aumid: session.SourceAppUserModelId()?.to_string(),
                title: props.Title()?.to_string(),
                artist: props.Artist()?.to_string(),
                album: props.AlbumTitle().map(|a| a.to_string()).unwrap_or_default(),
                playing,
            };
            self.session = Some(session);
            Ok(Some(track))
        }

        /// Cover art bytes (JPEG or PNG) of the session last polled.
        pub fn cover(&self) -> windows::core::Result<Option<Vec<u8>>> {
            let Some(session) = &self.session else { return Ok(None) };
            let props = session.TryGetMediaPropertiesAsync()?.join()?;
            let Ok(thumb) = props.Thumbnail() else { return Ok(None) };
            let stream = thumb.OpenReadAsync()?.join()?;
            let size = stream.Size()?;
            if size == 0 || size > COVER_MAX {
                return Ok(None);
            }
            let reader = DataReader::CreateDataReader(&stream)?;
            let loaded = reader.LoadAsync(size as u32)?.join()? as usize;
            let mut buf = vec![0u8; loaded];
            reader.ReadBytes(&mut buf)?;
            Ok(Some(buf))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads what Windows reports as playing right now; touches no keyboard.
    /// `cargo test -p klavye-studio now_playing_probe -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn now_playing_probe() {
        let mut src = win::Source::default();
        match src.poll() {
            Ok(Some(t)) => {
                let cover = src.cover().ok().flatten().map(|c| c.len());
                println!(
                    "PLAYING={} app={} title={:?} artist={:?} album={:?} cover_bytes={cover:?}",
                    t.playing,
                    app_name(&t.aumid),
                    t.title,
                    t.artist,
                    t.album
                );
            }
            Ok(None) => println!("no media session"),
            Err(e) => panic!("media sessions unavailable: {e}"),
        }
    }

    #[test]
    fn friendly_app_names() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name("MSEdge"), "Edge");
        assert_eq!(app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "Medya Oynatıcı");
        assert_eq!(app_name(r"C:\Program Files\Foo\Bar.exe"), "Bar");
        assert_ne!(track_key("A", "t", "x"), track_key("A", "tx", ""));
    }
}
