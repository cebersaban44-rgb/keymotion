//! The program in front, shared by the game session (`games.rs`) and the
//! per-app profiles. The watcher (`kb_winsys::foreground`) only runs while
//! one of them wants it; it never opens another program.

use kb_winsys::foreground::{Front, Watcher};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Listener = Arc<dyn Fn(&Front) + Send + Sync>;

/// After the watcher failed to start, the next try waits this long.
const RETRY_AFTER: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Inner {
    watcher: Option<Watcher>,
    failed_at: Option<Instant>,
    wanted: Vec<&'static str>,
    listeners: Vec<Listener>,
    last: Option<Front>,
}

/// Hands each change of the program in front to the listeners.
#[derive(Default)]
pub struct FrontHub {
    inner: Arc<Mutex<Inner>>,
}

impl FrontHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// `f` gets every change while the watcher runs. It is called without
    /// any lock of the hub held, so it may call back into the hub; it should
    /// return quickly (hand the change to its own thread).
    pub fn listen(&self, f: impl Fn(&Front) + Send + Sync + 'static) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.listeners.push(Arc::new(f));
        }
    }

    /// The program in front as last seen (`None` while nobody watches).
    pub fn last(&self) -> Option<Front> {
        self.inner.lock().ok().and_then(|i| i.last.clone())
    }

    /// `who` needs the watcher (`on`) or not any more. It runs while anyone does.
    pub fn want(&self, who: &'static str, on: bool) {
        let Ok(mut inner) = self.inner.lock() else { return };
        let has = inner.wanted.contains(&who);
        if on && !has {
            inner.wanted.push(who);
        } else if !on && has {
            inner.wanted.retain(|w| *w != who);
        }
        let run = !inner.wanted.is_empty();
        let retry_ok = inner.failed_at.is_none_or(|t| t.elapsed() >= RETRY_AFTER);
        if run && inner.watcher.is_none() && retry_ok {
            let (tx, rx) = mpsc::channel::<Front>();
            match Watcher::start(tx) {
                Some(w) => {
                    tracing::info!("watching the program in front");
                    inner.failed_at = None;
                    inner.watcher = Some(w);
                    let shared = self.inner.clone();
                    // Ends when the watcher is dropped (its sender goes).
                    let _ = std::thread::Builder::new().name("front".into()).spawn(move || {
                        while let Ok(front) = rx.recv() {
                            // Not under the lock: a listener may take its own
                            // lock while its owner holds that one and calls
                            // `want` (lock order).
                            let listeners = {
                                let Ok(mut inner) = shared.lock() else { break };
                                inner.last = Some(front.clone());
                                inner.listeners.clone()
                            };
                            for f in &listeners {
                                f(&front);
                            }
                        }
                    });
                }
                None => {
                    tracing::warn!("the program in front cannot be watched; trying again in a minute");
                    inner.failed_at = Some(Instant::now());
                }
            }
        } else if !run && inner.watcher.is_some() {
            tracing::info!("no longer watching the program in front");
            inner.last = None;
            // Dropping stops the thread; not under the lock the forwarder needs.
            let w = inner.watcher.take();
            drop(inner);
            drop(w);
        }
    }
}
