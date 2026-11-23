//! Screen cards from agents (`card`) and "a long task done" (F): which one
//! is on its way, how many each agent sent this hour, and when it reached
//! the screen.
//!
//! A card goes the way every live card goes (`live.rs`): the app's page
//! draws it (`agent-card` event, then `agents_card_drawn` with the
//! finished picture), the live mode uploads it on its budget, and after its
//! time the user's own picture comes back. Here:
//!
//! * One card at a time: a newer one replaces the one on its way, as the
//!   live mode keeps only the newest.
//! * Each agent (the source's name before `:`, so all sessions of one
//!   coding app together) sends at most the user's hourly number; a card
//!   that never reached the screen (not drawn in time, the user dropped it)
//!   does not count.
//! * The same card again is not sent while the first is on its way or on
//!   the screen: nothing to upload.

use std::collections::{HashMap, VecDeque};

/// The live mode's source for agents' cards.
pub const SOURCE: &str = "agent";
/// The page draws a card within this, or it counts as failed.
pub const DRAW_WAIT_MS: u64 = 15_000;
/// A card handed to the live mode that is neither waiting there nor on the
/// screen after this was dropped (the user, the live mode turned off).
const GONE_AFTER_MS: u64 = 2_000;
pub const HOUR_MS: u64 = 3_600_000;

/// Where a card is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Asked of the page; not drawn yet.
    Drawing,
    /// Drawn; with the live mode (waiting for its budget, or going up).
    Live,
    /// On the screen.
    Shown,
    /// It did not reach the screen; why.
    Failed(String),
    /// A newer card took its place before it showed.
    Replaced,
}

impl Stage {
    /// Nothing more will happen to it.
    pub fn settled(&self) -> bool {
        matches!(self, Self::Shown | Self::Failed(_) | Self::Replaced)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Drawing => "drawing",
            Self::Live => "waiting",
            Self::Shown => "shown",
            Self::Failed(_) => "failed",
            Self::Replaced => "replaced",
        }
    }
}

#[derive(Debug, Clone)]
struct Card {
    id: String,
    agent: String,
    /// The live mode's name for it.
    name: String,
    content: u64,
    show_ms: u64,
    stage: Stage,
    /// When it was asked for, drawn, shown.
    since: u64,
    drawn_at: Option<u64>,
    shown_at: Option<u64>,
}

/// What the live mode says of agents' cards now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OnScreen {
    /// An agent's card is on the screen.
    pub shown: bool,
    /// An agent's card waits for the budget or goes up now.
    pub waiting: bool,
}

/// What the worker should do after a look ([`CardBook::tick`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Due {
    /// The card just reached the screen: put the user's picture back after
    /// this long.
    pub return_after_ms: Option<u64>,
    /// Something changed (a `--wait` may end).
    pub changed: bool,
}

#[derive(Debug, Default)]
pub struct CardBook {
    next: u64,
    current: Option<Card>,
    /// The last settled card's id and stage, for a `--wait` that asks after.
    last: Option<(String, Stage)>,
    /// When each agent's counted cards went, in the last hour.
    sent: HashMap<String, VecDeque<u64>>,
}

/// The agent a source belongs to: its name before `:`.
pub fn agent_of(source: &str) -> &str {
    source.split(':').next().unwrap_or(source)
}

impl CardBook {
    /// `agent` may send another card now (`limit` an hour), or how long
    /// until it may.
    pub fn room(&mut self, agent: &str, limit: u8, now: u64) -> Result<(), u64> {
        let times = self.sent.entry(agent.to_string()).or_default();
        while times.front().is_some_and(|&t| now.saturating_sub(t) >= HOUR_MS) {
            times.pop_front();
        }
        if times.len() < usize::from(limit.max(1)) {
            Ok(())
        } else {
            Err(times.front().map_or(HOUR_MS, |&t| (t + HOUR_MS).saturating_sub(now)).max(1))
        }
    }

    /// Cards `agent` sent in the last hour.
    pub fn sent_this_hour(&self, agent: &str, now: u64) -> usize {
        self.sent.get(agent).map_or(0, |t| t.iter().filter(|&&t| now.saturating_sub(t) < HOUR_MS).count())
    }

