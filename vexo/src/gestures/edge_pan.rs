//! EdgePanRecognizer — recognizes a horizontal drag starting from the
//! leading (left) screen edge.
//!
//! Mirrors `VerticalDragRecognizer`'s slop/accept/reject model, with two
//! additions:
//! 1. The initial `Down` must land within `EDGE_WIDTH` of the left edge;
//!    otherwise the recognizer rejects immediately (a non-edge drag never
//!    competes, so a future horizontal-scroll recognizer isn't starved).
//! 2. Only rightward movement (positive Δx) accepts — a leftward drag from
//!    the edge stays Pending (does not accept), so it doesn't start a pop;
//!    on Up without any rightward slop it rejects, letting content (e.g. a
//!    scroll view) handle it.
//!
//! `total_delta_x` is the NET signed displacement (`last.x - down.x`), not
//! cumulative magnitude. This is what swipe-to-pop needs for finger-tracking
//! progress, and a finger that jitters in place without net rightward
//! movement shouldn't start a pop.

use crate::core::{Logical, Point};

use std::any::Any;

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
use super::{EDGE_WIDTH, HORIZONTAL_DRAG_SLOP};

pub struct EdgePanRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
    last_position: Point<Logical>,
}

impl EdgePanRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
            last_position: Point::zero(),
        }
    }

    /// Net signed horizontal displacement from the down position.
    /// Positive = rightward (the swipe-to-pop direction). Read by
    /// `EdgePanDetectorElement` to drive `on_update(total_delta_x)`.
    pub fn total_delta_x(&self) -> f32 {
        self.last_position.x - self.down_position.x
    }

    pub fn down_position(&self) -> Point<Logical> {
        self.down_position
    }

    pub fn last_position(&self) -> Point<Logical> {
        self.last_position
    }
}

impl Default for EdgePanRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for EdgePanRecognizer {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext) {
        if self.rejected() {
            return;
        }
        match event {
            ArenaEvent::Down { .. } => {
                if ctx.down_position.x <= EDGE_WIDTH {
                    self.down_position = ctx.down_position;
                    self.last_position = ctx.down_position;
                    self.resolution = RecognizerResolution::Pending;
                } else {
                    self.resolution = RecognizerResolution::Rejected;
                }
            }
            ArenaEvent::Move { .. } => {
                self.last_position = ctx.current_position;
                if self.resolution == RecognizerResolution::Pending {
                    let dx = self.total_delta_x();
                    let abs_dy = (ctx.current_position.y - self.down_position.y).abs();
                    if dx > HORIZONTAL_DRAG_SLOP && dx > abs_dy {
                        self.resolution = RecognizerResolution::Accepted;
                    } else if abs_dy > HORIZONTAL_DRAG_SLOP && abs_dy > dx {
                        self.resolution = RecognizerResolution::Rejected;
                    }
                }
            }
            ArenaEvent::Up { .. } => {
                if self.resolution == RecognizerResolution::Pending {
                    self.resolution = RecognizerResolution::Rejected;
                }
            }
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
            ArenaEvent::Tick { .. } => {} // EdgePan is purely event-driven; ignore the clock tick.
        }
    }

    fn resolution(&self) -> RecognizerResolution {
        self.resolution
    }

    fn as_any(&self) -> &dyn Any {
        self
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
    fn down_in_edge_zone_stays_pending() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn down_outside_edge_zone_rejects_immediately() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn rightward_move_past_slop_accepts() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(40.0, 52.0),
            },
            &ctx(down, Point::new(40.0, 52.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
        assert_eq!(r.total_delta_x(), 30.0);
    }

    #[test]
    fn leftward_move_does_not_accept() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(-20.0, 50.0),
            },
            &ctx(down, Point::new(-20.0, 50.0)),
        );
        assert_eq!(
            r.resolution(),
            RecognizerResolution::Pending,
            "leftward drag from edge must not accept"
        );
    }

    #[test]
    fn vertical_dominant_move_rejects() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(15.0, 100.0),
            },
            &ctx(down, Point::new(15.0, 100.0)),
        );
        assert_eq!(
            r.resolution(),
            RecognizerResolution::Rejected,
            "vertical-dominant movement must reject so vertical scroll can win"
        );
    }

    #[test]
    fn up_without_slop_rejects() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Up { position: down }, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn stays_accepted_after_slop() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 50.0),
            },
            &ctx(down, Point::new(50.0, 50.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(30.0, 50.0),
            },
            &ctx(down, Point::new(30.0, 50.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
        assert_eq!(
            r.total_delta_x(),
            20.0,
            "net displacement tracks last position"
        );
    }

    #[test]
    fn cancel_rejects() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Cancel, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }
}
