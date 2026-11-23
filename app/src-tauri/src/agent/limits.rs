//! How often agents may write the keyboard's memory: profile lighting (at
//! most [`PROFILE_WRITES`] in any 24 hours) and screen cards (the user's
//! number in any hour).
//!
//! An agent's name is whatever its `--source` says, so it cannot be
//! trusted: the limits are for **all agents together**; the counts per
//! agent are only shown. The times are kept in `agent-limits.json`, so a
//! restart does not start the count again, and they are Unix times (UTC):
//! a change of time zone moves nothing. A time ahead of the clock (the
//! clock was put back) counts as now, so putting the clock back frees no
//! write either. A slot is taken before the write and given back when the
//! write did not happen, under one lock: two requests at once cannot both
//! take the last one.

use serde::{Deserialize, Serialize};

pub const HOUR_MS: u64 = 3_600_000;
pub const DAY_MS: u64 = 24 * HOUR_MS;
/// Profile writes by all agents together, in any 24 hours.
pub const PROFILE_WRITES: usize = 10;
/// The file the times are kept in.
pub const FILE: &str = "agent-limits.json";

/// One write: when (Unix ms), by which agent, and what (to give it back).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub at: u64,
    pub agent: String,
    #[serde(default)]
    pub id: String,
}

/// The writes of one kind in a rolling window of `span` ms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Window {
    entries: Vec<Entry>,
}

impl Window {
    /// Forget what is older than `span`; a time ahead of `now` counts as
    /// now.
    fn prune(&mut self, now: u64, span: u64) {
        for e in &mut self.entries {
            e.at = e.at.min(now);
        }
        self.entries.retain(|e| now.saturating_sub(e.at) < span);
        // Never more than a full window's worth of anything read from disk.
        let keep = 256;
        if self.entries.len() > keep {
            self.entries.drain(..self.entries.len() - keep);
        }
    }

    /// Room for one more (`limit` in `span`), or the ms until there is.
    pub fn room(&mut self, limit: usize, now: u64, span: u64) -> Result<(), u64> {
        self.prune(now, span);
        if self.entries.len() < limit.max(1) {
            return Ok(());
        }
        let oldest = self.entries.iter().map(|e| e.at).min().unwrap_or(now);
        Err((oldest + span).saturating_sub(now).max(1))
    }

    /// Take a slot for `agent`'s write `id` (after [`Self::room`]).
    pub fn take(&mut self, agent: &str, id: &str, now: u64) {
        self.entries.push(Entry { at: now, agent: agent.to_string(), id: id.to_string() });
    }

    /// Write `id` did not happen: its slot is free again.
    pub fn give_back(&mut self, id: &str) -> bool {
        match self.entries.iter().rposition(|e| e.id == id) {
            Some(i) => {
                self.entries.remove(i);
                true
            }
            None => false,
        }
    }

    /// Writes in the window ending `now`: all, and `agent`'s.
    pub fn count(&self, now: u64, span: u64) -> usize {
        self.entries.iter().filter(|e| now.saturating_sub(e.at.min(now)) < span).count()
    }

    pub fn count_for(&self, agent: &str, now: u64, span: u64) -> usize {
        self.entries.iter().filter(|e| e.agent == agent && now.saturating_sub(e.at.min(now)) < span).count()
    }
}

/// Both kinds, as kept in [`FILE`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub profile_writes: Window,
    pub cards: Window,
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 1_800_000_000_000;

    #[test]
    fn a_rolling_window_for_everyone_together() {
        let mut w = Window::default();
        for i in 0..PROFILE_WRITES {
            assert!(w.room(PROFILE_WRITES, T + i as u64, DAY_MS).is_ok());
            w.take(if i % 2 == 0 { "claude-code" } else { "a" }, &format!("w{i}"), T + i as u64);
        }
        // Another name gets no fresh count.
        let wait = w.room(PROFILE_WRITES, T + 1_000, DAY_MS).unwrap_err();
        assert_eq!(wait, DAY_MS - 1_000, "until the first write is a day old");
        assert_eq!((w.count(T + 1_000, DAY_MS), w.count_for("a", T + 1_000, DAY_MS)), (10, 5));
        // A day after the first: one free.
        assert!(w.room(PROFILE_WRITES, T + DAY_MS, DAY_MS).is_ok());
        assert_eq!(w.count(T + DAY_MS, DAY_MS), 9);
    }

    #[test]
    fn a_write_that_did_not_happen_gives_its_slot_back() {
        let mut w = Window::default();
        w.take("a", "w1", T);
        assert!(w.room(1, T, HOUR_MS).is_err());
        assert!(w.give_back("w1"));
        assert!(!w.give_back("w1"));
        assert!(w.room(1, T, HOUR_MS).is_ok());
    }

    #[test]
    fn putting_the_clock_back_frees_nothing_and_the_times_survive_a_restart() {
        let mut l = Limits::default();
        l.cards.take("a", "card-1", T);
        l.cards.take("b", "card-2", T + 60_000);
        // A restart: the file read back.
        let mut read: Limits = serde_json::from_value(serde_json::to_value(&l).unwrap()).unwrap();
        assert_eq!(read, l);
        assert!(read.cards.room(2, T + 120_000, HOUR_MS).is_err());
        // The clock put back a day: both still count, from now on.
        let back = T - DAY_MS;
        assert_eq!(read.cards.room(2, back, HOUR_MS).unwrap_err(), HOUR_MS);
        assert!(read.cards.room(2, back + HOUR_MS, HOUR_MS).is_ok());
        // An old file without the field reads as empty.
        let empty: Limits = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Limits::default());
    }
}