    /// The card on its way or on the screen has this content.
    pub fn same(&self, content: u64) -> Option<String> {
        self.current
            .as_ref()
            .filter(|c| c.content == content && !matches!(c.stage, Stage::Failed(_) | Stage::Replaced))
            .map(|c| c.id.clone())
    }

    /// A new card from `agent`: counted, and it takes the place of the one
    /// on its way. Its id.
    pub fn start(&mut self, agent: &str, content: u64, show_ms: u64, name: &str, now: u64) -> String {
        self.next += 1;
        let id = format!("card-{}", self.next);
        if let Some(old) = self.current.take() {
            if !old.stage.settled() {
                self.last = Some((old.id, Stage::Replaced));
            }
        }
        self.sent.entry(agent.to_string()).or_default().push_back(now);
        self.current = Some(Card {
            id: id.clone(),
            agent: agent.to_string(),
            name: name.to_string(),
            content,
            show_ms,
            stage: Stage::Drawing,
            since: now,
            drawn_at: None,
            shown_at: None,
        });
        id
    }

    /// The page drew card `id`: it goes to the live mode, under the name
    /// returned. `Err` when it is not the card on its way any more.
    pub fn drawn(&mut self, id: &str, now: u64) -> Result<String, String> {
        match self.current.as_mut() {
            Some(c) if c.id == id && c.stage == Stage::Drawing => {
                c.stage = Stage::Live;
                c.drawn_at = Some(now);
                Ok(c.name.clone())
            }
            Some(c) if c.id == id => Err(format!("card {id} is {} already", c.stage.name())),
            _ => Err(format!("card {id} is not on its way any more")),
        }
    }

    /// Card `id` did not make it: `why`. Its agent gets its count back.
    pub fn failed(&mut self, id: &str, why: &str) -> bool {
        let Some(c) = self.current.as_mut().filter(|c| c.id == id && !c.stage.settled()) else { return false };
        c.stage = Stage::Failed(why.to_string());
        let (agent, since) = (c.agent.clone(), c.since);
        if let Some(times) = self.sent.get_mut(&agent) {
            if let Some(i) = times.iter().rposition(|&t| t == since) {
                times.remove(i);
            }
        }
        true
    }

    /// Where card `id` is.
    pub fn stage(&self, id: &str) -> Option<Stage> {
        match &self.current {
            Some(c) if c.id == id => Some(c.stage.clone()),
            _ => self.last.as_ref().filter(|(i, _)| i == id).map(|(_, s)| s.clone()),
        }
    }

    /// The worker's look, with what the live mode says now.
    pub fn tick(&mut self, now: u64, screen: OnScreen) -> Due {
        let mut due = Due::default();
        let Some(c) = self.current.as_ref() else { return due };
        match c.stage {
            Stage::Drawing if now.saturating_sub(c.since) >= DRAW_WAIT_MS => {
                let id = c.id.clone();
                self.failed(&id, "The card was not drawn in time (is Keymotion's window process stuck?).");
                due.changed = true;
            }
            Stage::Live if screen.shown && !screen.waiting => {
                let show = c.show_ms;
                if let Some(c) = self.current.as_mut() {
                    c.stage = Stage::Shown;
                    c.shown_at = Some(now);
                }
                due.return_after_ms = Some(show);
                due.changed = true;
            }
            Stage::Live if !screen.waiting && c.drawn_at.is_some_and(|t| now.saturating_sub(t) >= GONE_AFTER_MS) => {
                let id = c.id.clone();
                self.failed(&id, "The card did not reach the screen: it was dropped, or the live screen went off.");
                due.changed = true;
            }
            Stage::Shown if c.shown_at.is_some_and(|t| now.saturating_sub(t) >= c.show_ms) || !screen.shown => {
                // Its time is up (the user's picture is back), or another
                // card took the screen.
                let done = self.current.take().map(|c| (c.id, c.stage));
                self.last = done;
                due.changed = true;
            }
            _ => {}
        }
        if self.current.as_ref().is_some_and(|c| matches!(c.stage, Stage::Failed(_))) {
            self.last = self.current.take().map(|c| (c.id, c.stage));
        }
        due
    }

