//! GestureArena — per-pointer resolver for competing gesture recognizers.
//!
//! One arena per active pointer press. Elements register recognizers into it
//! on press; the arena feeds ArenaEvents to every recognizer and resolves a
//! single winner via the slop model:
//!   - Any recognizer that returns Accepted on Move → wins immediately,
//!     all others are rejected, arena closes.
//!   - On Up, if still open: any recognizer that accepts on Up wins;
//!     otherwise sweep to the first non-rejected recognizer (Flutter default).
//!   - On Cancel: arena closes with no winner.
//!
//! The arena is pure: it does NOT fire user callbacks. EventHandler reads the
//! winner and notifies the owning element.

use crate::core::{Logical, Point};
use crate::id::ElementKey;

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer};

/// Result of feeding an event to the arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaOutcome {
    /// A recognizer accepted; `winner_index` is its position in the arena.
    Resolved { winner_index: usize },
    /// Arena was cancelled (no winner).
    ClosedNoWinner,
    /// Still competing — no decision yet.
    Open,
}

struct ArenaEntry {
    recognizer: Box<dyn GestureRecognizer>,
    owner: ElementKey,
}

pub struct GestureArena {
    entries: Vec<ArenaEntry>,
    down_position: Point<Logical>,
    winner: Option<usize>,
    closed: bool,
}

impl GestureArena {
    pub fn new(down_position: Point<Logical>) -> Self {
        Self {
            entries: Vec::new(),
            down_position,
            winner: None,
            closed: false,
        }
    }

    /// Register a recognizer with its owning element. No-op if the arena is
    /// already closed (single-winner invariant).
    pub fn add(&mut self, recognizer: Box<dyn GestureRecognizer>, owner: ElementKey) {
        if self.closed {
            return;
        }
        self.entries.push(ArenaEntry { recognizer, owner });
    }

    /// Number of registered recognizers.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The winning recognizer, if resolved.
    pub fn winner_recognizer(&self) -> Option<&dyn GestureRecognizer> {
        self.winner.map(|i| self.entries[i].recognizer.as_ref())
    }

    /// The owning element of the winning recognizer, if resolved.
    pub fn winner_owner(&self) -> Option<ElementKey> {
        self.winner.map(|i| self.entries[i].owner)
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Feed an event to every recognizer, then resolve.
    pub fn handle_event(&mut self, event: ArenaEvent) -> ArenaOutcome {
        if self.closed {
            // Already resolved; a closed arena stays closed with its winner.
            return match self.winner {
                Some(i) => ArenaOutcome::Resolved { winner_index: i },
                None => ArenaOutcome::ClosedNoWinner,
            };
        }

        let current_position = match &event {
            ArenaEvent::Down { position } => *position,
            ArenaEvent::Move { position } => *position,
            ArenaEvent::Up { position } => *position,
            ArenaEvent::Cancel => self.down_position,
        };
        let ctx = ArenaContext {
            down_position: self.down_position,
            current_position,
        };

        // Feed event to every recognizer.
        for entry in &mut self.entries {
            entry.recognizer.handle_event(&event, &ctx);
        }

        match event {
            ArenaEvent::Cancel => {
                self.closed = true;
                self.winner = None;
                ArenaOutcome::ClosedNoWinner
            }
            ArenaEvent::Move { .. } | ArenaEvent::Up { .. } => {
                self.try_resolve();
                match self.winner {
                    Some(i) => ArenaOutcome::Resolved { winner_index: i },
                    None if self.closed => ArenaOutcome::ClosedNoWinner,
                    None => ArenaOutcome::Open,
                }
            }
            ArenaEvent::Down { .. } => ArenaOutcome::Open,
        }
    }

    /// Resolution sweep: if any recognizer accepted → it wins, reject others.
    /// If on Up and none accepted but some are pending → sweep to first
    /// non-rejected (Flutter default sweep).
    fn try_resolve(&mut self) {
        if self.closed {
            return;
        }
        // First pass: look for an Accepted recognizer.
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.recognizer.accepted() {
                self.declare_winner(i);
                return;
            }
        }
        // If we reach here on an Up event, sweep to first non-rejected.
        // (Called after feeding Up, so rejections are up-to-date.)
    }

