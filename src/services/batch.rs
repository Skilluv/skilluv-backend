//! Walking a table that is bigger than memory, and saying so when we stop.
//!
//! ## The two ways to get this wrong
//!
//! **No bound at all.** The weekly digest read every active account into a
//! `Vec` before sending anything. It works until it does not, and the day
//! it stops working is the day the table is large enough that the process
//! is killed mid-run — so nobody gets a digest, and the logs show a restart
//! rather than a cause.
//!
//! **A bound that truncates silently.** Worse, and three jobs had one. The
//! drip sequences selected `LIMIT 500`. The five-hundred-and-first eligible
//! person never received the day-one email — not late, never — and nothing
//! anywhere said so. A cap that looks like it works is how a system lies to
//! the person operating it.
//!
//! ## What this does instead
//!
//! Keyset pagination: order by a unique key, remember the last one seen,
//! ask for the next page. Constant memory, no `OFFSET` scan that gets
//! slower every page, and — unlike `OFFSET` — correct when rows are
//! inserted while the walk is running.
//!
//! A per-run ceiling still exists, because a job that runs every hour
//! should not still be running when the next tick arrives. The difference
//! is that reaching it is **logged and counted**, and the next run resumes
//! from where this one stopped rather than from the beginning.

use uuid::Uuid;

/// How many rows to fetch per round trip.
///
/// Small enough that one page fits comfortably in memory alongside
/// whatever the job builds per row; large enough that the round trips are
/// not the cost.
pub const PAGE: i64 = 500;

/// How the walk ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// Every row was seen. The normal case, and the only one worth being
    /// quiet about.
    Exhausted,
    /// The per-run ceiling was reached. The next run resumes from here.
    Truncated,
}

/// A walk in progress.
///
/// Kept as a struct rather than three loose variables so a job cannot
/// forget to advance the cursor — the bug that turns a paginated loop into
/// an infinite one sending the same email forever.
pub struct Walk {
    /// Last key seen. `None` starts at the beginning.
    after: Option<Uuid>,
    seen: usize,
    ceiling: usize,
    label: &'static str,
}

impl Walk {
    /// Start a walk, capped at `ceiling` rows for this run.
    pub fn new(label: &'static str, ceiling: usize) -> Self {
        Self {
            after: None,
            seen: 0,
            ceiling,
            label,
        }
    }

    /// The cursor to bind, as `WHERE ($1::uuid IS NULL OR id > $1) ORDER BY id`.
    ///
    /// A single nullable bind rather than two query variants: two variants
    /// is two places for the ordering to drift apart, and an ordering that
    /// disagrees with the cursor skips rows.
    pub fn after(&self) -> Option<Uuid> {
        self.after
    }

    /// How many rows are still allowed this run, never more than a page.
    pub fn page_size(&self) -> i64 {
        PAGE.min((self.ceiling.saturating_sub(self.seen)) as i64)
    }

    /// Record a page and advance. `last` is the key of its final row.
    pub fn advance(&mut self, count: usize, last: Uuid) {
        self.seen += count;
        self.after = Some(last);
    }

    /// Whether to ask for another page.
    pub fn should_continue(&self, last_page_len: usize) -> bool {
        // A short page means the table is exhausted. Continuing would ask
        // for rows that are not there, forever.
        last_page_len >= PAGE as usize && self.seen < self.ceiling
    }

    pub fn seen(&self) -> usize {
        self.seen
    }

    /// Close the walk, reporting a ceiling that was hit rather than hiding
    /// it. Returns how it ended so the caller can put it in its own report.
    pub fn finish(self, last_page_len: usize) -> Ending {
        if last_page_len >= PAGE as usize && self.seen >= self.ceiling {
            metrics::counter!(
                "skilluv_batch_truncated_total",
                "job" => self.label
            )
            .increment(1);
            tracing::warn!(
                job = self.label,
                seen = self.seen,
                ceiling = self.ceiling,
                "batch stopped at its per-run ceiling — the rest resumes next run, \
                 but if this repeats the ceiling is too low for the volume"
            );
            return Ending::Truncated;
        }
        tracing::debug!(job = self.label, seen = self.seen, "batch exhausted");
        Ending::Exhausted
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn a_walk_stops_when_a_page_comes_back_short() {
        let walk = Walk::new("test", 10_000);
        assert!(
            !walk.should_continue(PAGE as usize - 1),
            "a short page means there is nothing after it"
        );
        assert!(walk.should_continue(PAGE as usize));
    }

    #[test]
    fn the_ceiling_shrinks_the_last_page_rather_than_overshooting() {
        let mut walk = Walk::new("test", 600);
        assert_eq!(walk.page_size(), PAGE);
        walk.advance(500, Uuid::new_v4());
        assert_eq!(
            walk.page_size(),
            100,
            "the last page must not fetch past the ceiling"
        );
    }

    #[test]
    fn a_full_walk_is_exhausted_and_a_capped_one_is_truncated() {
        let mut walk = Walk::new("test", 1_000);
        walk.advance(500, Uuid::new_v4());
        assert_eq!(walk.finish(499), Ending::Exhausted);

        let mut capped = Walk::new("test", 500);
        capped.advance(500, Uuid::new_v4());
        assert_eq!(capped.finish(500), Ending::Truncated);
    }

    #[test]
    fn advancing_moves_the_cursor() {
        // The bug this prevents: a loop that never advances re-reads the
        // first page forever, sending the same email on every iteration.
        let mut walk = Walk::new("test", 10);
        assert_eq!(walk.after(), None);
        let id = Uuid::new_v4();
        walk.advance(1, id);
        assert_eq!(walk.after(), Some(id));
        assert_eq!(walk.seen(), 1);
    }
}
