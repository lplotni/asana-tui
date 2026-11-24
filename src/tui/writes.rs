//! Own writes Asana hasn't confirmed yet, and the loads that may predate them
//! (spec F30). With a refresh every few seconds, a board or detail load is
//! often running when a write is made. Its result can still have the task as
//! it was, and would undo what the screen already shows (a completed card
//! coming back, a saved description or a posted comment vanishing). Such a
//! result is stale: the caller keeps what it shows and loads again once the
//! writes are settled. Pure state, no I/O (ADR-002).

use std::collections::HashMap;

/// What a load is of.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Load {
    /// A project's board, by project GID.
    Board(String),
    /// A task's detail, by task GID.
    Detail(String),
}

#[derive(Debug, Default)]
pub struct Writes {
    /// Writes sent and not answered yet, by the task (or, for a new task,
    /// the project) they are about.
    in_flight: HashMap<String, usize>,
    /// Bumped whenever a write is sent or answered.
    epoch: u64,
    /// For each running load, the epoch it started in, or `None` when a write
    /// was in flight then.
    loads: HashMap<Load, Option<u64>>,
    /// Ticks since a write was last sent or answered, while any is in flight.
    stuck_ticks: u32,
}

impl Writes {
    /// A write about `key` was sent.
    pub fn sent(&mut self, key: &str) {
        *self.in_flight.entry(key.to_owned()).or_default() += 1;
        self.changed();
    }

    /// Asana answered a write about `key`, whether it took it or not.
    pub fn settled(&mut self, key: &str) {
        if let Some(count) = self.in_flight.get_mut(key) {
            *count -= 1;
            if *count == 0 {
                self.in_flight.remove(key);
            }
        }
        self.changed();
    }

    fn changed(&mut self) {
        self.epoch += 1;
        self.stuck_ticks = 0;
    }

    /// `load` started now.
    pub fn started(&mut self, load: Load) {
        let mark = self.in_flight.is_empty().then_some(self.epoch);
        self.loads.insert(load, mark);
    }

    /// Whether the result of `load` may predate an own write: one was in
    /// flight when it started, or was sent or answered since. A load that
    /// wasn't recorded as started isn't stale. Forgets the load.
    pub fn stale(&mut self, load: &Load) -> bool {
        self.loads
            .remove(load)
            .is_some_and(|mark| mark != Some(self.epoch))
    }

    pub fn any_in_flight(&self) -> bool {
        !self.in_flight.is_empty()
    }

    /// A refresh tick. Writes still in flight after `limit` ticks without
    /// news are given up on (their answer never came), so loads are shown
    /// again.
    pub fn tick(&mut self, limit: u32) {
        if self.in_flight.is_empty() {
            return;
        }
        self.stuck_ticks += 1;
        if self.stuck_ticks >= limit {
            self.in_flight.clear();
            self.changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Load {
        Load::Board("42".into())
    }

    #[test]
    fn a_load_with_no_write_around_it_is_fresh() {
        let mut w = Writes::default();
        w.started(board());

        assert!(!w.stale(&board()));
    }

    #[test]
    fn a_load_a_write_was_sent_during_is_stale() {
        let mut w = Writes::default();
        w.started(board());
        w.sent("t1");

        assert!(w.stale(&board()));
    }

    #[test]
    fn a_load_started_while_a_write_was_in_flight_is_stale_even_once_it_settled() {
        let mut w = Writes::default();
        w.sent("t1");
        w.started(board());
        w.settled("t1");

        assert!(w.stale(&board()));
    }

    #[test]
    fn a_load_started_after_every_write_settled_is_fresh() {
        let mut w = Writes::default();
        w.sent("t1");
        w.settled("t1");
        w.started(Load::Detail("t1".into()));

        assert!(!w.stale(&Load::Detail("t1".into())));
        assert!(!w.any_in_flight());
    }

    #[test]
    fn two_writes_to_one_task_both_have_to_settle() {
        let mut w = Writes::default();
        w.sent("t1");
        w.sent("t1");
        w.settled("t1");

        assert!(w.any_in_flight());
        w.settled("t1");
        assert!(!w.any_in_flight());
    }

    #[test]
    fn a_load_that_wasnt_recorded_is_not_stale() {
        let mut w = Writes::default();
        w.sent("t1");

        assert!(!w.stale(&board()));
    }

    #[test]
    fn writes_without_an_answer_are_given_up_after_the_limit() {
        let mut w = Writes::default();
        w.sent("t1");
        w.tick(2);
        assert!(w.any_in_flight());

        w.tick(2);

        assert!(!w.any_in_flight());
        w.started(board());
        assert!(!w.stale(&board()));
    }

    #[test]
    fn news_about_a_write_restarts_the_limit() {
        let mut w = Writes::default();
        w.sent("t1");
        w.tick(2);
        w.sent("t2");
        w.tick(2);

        assert!(w.any_in_flight());
    }
}
