//! Gesture arena: disambiguates competing gesture recognizers for a pointer.
//!
//! Currently implements Tap vs. VerticalDrag (scroll) vs. LongPress
//! disambiguation via a slop-threshold + time rule, matching Flutter's
//! GestureArena behavior for this recognizer set.

pub mod arena;
pub mod arena_event;
pub mod long_press;
pub mod recognizer;
pub mod tap;
pub mod velocity_tracker;
pub mod vertical_drag;

pub use arena::{ArenaOutcome, GestureArena};
pub use arena_event::ArenaEvent;
pub use long_press::LongPressRecognizer;
pub use recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
pub use tap::TapRecognizer;
pub use velocity_tracker::VelocityTracker;
pub use vertical_drag::VerticalDragRecognizer;

/// Movement threshold (in logical pixels) beyond which a tap is rejected.
/// Matches Flutter's `kTouchSlop`.
pub(crate) const TAP_SLOP: f32 = 18.0;

/// Cumulative vertical movement threshold beyond which a vertical drag is
/// recognized. Matches Flutter's vertical drag slop.
pub(crate) const VERTICAL_DRAG_SLOP: f32 = 18.0;

/// Duration the pointer must remain pressed (without exceeding slop)
/// before a long-press is recognized. Matches iOS
/// `UILongPressGestureRecognizer`'s default `minimumPressDuration`.
pub(crate) const LONG_PRESS_DURATION: std::time::Duration =
    std::time::Duration::from_millis(500);

/// Movement threshold (in logical pixels) beyond which a long-press is
/// rejected. Same value as TAP_SLOP and VERTICAL_DRAG_SLOP — one slop
/// for all three keeps the feel consistent and avoids surprising
/// "I moved 17px and got a long-press instead of a scroll" edge cases.
pub(crate) const LONG_PRESS_SLOP: f32 = 18.0;
