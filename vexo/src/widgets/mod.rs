use crate::renderer::UiBatcher;
use crate::core::{Logical, Physical, Point, Scale, WidgetId};
use crate::state::WidgetStateRegistry;
use crate::input::InputEvent;
use crate::layout::Layout;
use glyphon::FontSystem;
use taffy::prelude::NodeId;

pub trait Widget<M: Clone + std::fmt::Debug + Send> {
    /// Optional stable key for identity across reorders.
    /// Widgets that need focus tracking must have a unique key.
    fn key(&self) -> Option<&str> {
        None
    }

    /// Return layout properties for this widget.
    /// Default implementation returns empty Layout.
    fn layout_props(&self) -> Layout {
        Layout::default()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId;

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: taffy::NodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>, // Current focused widget (if have one), // Pass focus here for drawing. (eg: draw a blue border when focused)
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    );

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: taffy::NodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>, // Current focused widget (if have one)
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M>;
}

// ============================================================================
// Box<dyn Widget<M>> Support
// ============================================================================

/// Enables trait objects: Box<dyn Widget<M>> implements Widget<M>.
/// This allows widgets to be stored in collections and returned from functions.
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Box<dyn Widget<M>> {
    fn key(&self) -> Option<&str> {
        (**self).key()
    }

    fn layout_props(&self) -> Layout {
        (**self).layout_props()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        (**self).layout(taffy, ctx)
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: taffy::NodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        (**self).draw(taffy, node, renderer, offset, focused_id, cursor_blink, ctx)
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: taffy::NodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        (**self).on_event(taffy, node, offset, event, focused_id, ctx)
    }
}

pub struct WidgetResponse<M> {
    /// The user-defined message
    pub message: Option<M>,

    /// If Some(id), this widget want to grab the keyboard focus.
    pub focus_request: Option<WidgetId>,

    /// Did the widget consume this event? (Stops propagation)
    pub handled: bool,

    /// Should the framework clear focus from the currently focused widget?
    /// Used by non-focusable widgets (like Button) to clear focus when clicked.
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

pub struct WidgetContext {
    /// State registry for editors and focus management.
    state: WidgetStateRegistry,
    pub font_system: FontSystem,
    pub scale: Scale,
    pub cursor_pos: Point<Physical>,
}

impl WidgetContext {
    pub fn new() -> Self {
        // Embed a font so we are guaranteed to have one available.
        // Eg: we can't get the system font on ios platform
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(alloc::sync::Arc::new(font_data));
        let font_system = FontSystem::new_with_fonts([binary]);

        Self {
            state: WidgetStateRegistry::new(),
            font_system,
            scale: Scale::new(1.0),
            cursor_pos: Point::new(0.0, 0.0),
        }
    }

    /// Get or create an editor by ID.
    ///
    /// Delegates to the internal WidgetStateRegistry.
    pub fn get_or_create_editor(&mut self, id: &str, initial_text: &str) -> EditorRef {
        self.state.get_or_create_editor(id, initial_text, &mut self.font_system)
    }

    /// Get access to the state registry.
    pub fn state(&self) -> &WidgetStateRegistry {
        &self.state
    }

    /// Get mutable access to the state registry.
    pub fn state_mut(&mut self) -> &mut WidgetStateRegistry {
        &mut self.state
    }
}

type EditorRef = crate::state::EditorRef;

mod button;
mod color_widget;
mod containers;
mod grid;
mod modifiers;
mod text;
mod text_edit;

pub use button::Button;
pub use color_widget::ColorWidget;
pub use containers::Column;
pub use containers::Row;
pub use grid::Grid;
pub use modifiers::Background;
pub use modifiers::Border;
pub use modifiers::CornerRadius;
pub use modifiers::WidgetExt;
pub use text::Text;
pub use text_edit::TextEdit;