    /// A card is on its way or on the screen: the worker keeps looking.
    pub fn busy(&self) -> bool {
        self.current.is_some()
    }
}

/// A short fingerprint of a card's content, to tell the same card again.
pub fn content_id(parts: &[&[u8]]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.len().hash(&mut h);
        p.hash(&mut h);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOWN: OnScreen = OnScreen { shown: true, waiting: false };
    const WAITING: OnScreen = OnScreen { shown: false, waiting: true };
    const NONE: OnScreen = OnScreen { shown: false, waiting: false };

    #[test]
    fn each_agent_sends_its_hourly_number_and_a_failed_card_does_not_count() {
        let mut b = CardBook::default();
        assert_eq!(agent_of("claude-code:3f9a"), "claude-code");
        for i in 0..4 {
            assert!(b.room("claude-code", 4, i * 1000).is_ok());
            let id = b.start("claude-code", i, 60_000, "card", i * 1000);
            b.drawn(&id, i * 1000).unwrap();
        }
        let wait = b.room("claude-code", 4, 10_000).unwrap_err();
        assert_eq!(wait, HOUR_MS - 10_000, "until the first of the hour drops out");
        assert!(b.room("codex", 4, 10_000).is_ok(), "another agent has its own");
        assert!(b.room("claude-code", 4, HOUR_MS + 1).is_ok());
        assert_eq!(b.sent_this_hour("claude-code", HOUR_MS + 1), 3);
        // A card that was never drawn gives its count back.
        let mut b = CardBook::default();
        let id = b.start("cli", 1, 60_000, "card", 0);
        assert!(b.tick(DRAW_WAIT_MS - 1, NONE) == Due::default());
        assert!(b.tick(DRAW_WAIT_MS, NONE).changed);
        assert!(matches!(b.stage(&id), Some(Stage::Failed(_))));
        assert_eq!(b.sent_this_hour("cli", DRAW_WAIT_MS), 0);
        assert!(b.drawn(&id, DRAW_WAIT_MS + 1).is_err(), "too late");
    }

    #[test]
    fn a_card_goes_drawing_waiting_shown_and_then_back() {
        let mut b = CardBook::default();
        let id = b.start("cli", 7, 60_000, "card", 0);
        assert_eq!(b.same(7), Some(id.clone()), "the same card again is not sent");
        assert_eq!(b.same(8), None);
        b.drawn(&id, 100).unwrap();
        assert!(b.drawn(&id, 100).is_err(), "drawn once");
        assert_eq!(b.tick(200, WAITING), Due::default(), "waiting for the budget");
        assert_eq!(b.stage(&id), Some(Stage::Live));
        let due = b.tick(5_000, SHOWN);
        assert_eq!(due.return_after_ms, Some(60_000));
        assert_eq!(b.stage(&id), Some(Stage::Shown));
        assert!(b.tick(30_000, SHOWN) == Due::default());
        assert!(b.tick(65_000, SHOWN).changed, "its time is up");
        assert!(!b.busy());
        assert_eq!(b.stage(&id), Some(Stage::Shown), "still known to a late --wait");
        assert_eq!(b.same(7), None, "after its time the same card may come again");
    }

    #[test]
    fn a_newer_card_replaces_one_on_its_way_and_a_dropped_one_fails() {
        let mut b = CardBook::default();
        let first = b.start("a", 1, 60_000, "card", 0);
        let second = b.start("b", 2, 60_000, "card", 10);
        assert_eq!(b.stage(&first), Some(Stage::Replaced));
        assert_eq!(b.sent_this_hour("a", 10), 1, "it went; it counts");
        b.drawn(&second, 20).unwrap();
        b.tick(500, NONE);
        assert_eq!(b.stage(&second), Some(Stage::Live), "a moment's grace");
        b.tick(20 + GONE_AFTER_MS, NONE);
        assert!(matches!(b.stage(&second), Some(Stage::Failed(_))));
        assert_eq!(b.sent_this_hour("b", 3_000), 0);
    }

    #[test]
    fn content_ids_tell_cards_apart() {
        assert_eq!(content_id(&[b"a", b"b"]), content_id(&[b"a", b"b"]));
        assert_ne!(content_id(&[b"ab", b""]), content_id(&[b"a", b"b"]));
    }
}
