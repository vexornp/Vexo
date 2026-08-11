//! LongPressRecognizer — recognizes a long-press (pointer down held still
//! for LONG_PRESS_DURATION).
//!
//! State transitions on ArenaEvent:
//! - Down  → store position, stay Pending (down_time deferred to first Tick)
//! - Move  → if |Δx| or |Δy| from down exceeds LONG_PRESS_SLOP → Rejected
//! - Tick  → if Pending and elapsed >= LONG_PRESS_DURATION → Accepted
//! - Up    → Rejected (finger lifted before the duration — was a tap)
//! - Cancel → Rejected
//!
//! Slop check uses NET displacement from `down_position` (like TapRecognizer),
//! not cumulative delta (like VerticalDragRecognizer): a finger that drifts
//! back and forth within slop is still "essentially still" — a long-press.

use std::any::Any;
use std::time::Instant;

use crate::core::{Logical, Point};

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
use super::{LONG_PRESS_DURATION, LONG_PRESS_SLOP};

pub struct LongPressRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
    down_time: Option<Instant>,
}

impl LongPressRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
            down_time: None,
        }
    }

    /// The pointer's press location. Read by `GestureDetectorElement::
    /// on_arena_winner_update` to source the long-press callback's
    /// position argument (semantically: the long-press happened *at* where
    /// the finger went down, not where it drifted to by 500ms).
    pub fn down_position(&self) -> Point<Logical> {
        self.down_position
    }
}

impl Default for LongPressRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for LongPressRecognizer {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext) {
        if self.rejected() {
            return;
        }
        match event {
            ArenaEvent::Down { .. } => {
                self.down_position = ctx.down_position;
                // ctx doesn't carry the time; Down's `now` is inferred from
                // the first Tick. Set down_time on the first Tick instead
                // (see Tick arm). Leave None here so a Tick without a prior
                // Down is a no-op.
            }
            ArenaEvent::Move { .. } => {
                let dx = (ctx.current_position.x - self.down_position.x).abs();
                let dy = (ctx.current_position.y - self.down_position.y).abs();
                if dx > LONG_PRESS_SLOP || dy > LONG_PRESS_SLOP {
                    self.resolution = RecognizerResolution::Rejected;
                }
            }
            ArenaEvent::Tick { now } => {
                // First Tick after Down: record the start time. This is the
                // clock that drives the 500ms threshold. Using the first
                // Tick (not Down) means down_time is always None until the
                // first frame, so a stray Tick without a prior Down is a
                // no-op (defensive).
                if self.down_time.is_none() {
                    self.down_time = Some(*now);
                }
                if let Some(start) = self.down_time {
                    if now.duration_since(start) >= LONG_PRESS_DURATION {
                        self.resolution = RecognizerResolution::Accepted;
                    }
                }
            }
            ArenaEvent::Up { .. } => {
                // Finger lifted before the duration — was a tap, not a
                // long-press.
                self.resolution = RecognizerResolution::Rejected;
            }
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
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
    use std::time::Duration;

    fn ctx(down: Point<Logical>, current: Point<Logical>) -> ArenaContext {
        ArenaContext {
            down_position: down,
            current_position: current,
        }
    }

    #[test]
    fn long_press_accepts_on_tick_after_500ms() {
        let mut r = LongPressRecognizer::new();
        let p = Point::new(50.0, 50.0);
        let start = Instant::now();
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        // First Tick after Down: records down_time (start), stays Pending.
        r.handle_event(&ArenaEvent::Tick { now: start }, &ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        // Tick at 499ms — still Pending.
        r.handle_event(
            &ArenaEvent::Tick {
                now: start + Duration::from_millis(499),
            },
            &ctx(p, p),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        // Tick at 500ms — Accepted.
        r.handle_event(
            &ArenaEvent::Tick {
                now: start + Duration::from_millis(500),
            },
            &ctx(p, p),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
    }

    #[test]
    fn long_press_rejects_on_up_before_500ms() {
        let mut r = LongPressRecognizer::new();
        let p = Point::new(50.0, 50.0);
        let start = Instant::now();
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(
            &ArenaEvent::Tick {
                now: start + Duration::from_millis(300),
            },
            &ctx(p, p),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        r.handle_event(&ArenaEvent::Up { position: p }, &ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn long_press_rejects_on_move_past_slop() {
        let mut r = LongPressRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(50.0, 80.0); // Δy = 30 > 18
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, &ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn long_press_rejects_on_cancel() {
        let mut r = LongPressRecognizer::new();
        let p = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(&ArenaEvent::Cancel, &ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn long_press_tick_is_noop_before_down() {
        let mut r = LongPressRecognizer::new();
        let now = Instant::now();
        // Tick without a prior Down — down_time stays None, no-op.
        r.handle_event(
            &ArenaEvent::Tick { now },
            &ctx(Point::zero(), Point::zero()),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn long_press_stays_pending_on_tick_before_500ms() {
        let mut r = LongPressRecognizer::new();
        let p = Point::new(50.0, 50.0);
        let start = Instant::now();
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(
            &ArenaEvent::Tick {
                now: start + Duration::from_millis(250),
            },
            &ctx(p, p),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn long_press_stays_pending_on_move_within_slop() {
        let mut r = LongPressRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(55.0, 60.0); // Δx=5, Δy=10, both < 18
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, &ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }
}
