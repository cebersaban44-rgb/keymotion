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
//! * How many go is counted elsewhere ([`super::limits`]: the user's hourly
//!   number, all agents together); a card that never reached the screen
//!   (not drawn in time, the user dropped it, a newer card took its place
//!   before it went up) is said here ([`Due::failed`],
//!   [`CardBook::start`], [`CardBook::orphan_dropped`]) so its count is
//!   given back.
//! * The same card again is not sent while the first is on its way or on
//!   the screen: nothing to upload.
//! * An agent's card on the screen that no card here will take back (one
//!   replaced on its way that still went up, one left from before a
//!   restart) goes back to the user's picture too ([`CardBook::tick`]).

use std::collections::VecDeque;

/// The live mode's source for agents' cards.
pub const SOURCE: &str = "agent";
/// The page draws a card within this, or it counts as failed.
pub const DRAW_WAIT_MS: u64 = 15_000;
/// A card handed to the live mode that is neither waiting there nor on the
/// screen after this was dropped (the user, the live mode turned off).
const GONE_AFTER_MS: u64 = 2_000;
/// A card left on the screen with nothing to take it back (from before a
/// restart) goes after this.
pub const LEFT_BEHIND_MS: u64 = 30_000;
/// Settled cards remembered for a `--wait` that asks after.
const HISTORY: usize = 16;

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

/// What the live mode says of agents' cards now, read at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OnScreen {
    /// An agent's card is on the screen.
    pub shown: bool,
    /// An agent's card waits for the budget or goes up now.
    pub waiting: bool,
    /// The user's own picture is planned to come back (or coming back).
    pub back_planned: bool,
}

/// What the worker should do after a look ([`CardBook::tick`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Due {
    /// Put the user's picture back after this long: the card just reached
    /// the screen, or one is there that nothing else takes back.
    pub return_after_ms: Option<u64>,
    /// Something changed (a `--wait` may end).
    pub changed: bool,
    /// Cards that did not reach the screen: their counts go back.
    pub failed: Vec<String>,
}

/// A card replaced while the live mode had it: it may still go up.
#[derive(Debug, Clone)]
struct Orphan {
    id: String,
    /// How long it stays once on the screen.
    show_ms: u64,
}

#[derive(Debug, Default)]
pub struct CardBook {
    next: u64,
    current: Option<Card>,
    /// Settled cards' ids and stages, newest last, for a `--wait` that asks
    /// after.
    last: VecDeque<(String, Stage)>,
    orphan: Option<Orphan>,
}

/// The agent a source belongs to: its name before `:`.
pub fn agent_of(source: &str) -> &str {
    source.split(':').next().unwrap_or(source)
}

impl CardBook {
    fn settle(&mut self, id: String, stage: Stage) {
        self.last.retain(|(i, _)| *i != id);
        self.last.push_back((id, stage));
        while self.last.len() > HISTORY {
            self.last.pop_front();
        }
    }

    /// The card on its way or on the screen has this content.
    pub fn same(&self, content: u64) -> Option<String> {
        self.current
            .as_ref()
            .filter(|c| c.content == content && !matches!(c.stage, Stage::Failed(_) | Stage::Replaced))
            .map(|c| c.id.clone())
    }

    /// A new card (counted by the caller): it takes the place of the one on
    /// its way. Its id, and the id of a card it replaced before the page
    /// drew it: that one is never written, so its count goes back.
    pub fn start(&mut self, content: u64, show_ms: u64, name: &str, now: u64) -> (String, Option<String>) {
        self.next += 1;
        let id = format!("card-{}", self.next);
        let mut never_written = None;
        if let Some(old) = self.current.take() {
            match old.stage {
                // The live mode has it: it may still reach the screen.
                Stage::Live => self.orphan = Some(Orphan { id: old.id.clone(), show_ms: old.show_ms }),
                Stage::Drawing => never_written = Some(old.id.clone()),
                _ => {}
            }
            let stage = if old.stage.settled() { old.stage } else { Stage::Replaced };
            self.settle(old.id, stage);
        }
        self.current = Some(Card {
            id: id.clone(),
            name: name.to_string(),
            content,
            show_ms,
            stage: Stage::Drawing,
            since: now,
            drawn_at: None,
            shown_at: None,
        });
        (id, never_written)
    }

