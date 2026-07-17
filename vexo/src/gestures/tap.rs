//! TapRecognizer — recognizes a tap (down + up without slop breach).
//!
//! State transitions on ArenaEvent:
//! - Down  → store position, stay Pending
//! - Move  → if |Δx| or |Δy| from down exceeds TAP_SLOP → Rejected, else Pending
//! - Up    → if still Pending → Accepted (tap wins)
//! - Cancel → Rejected

use crate::core::Logical;
use crate::core::Point;

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
use super::TAP_SLOP;

pub struct TapRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
}

impl TapRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
        }
    }
}

impl Default for TapRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for TapRecognizer {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext) {
        if self.rejected() {
            return;
        }
        match event {
            ArenaEvent::Down { .. } => {
                self.down_position = ctx.down_position;
            }
            ArenaEvent::Move { .. } => {
                let dx = (ctx.current_position.x - self.down_position.x).abs();
                let dy = (ctx.current_position.y - self.down_position.y).abs();
                if dx > TAP_SLOP || dy > TAP_SLOP {
                    self.resolution = RecognizerResolution::Rejected;
                }
            }
            ArenaEvent::Up { .. } => {
                if self.resolution == RecognizerResolution::Pending {
                    self.resolution = RecognizerResolution::Accepted;
                }
            }
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
        }
    }

    fn resolution(&self) -> RecognizerResolution {
        self.resolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(down: Point<Logical>, current: Point<Logical>) -> ArenaContext {
        ArenaContext {
            down_position: down,
            current_position: current,
        }
    }

    #[test]
    fn tap_accepts_on_up_after_down_no_move() {
        let mut r = TapRecognizer::new();
        let p = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(&ArenaEvent::Up { position: p }, &ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
    }

    #[test]
    fn tap_rejects_on_move_past_slop_vertical() {
        let mut r = TapRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(50.0, 80.0); // Δy = 30 > 18
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, &ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn tap_rejects_on_move_past_slop_horizontal() {
        let mut r = TapRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(80.0, 50.0); // Δx = 30 > 18
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, &ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn tap_stays_pending_on_move_within_slop() {
        let mut r = TapRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(55.0, 60.0); // Δx=5, Δy=10, both < 18
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, &ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn tap_rejects_on_cancel() {
        let mut r = TapRecognizer::new();
        let p = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(&ArenaEvent::Cancel, &ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn tap_rejects_on_up_after_slop_breach() {
        let mut r = TapRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(50.0, 80.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, &ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
        r.handle_event(&ArenaEvent::Up { position: moved }, &ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }
}
