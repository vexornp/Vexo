use crate::editor;
use crate::renderer::UiBatcher;
use crate::utils::Physical;
use glyphon::{Attrs, Buffer, Edit, Editor, FontSystem, Metrics, Shaping};
use std::collections::HashMap;
use std::rc::Rc;
use taffy::prelude::NodeId;

pub trait Widget<M: Clone + std::fmt::Debug + Send> {
    /// Optional stable key for identity across reorders.
    /// Widgets that need focus tracking must have a unique key.
    fn key(&self) -> Option<&str> {
        None
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId;

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: taffy::NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>, // Current focused widget (if have one), // Pass focus here for drawing. (eg: draw a blue border when focused)
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    );

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: taffy::NodeId,
        offset: crate::utils::Point<crate::utils::Logical>,
        event: &winit::event::WindowEvent,
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

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        (**self).layout(taffy, ctx)
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: taffy::NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
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
        offset: crate::utils::Point<crate::utils::Logical>,
        event: &winit::event::WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        (**self).on_event(taffy, node, offset, event, focused_id, ctx)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

impl WidgetId {
    /// Create a WidgetId deterministically from a stable `key` string.
    pub fn from_key(key: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut s);
        WidgetId(s.finish())
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
    pub editors: HashMap<String, EditorRef>,
    pub font_system: FontSystem,
    pub scale: crate::utils::Scale,
    pub cursor_pos: crate::utils::Point<Physical>,
}

impl WidgetContext {
    pub fn new() -> Self {
        // Embed a font so we are guaranteed to have one available.
        // Eg: we can't get the system font on ios platform
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(alloc::sync::Arc::new(font_data));
        let font_system = FontSystem::new_with_fonts([binary]);

        Self {
            editors: HashMap::new(),
            font_system,
            scale: crate::utils::Scale::new(1.0),
            cursor_pos: crate::utils::Point::new(0.0, 0.0),
        }
    }

    pub fn get_or_create_editor(&mut self, id: &str, initial_text: &str) -> EditorRef {
        self.editors
            .entry(id.to_string())
            .or_insert_with(|| {
                let font_size = 16.0;
                let metrics = Metrics::new(font_size, font_size * 1.25);
                let mut editor = Editor::new(Buffer::new_empty(metrics));
                editor.with_buffer_mut(|buffer| {
                    buffer.set_text(
                        &mut self.font_system,
                        initial_text,
                        &Attrs::new(),
                        Shaping::Advanced,
                    );
                });
                editor.shape_as_needed(&mut self.font_system, true);
                Rc::new(std::cell::RefCell::new(editor::Editor::new(editor)))
            })
            .clone()
    }
}

type EditorRef = std::rc::Rc<std::cell::RefCell<editor::Editor>>;

mod button;
mod color_widget;
mod containers;
mod modifiers;
mod text;
mod text_edit;

pub use button::Button;
pub use color_widget::ColorWidget;
pub use containers::Column;
pub use containers::Row;
pub use modifiers::Background;
pub use modifiers::Border;
pub use modifiers::CornerRadius;
pub use modifiers::Frame;
pub use modifiers::FrameSize;
pub use modifiers::Padding;
pub use modifiers::WidgetExt;
pub use text::Text;
pub use text_edit::TextEdit;
