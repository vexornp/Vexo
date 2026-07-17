//! Arena events fed to gesture recognizers by the GestureArena.

use crate::core::Logical;
use crate::core::Point;

/// An event delivered to every recognizer in the arena.
#[derive(Clone, Copy, Debug)]
pub enum ArenaEvent {
    Down { position: Point<Logical> },
    Move { position: Point<Logical> },
    Up { position: Point<Logical> },
    Cancel,
}
