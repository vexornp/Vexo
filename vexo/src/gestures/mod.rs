//! Gesture arena: disambiguates competing gesture recognizers for a pointer.
//!
//! Currently implements Tap vs. VerticalDrag (scroll) disambiguation via a
//! slop-threshold rule, matching Flutter's GestureArena behavior for this
//! recognizer pair.

pub mod arena;
pub mod arena_event;
pub mod recognizer;
pub mod tap;
pub mod vertical_drag;

pub use arena::{ArenaOutcome, GestureArena};
pub use arena_event::ArenaEvent;
pub use recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
pub use tap::TapRecognizer;
pub use vertical_drag::VerticalDragRecognizer;

/// Movement threshold (in logical pixels) beyond which a tap is rejected.
/// Matches Flutter's `kTouchSlop`.
pub(crate) const TAP_SLOP: f32 = 18.0;

/// Cumulative vertical movement threshold beyond which a vertical drag is
/// recognized. Matches Flutter's vertical drag slop.
pub(crate) const VERTICAL_DRAG_SLOP: f32 = 18.0;
