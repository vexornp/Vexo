use crate::renderer::UiBatcher;
use crate::core::{Logical, Physical, Point, Scale, WidgetId};
use crate::state::WidgetStateRegistry;
use crate::input::{CursorIcon, InputEvent};
use crate::layout::{Layout, LayoutContext, LayoutNodeKey, LayoutView};
use crate::render::RenderCommand;
use glyphon::FontSystem;

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

    /// Return the cursor to show when hovering over this widget.
    /// Default implementation returns Default (arrow cursor).
    fn cursor(&self) -> CursorIcon {
        CursorIcon::Default
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeKey;

    /// Receive computed layout after layout computation.
    ///
    /// This method is called by the rendering pipeline after layout computation
    /// so widgets can store their computed bounds for use during painting.
    fn apply_layout(&mut self, _layout: crate::testable::ComputedLayout) {
        // Default: no-op. Widgets that need layout should override this.
    }

    /// Paint this widget using the new Paint trait.
    ///
    /// Returns render commands that will be processed by the rendering pipeline.
    /// This is the new painting method that replaces `draw()`.
    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand>;

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeKey,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>, // Current focused widget (if have one), // Pass focus here for drawing. (eg: draw a blue border when focused)
        cursor_blink: &crate::CursorBlinkState,
        widget_context: &mut WidgetContext,
    );

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeKey,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>, // Current focused widget (if have one)
        widget_context: &mut WidgetContext,
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

    fn cursor(&self) -> CursorIcon {
        (**self).cursor()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeKey {
        (**self).layout(layout_context, widget_context)
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        (**self).apply_layout(layout)
    }

    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        (**self).paint(ctx)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeKey,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        widget_context: &mut WidgetContext,
    ) {
        (**self).draw(layout_view, node, renderer, offset, focused_id, cursor_blink, widget_context)
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeKey,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_context: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        (**self).on_event(layout_view, node, offset, event, focused_id, widget_context)
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

    /// Request to change the mouse cursor when hovering over this widget.
    /// None means "no opinion" - use parent's cursor or default.
    pub cursor: Option<CursorIcon>,
}

impl<M> Default for WidgetResponse<M> {
    fn default() -> Self {
        Self {
            message: None,
            focus_request: None,
            handled: false,
            clear_focus: false,
            cursor: None,
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

impl Default for WidgetContext {
    fn default() -> Self {
        Self::new()
    }
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

    /// Create a ComponentContext for use with components.
    ///
    /// This method handles the internal borrow splitting required to provide
    /// mutable access to both the component storage and font system.
    pub fn create_component_context<'a, M: Clone + std::fmt::Debug + Send>(
        &'a mut self,
        key_path: crate::component::KeyPath,
    ) -> crate::component::ComponentContext<'a, M> {
        crate::component::ComponentContext::new(
            key_path,
            self.state.component_storage(),
            &mut self.font_system,
            self.scale,
        )
    }
}

type EditorRef = crate::state::EditorRef;

/// Helper function for container widgets to propagate PointerMoved events.
///
/// This function hit-tests children and propagates the PointerMoved event to the
/// child that contains the pointer position. It returns that child's WidgetResponse,
/// which includes the cursor to display.
///
/// This is used by Row, Column, and other container widgets to deduplicate
/// PointerMoved handling logic.
pub(crate) fn propagate_pointer_moved_to_containing_child<M: Clone + std::fmt::Debug + Send>(
    children: &mut [Box<dyn Widget<M>>],
    child_ids: &[LayoutNodeKey],
    layout_view: &LayoutView,
    offset: Point<Logical>,
    event: &InputEvent,
    focused_id: Option<WidgetId>,
    widget_context: &mut WidgetContext,
) -> WidgetResponse<M> {
    let position = match event {
        InputEvent::PointerMoved { position } => *position,
        _ => return WidgetResponse::default(),
    };

    for (child, child_node_id) in children.iter_mut().zip(child_ids.iter()) {
        if let Some(child_layout) = layout_view.get_layout(*child_node_id) {
            let child_bounds = crate::core::Bounds::from_xywh(
                offset.x + child_layout.x(),
                offset.y + child_layout.y(),
                child_layout.width(),
                child_layout.height(),
            );
            if child_bounds.contains(&position) {
                return child.on_event(
                    layout_view,
                    *child_node_id,
                    offset,
                    event,
                    focused_id,
                    widget_context,
                );
            }
        }
    }
    WidgetResponse::default()
}

mod button;
mod color_widget;
mod column;
mod grid;
mod map_widget;
mod modifiers;
mod row;
mod scroll_view;
mod text;
mod text_edit;

pub use button::Button;
pub use color_widget::ColorWidget;
pub use column::Column;
pub use grid::Grid;
pub use map_widget::MapWidget;
pub use modifiers::Background;
pub use modifiers::Border;
pub use modifiers::CornerRadius;
pub use modifiers::WidgetExt;
pub use row::Row;
pub use scroll_view::{ScrollView, ScrollState};
pub use text::Text;
pub use text_edit::TextEdit;
