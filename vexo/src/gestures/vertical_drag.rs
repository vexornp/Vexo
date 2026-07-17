//! VerticalDragRecognizer — recognizes a vertical drag (cumulative Δy past slop).
//!
//! State transitions on ArenaEvent:
//! - Down  → store positions, total_delta_y = 0, stay Pending
//! - Move  → accumulate total_delta_y += |delta.y|; if total > VERTICAL_DRAG_SLOP → Accepted
//! - Up    → if Pending → Rejected (was a tap); if Accepted → stays Accepted
//! - Cancel → Rejected
//!
//! Uses CUMULATIVE delta (sum of per-move deltas), not net displacement, so
//! back-and-forth jitter still counts as drag intent. Matches Flutter's
//! VerticalDragGestureRecognizer.

use crate::core::{Logical, Point};

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
use super::VERTICAL_DRAG_SLOP;

pub struct VerticalDragRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
    last_position: Point<Logical>,
    total_delta_y: f32,
}

impl VerticalDragRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
            last_position: Point::zero(),
            total_delta_y: 0.0,
        }
    }

    /// Last pointer position seen — read by ScrollViewElement to compute the
    /// per-move scroll delta.
    pub fn last_position(&self) -> Point<Logical> {
        self.last_position
    }

    /// Cumulative vertical movement since down. Read by the element for
    /// diagnostics; scroll deltas are computed from `last_position` deltas.
    pub fn total_delta_y(&self) -> f32 {
        self.total_delta_y
    }

    pub fn down_position(&self) -> Point<Logical> {
        self.down_position
    }
}

impl Default for VerticalDragRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for VerticalDragRecognizer {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext) {
        if self.rejected() {
            return;
        }
        match event {
            ArenaEvent::Down { .. } => {
                self.down_position = ctx.down_position;
                self.last_position = ctx.down_position;
                self.total_delta_y = 0.0;
            }
            ArenaEvent::Move { .. } => {
                let delta_y = ctx.current_position.y - self.last_position.y;
                self.last_position = ctx.current_position;
                self.total_delta_y += delta_y.abs();
                if self.resolution == RecognizerResolution::Pending
                    && self.total_delta_y > VERTICAL_DRAG_SLOP
                {
                    self.resolution = RecognizerResolution::Accepted;
                }
            }
            ArenaEvent::Up { .. } => {
                if self.resolution == RecognizerResolution::Pending {
                    self.resolution = RecognizerResolution::Rejected;
                }
                // If already Accepted, stays Accepted (drag completed).
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
    fn drag_accepts_on_cumulative_move_past_slop() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 60.0),
            },
            &ctx(down, Point::new(50.0, 60.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 70.0),
            },
            &ctx(down, Point::new(50.0, 70.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
    }

    #[test]
    fn drag_stays_pending_on_single_small_move() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 60.0),
            },
            &ctx(down, Point::new(50.0, 60.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn drag_rejects_on_up_without_slop() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Up { position: down }, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn drag_stays_accepted_after_slop() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 80.0),
            },
            &ctx(down, Point::new(50.0, 80.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 75.0),
            },
            &ctx(down, Point::new(50.0, 75.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
    }

    #[test]
    fn drag_rejects_on_cancel() {
        let mut r = VerticalDragRecognizer::new();
        let p = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(&ArenaEvent::Cancel, &ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn drag_cumulative_back_and_forth_still_breaches() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        // +15
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 65.0),
            },
            &ctx(down, Point::new(50.0, 65.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        // -15 (back to start, net 0, but cumulative 30)
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 50.0),
            },
            &ctx(down, Point::new(50.0, 50.0)),
        );
        assert_eq!(
            r.resolution(),
            RecognizerResolution::Accepted,
            "cumulative 30 > 18 slop, even though net displacement is 0"
        );
    }
}