    /// Called when the arena should attempt a sweep (e.g. on Up with no
    /// accepted recognizer). Public so EventHandler can trigger a sweep
    /// after feeding Up.
    pub fn sweep_on_up(&mut self) {
        if self.closed {
            return;
        }
        // First: any accepted?
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.recognizer.accepted() {
                self.declare_winner(i);
                return;
            }
        }
        // Sweep to first non-rejected (Pending). Rejected ones are skipped.
        for (i, entry) in self.entries.iter().enumerate() {
            if !entry.recognizer.rejected() {
                self.declare_winner(i);
                return;
            }
        }
        // All rejected — no winner.
        self.closed = true;
        self.winner = None;
    }

    fn declare_winner(&mut self, index: usize) {
        self.winner = Some(index);
        self.closed = true;
        // Reject all others (they've lost).
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if i != index && !entry.recognizer.rejected() {
                // Feed Cancel to losers so they clean up.
                let ctx = ArenaContext {
                    down_position: self.down_position,
                    current_position: self.down_position,
                };
                entry.recognizer.handle_event(&ArenaEvent::Cancel, &ctx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gestures::{TapRecognizer, VerticalDragRecognizer};

    fn dummy_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    fn arena_with_tap_and_drag() -> GestureArena {
        let mut arena = GestureArena::new(Point::new(50.0, 50.0));
        arena.add(Box::new(TapRecognizer::new()), dummy_element_key());
        arena.add(Box::new(VerticalDragRecognizer::new()), dummy_element_key());
        arena
    }

    #[test]
    fn arena_resolves_drag_winner_on_slop_breach() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        let outcome = arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 80.0), // Δy = 30 > 18
        });
        assert_eq!(outcome, ArenaOutcome::Resolved { winner_index: 1 });
        assert!(arena.winner_recognizer().unwrap().accepted());
    }

    #[test]
    fn arena_resolves_tap_winner_on_release_before_slop() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Up { position: down });
        arena.sweep_on_up();
        assert!(arena.is_closed());
        assert_eq!(arena.winner, Some(0)); // tap at index 0
    }

    #[test]
    fn arena_open_during_small_move() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        let outcome = arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 55.0), // Δy = 5 < 18
        });
        assert_eq!(outcome, ArenaOutcome::Open);
    }

    #[test]
    fn arena_closed_no_winner_on_cancel() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        let outcome = arena.handle_event(ArenaEvent::Cancel);
        assert_eq!(outcome, ArenaOutcome::ClosedNoWinner);
        assert!(!arena.winner.is_some());
    }

    #[test]
    fn arena_single_recipient_sweeps_on_up() {
        let mut arena = GestureArena::new(Point::new(50.0, 50.0));
        arena.add(Box::new(TapRecognizer::new()), dummy_element_key());
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Up { position: down });
        arena.sweep_on_up();
        assert_eq!(arena.winner, Some(0));
    }

    #[test]
    fn arena_deepest_wins_on_tie() {
        // Deepest (index 0) is the inner drag; outer (index 1) is the outer drag.
        let mut arena = GestureArena::new(Point::new(50.0, 50.0));
        arena.add(Box::new(VerticalDragRecognizer::new()), dummy_element_key());
        arena.add(Box::new(VerticalDragRecognizer::new()), dummy_element_key());
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 80.0),
        });
        assert_eq!(arena.winner, Some(0), "deepest (index 0) wins the tie");
    }

    #[test]
    fn arena_add_noop_after_closed() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Cancel);
        let len_before = arena.len();
        arena.add(Box::new(TapRecognizer::new()), dummy_element_key());
        assert_eq!(arena.len(), len_before, "add is no-op on closed arena");
    }

    #[test]
    fn arena_no_second_winner_after_closed() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 80.0),
        });
        // Drag won (index 1). Feed Up — winner must stay index 1.
        let outcome = arena.handle_event(ArenaEvent::Up {
            position: Point::new(50.0, 80.0),
        });
        assert_eq!(outcome, ArenaOutcome::Resolved { winner_index: 1 });
        assert_eq!(arena.winner, Some(1));
    }
}
