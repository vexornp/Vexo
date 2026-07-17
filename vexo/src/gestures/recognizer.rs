//! GestureRecognizer trait and supporting types.

use std::any::Any;

use crate::core::{Logical, Point};

use super::arena_event::ArenaEvent;

/// Outcome of a recognizer's internal state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecognizerResolution {
    /// Still competing — no decision yet.
    Pending,
    /// This recognizer has claimed the gesture (won).
    Accepted,
    /// This recognizer has given up (lost or cancelled).
    Rejected,
}

/// Shared facts computed once by the arena and handed to each recognizer.
///
/// Recognizers track their own accumulated state (e.g. `total_delta_y`)
/// internally; this struct only carries the per-event shared facts.
#[derive(Clone, Copy, Debug)]
pub struct ArenaContext {
    pub down_position: Point<Logical>,
    pub current_position: Point<Logical>,
}

/// A self-contained gesture state machine.
///
/// Recognizers never call arena methods and never hold user callbacks.
/// The arena reads `resolution()` to decide a winner; the owning element
/// holds the callback and fires it when the arena resolves.
pub trait GestureRecognizer: Any {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext);
    fn resolution(&self) -> RecognizerResolution;

    /// Downcast this `&dyn GestureRecognizer` to a concrete `&dyn Any`.
    ///
    /// Required because `Any` is a supertrait but not directly accessible
    /// through a trait object — `&dyn GestureRecognizer` cannot be downcast
    /// without an explicit `as_any()` method. Implementations just return
    /// `self`.
    fn as_any(&self) -> &dyn Any;

    fn accepted(&self) -> bool {
        matches!(self.resolution(), RecognizerResolution::Accepted)
    }
    fn rejected(&self) -> bool {
        matches!(self.resolution(), RecognizerResolution::Rejected)
    }
}
