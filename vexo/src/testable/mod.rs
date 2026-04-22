//! Testable traits for the Vexo UI framework.
//!
//! This module provides separated traits for widget concerns, enabling
//! unit testing of widgets without GPU or window. Each trait handles
//! a specific aspect of widget behavior:
//!
//! - `Identifiable` - Widget identity for focus/hover tracking
//! - `Layout` - Layout constraints and receiving computed layout
//! - `Paint` - Generating render commands
//! - `Interact` - Handling input events
//!
//! These traits allow testing each concern in isolation with pure data
//! structures, no rendering backend required.

mod identifiable;
mod layout;
mod paint;
mod interact;

pub use identifiable::Identifiable;
pub use layout::{Layout, LayoutConstraints, ComputedLayout};
pub use paint::{Paint, PaintContext};
pub use interact::{Interact, InteractionContext, InteractionResponse};

use crate::core::WidgetId;

// ============================================================================
// COMPOSITE WIDGET TRAIT
// ============================================================================

/// Composite trait for full widgets that implement all behaviors.
///
/// A widget that implements all four traits (Identifiable, Layout, Paint, Interact)
/// automatically implements this trait via blanket implementation.
pub trait Widget<M: Clone + std::fmt::Debug + Send>:
    Identifiable + Layout + Paint + Interact<M>
{
}

/// Blanket implementation: any type implementing all four traits is a Widget.
impl<M: Clone + std::fmt::Debug + Send, T: Identifiable + Layout + Paint + Interact<M>> Widget<M> for T {}

// ============================================================================
// WIDGET RESPONSE (Compatibility with existing code)
// ============================================================================

/// Response from widget event handling.
///
/// This is kept for compatibility with the existing widget system.
#[derive(Debug)]
pub struct WidgetResponse<M> {
    /// User-defined message to emit.
    pub message: Option<M>,
    /// Request to change focus.
    pub focus_request: Option<WidgetId>,
    /// Whether the event was consumed.
    pub handled: bool,
    /// Whether to clear focus from the currently focused widget.
    pub clear_focus: bool,
}

impl<M> Default for WidgetResponse<M> {
    fn default() -> Self {
        Self {
            message: None,
            focus_request: None,
            handled: false,
            clear_focus: false,
        }
    }
}

impl<M> WidgetResponse<M> {
    /// Create a response indicating the event was handled.
    pub fn handled() -> Self {
        Self {
            handled: true,
            ..Self::default()
        }
    }

    /// Create a response with a message.
    pub fn with_message(message: M) -> Self {
        Self {
            message: Some(message),
            handled: true,
            ..Self::default()
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::RenderCommand;

    /// Test widget that implements all traits.
    struct TestWidget {
        id: WidgetId,
        computed_layout: Option<ComputedLayout>,
    }

    impl TestWidget {
        fn new(id: &str) -> Self {
            Self {
                id: WidgetId::from_key(id),
                computed_layout: None,
            }
        }
    }

    impl Identifiable for TestWidget {
        fn id(&self) -> Option<WidgetId> {
            Some(self.id)
        }
    }

    impl Layout for TestWidget {
        fn constraints(&self) -> LayoutConstraints {
            LayoutConstraints::fixed(100.0, 50.0)
        }

        fn apply_layout(&mut self, layout: ComputedLayout) {
            self.computed_layout = Some(layout);
        }
    }

    impl Paint for TestWidget {
        fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
            let layout = self.computed_layout.unwrap_or_default();
            vec![RenderCommand::rect(layout.bounds, crate::core::Color::RED)]
        }
    }

    impl<M: Clone + std::fmt::Debug + Send> Interact<M> for TestWidget {
        fn on_event(
            &mut self,
            _event: &crate::input::InputEvent,
            _ctx: &InteractionContext,
        ) -> InteractionResponse<M> {
            InteractionResponse::default()
        }
    }

    #[test]
    fn test_widget_trait_composition() {
        let widget = TestWidget::new("test");

        // Should implement Identifiable
        let id = widget.id();
        assert!(id.is_some());

        // Should implement Layout
        let constraints = widget.constraints();
        assert!(constraints.is_fixed_width());

        // Should be a Widget via blanket impl
        fn assert_widget<M: Clone + std::fmt::Debug + Send, W: Widget<M>>(_: &W) {}
        assert_widget::<(), _>(&widget);
    }

    #[test]
    fn test_widget_response() {
        let r: WidgetResponse<()> = WidgetResponse::default();
        assert!(!r.handled);

        let r = WidgetResponse::<()>::handled();
        assert!(r.handled);

        let r = WidgetResponse::with_message("test");
        assert_eq!(r.message, Some("test"));
        assert!(r.handled);
    }
}