    /// The card on its way went to the live mode in the place of an agent's
    /// card still waiting there ("the newest card wins"): the card replaced
    /// before it, which now never goes up. Its id (its count goes back).
    pub fn orphan_dropped(&mut self) -> Option<String> {
        self.orphan.take().map(|o| o.id)
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

    /// Card `id` did not make it: `why`. `true` when it was on its way
    /// (its count goes back).
    pub fn failed(&mut self, id: &str, why: &str) -> bool {
        let Some(c) = self.current.as_mut().filter(|c| c.id == id && !c.stage.settled()) else { return false };
        c.stage = Stage::Failed(why.to_string());
        true
    }

    /// Where card `id` is.
    pub fn stage(&self, id: &str) -> Option<Stage> {
        match &self.current {
            Some(c) if c.id == id => Some(c.stage.clone()),
            _ => self.last.iter().rev().find(|(i, _)| i == id).map(|(_, s)| s.clone()),
        }
    }

    /// The worker's look, with what the live mode says now.
    pub fn tick(&mut self, now: u64, screen: OnScreen) -> Due {
        let mut due = Due::default();
        if let Some(c) = self.current.as_ref() {
            match c.stage {
                Stage::Drawing if now.saturating_sub(c.since) >= DRAW_WAIT_MS => {
                    let id = c.id.clone();
                    self.failed(&id, "The card was not drawn in time (is Keymotion's window process stuck?).");
                    due.failed.push(id);
                    due.changed = true;
                }
                Stage::Live if screen.shown && !screen.waiting => {
                    let show = c.show_ms;
                    if let Some(c) = self.current.as_mut() {
                        c.stage = Stage::Shown;
                        c.shown_at = Some(now);
                    }
                    // Its own return covers a card replaced before it.
                    self.orphan = None;
                    due.return_after_ms = Some(show);
                    due.changed = true;
                }
                Stage::Live
                    if !screen.waiting && c.drawn_at.is_some_and(|t| now.saturating_sub(t) >= GONE_AFTER_MS) =>
                {
                    let id = c.id.clone();
                    self.failed(&id, "The card did not reach the screen: it was dropped, or the live screen went off.");
                    due.failed.push(id);
                    due.changed = true;
                }
                Stage::Shown if c.shown_at.is_some_and(|t| now.saturating_sub(t) >= c.show_ms) || !screen.shown => {
                    // Its time is up (the user's picture is back), or another
                    // card took the screen.
                    if let Some(c) = self.current.take() {
                        self.settle(c.id, c.stage);
                    }
                    due.changed = true;
                }
                _ => {}
            }
            if self.current.as_ref().is_some_and(|c| matches!(c.stage, Stage::Failed(_))) {
                if let Some(c) = self.current.take() {
                    self.settle(c.id, c.stage);
                }
            }
        }
        // An agent's card on the screen that no card here takes back: one
        // replaced on its way that went up all the same, or one from before
        // a restart. Nothing of ours is on its way (that one would replace
        // it, or fail and come here).
        if self.current.is_none() && !screen.waiting {
            if screen.shown && !screen.back_planned {
                due.return_after_ms = Some(self.orphan.take().map_or(LEFT_BEHIND_MS, |o| o.show_ms));
            } else if !screen.shown {
                // Nothing of the agents' waits or shows: the card replaced
                // while the live mode had it never went up (another card
                // took its place there, or the live screen went off).
                if let Some(o) = self.orphan.take() {
                    due.failed.push(o.id);
                    due.changed = true;
                }
            }
        }
        due
    }

    /// A card is on its way or on the screen, or one replaced on its way
    /// may still go up: the worker keeps looking.
    pub fn busy(&self) -> bool {
        self.current.is_some() || self.orphan.is_some()
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

    const SHOWN: OnScreen = OnScreen { shown: true, waiting: false, back_planned: false };
    const SHOWN_BACK: OnScreen = OnScreen { shown: true, waiting: false, back_planned: true };
    const WAITING: OnScreen = OnScreen { shown: false, waiting: true, back_planned: false };
    const NONE: OnScreen = OnScreen { shown: false, waiting: false, back_planned: false };

    #[test]
    fn a_card_never_drawn_fails_and_says_so_for_its_count() {
        assert_eq!(agent_of("claude-code:3f9a"), "claude-code");
        let mut b = CardBook::default();
        let id = b.start(1, 60_000, "card", 0).0;
        assert_eq!(id, "card-1");
        assert!(b.tick(DRAW_WAIT_MS - 1, NONE) == Due::default());
        let due = b.tick(DRAW_WAIT_MS, NONE);
        assert!(due.changed);
        assert_eq!(due.failed, std::slice::from_ref(&id), "its count goes back");
        assert!(matches!(b.stage(&id), Some(Stage::Failed(_))));
        assert!(b.drawn(&id, DRAW_WAIT_MS + 1).is_err(), "too late");
    }

    #[test]
    fn a_card_goes_drawing_waiting_shown_and_then_back() {
        let mut b = CardBook::default();
        let id = b.start(7, 60_000, "card", 0).0;
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
        let first = b.start(1, 60_000, "card", 0).0;
        let (second, never_written) = b.start(2, 60_000, "card", 10);
        assert_eq!(b.stage(&first), Some(Stage::Replaced));
        assert_eq!(never_written, Some(first.clone()), "not drawn yet: it never goes up, its count goes back");
        assert!(b.drawn(&first, 15).is_err(), "the page's late drawing is not sent");
        b.drawn(&second, 20).unwrap();
        b.tick(500, NONE);
        assert_eq!(b.stage(&second), Some(Stage::Live), "a moment's grace");
        let due = b.tick(20 + GONE_AFTER_MS, NONE);
        assert!(matches!(b.stage(&second), Some(Stage::Failed(_))));
        assert_eq!(due.failed, [second]);
        assert!(!b.busy(), "the first was never with the live mode");
    }

    #[test]
    fn a_card_replaced_while_going_up_still_goes_back() {
        // Card 1 is with the live mode (uploading); card 2 replaces it and
        // then fails; card 1 reaches the screen all the same.
        let mut b = CardBook::default();
        let first = b.start(1, 90_000, "card", 0).0;
        b.drawn(&first, 10).unwrap();
        let (second, never_written) = b.start(2, 60_000, "card", 20);
        assert_eq!(b.stage(&first), Some(Stage::Replaced));
        assert_eq!(never_written, None, "the live mode has it: it may still go up");
        assert!(b.failed(&second, "broken picture"));
        assert_eq!(b.tick(30, WAITING), Due::default(), "card 1 still uploading");
        assert!(b.busy());
        let due = b.tick(5_000, SHOWN);
        assert_eq!(due.return_after_ms, Some(90_000), "card 1's own time, then the user's picture");
        assert!(!b.busy());
        // Planned: not again.
        assert_eq!(b.tick(5_100, SHOWN_BACK).return_after_ms, None);
    }

    #[test]
    fn a_card_replaced_while_waiting_that_never_goes_up_gives_its_count_back() {
        // Card 1 waits for the budget; card 2 takes its place there.
        let mut b = CardBook::default();
        let first = b.start(1, 60_000, "card", 0).0;
        b.drawn(&first, 10).unwrap();
        let second = b.start(2, 60_000, "card", 20).0;
        b.drawn(&second, 30).unwrap();
        assert_eq!(b.orphan_dropped(), Some(first.clone()), "the live mode said it took card 1's place");
        assert_eq!(b.orphan_dropped(), None, "once");
        // Card 1 waits; card 2 fails; then the live screen is turned off:
        // nothing of the agents waits or shows, card 1 never went up.
        let mut b = CardBook::default();
        let first = b.start(1, 60_000, "card", 0).0;
        b.drawn(&first, 10).unwrap();
        let second = b.start(2, 60_000, "card", 20).0;
        b.failed(&second, "broken picture");
        assert!(b.tick(30, WAITING).failed.is_empty(), "card 1 still waiting");
        let due = b.tick(40, NONE);
        assert_eq!(due.failed, [first]);
        assert!(!b.busy());
    }

    #[test]
    fn a_card_left_from_before_a_restart_goes_back() {
        let mut b = CardBook::default();
        assert_eq!(b.tick(0, SHOWN).return_after_ms, Some(LEFT_BEHIND_MS));
        assert_eq!(b.tick(100, SHOWN_BACK).return_after_ms, None);
        assert_eq!(b.tick(200, NONE).return_after_ms, None);
        // A card of ours on its way: it will take the screen.
        b.start(1, 60_000, "card", 300);
        assert_eq!(b.tick(400, SHOWN).return_after_ms, None);
    }

    #[test]
    fn a_wait_finds_a_card_settled_a_while_ago() {
        let mut b = CardBook::default();
        let first = b.start(1, 60_000, "card", 0).0;
        for i in 0..HISTORY as u64 {
            b.start(10 + i, 60_000, "card", 10 + i);
        }
        assert_eq!(b.stage(&first), Some(Stage::Replaced), "still known");
        b.start(99, 60_000, "card", 99);
        assert_eq!(b.stage(&first), None, "only the last {HISTORY}");
    }

    #[test]
    fn content_ids_tell_cards_apart() {
        assert_eq!(content_id(&[b"a", b"b"]), content_id(&[b"a", b"b"]));
        assert_ne!(content_id(&[b"ab", b""]), content_id(&[b"a", b"b"]));
    }
}
