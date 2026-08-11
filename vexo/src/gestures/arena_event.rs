//! Arena events fed to gesture recognizers by the GestureArena.

use crate::core::Logical;
use crate::core::Point;

/// An event delivered to every recognizer in the arena.
#[derive(Clone, Copy, Debug)]
pub enum ArenaEvent {
    Down {
        position: Point<Logical>,
    },
    Move {
        position: Point<Logical>,
    },
    Up {
        position: Point<Logical>,
    },
    Cancel,
    /// Fired each animation frame while the arena is active (pointer down,
    /// not yet resolved). Carries the frame's `Instant` so time-based
    /// recognizers (e.g. `LongPressRecognizer`) can compute elapsed time.
    /// Pure event-driven recognizers (Tap, VerticalDrag) ignore this.
    Tick {
        now: std::time::Instant,
    },
}
