use crate::editor;
use crate::renderer::UiBatcher;
use glyphon::{Attrs, Buffer, Edit, Editor, FontSystem, Metrics, Shaping};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use taffy::prelude::NodeId;
use winit::window::Window;

pub trait Widget<M: Clone + std::fmt::Debug + Send> {
    /// Widget unique ID. (Used for focus tracking)
    /// Default implementation returns `WidgetId(0)`; prefer using the
    /// NodeId->WidgetId mapping stored in `WidgetContext` instead.
    fn id(&self) -> WidgetId {
        if let Some(k) = self.key() {
            return WidgetId::from_key(k);
        }
        WidgetId(0)
    }
    /// Optional stable key for identity across reorders.
    fn key(&self) -> Option<&str> {
        None
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId;

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: taffy::NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        focused_id: Option<WidgetId>, // Current focused widget (if have one), // Pass focus here for drawing. (eg: draw a blue border when focused)
        ctx: &mut WidgetContext,
    );

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: taffy::NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>, // Current focused widget (if have one)
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

impl WidgetId {
    /// Create a WidgetId deterministically from a stable `key` string.
    ///
    /// This uses the default std hasher to produce a 64-bit value.
    /// Prefer using the framework-provided path-mixing (via `WidgetContext`) when
    /// deriving ids from traversal paths, but this helper is convenient when
    /// you only have a developer-provided key and want a `WidgetId`.
    pub fn from_key(key: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut s);
        WidgetId(s.finish())
    }

    /// Mix this id with a child index to derive a child WidgetId along the
    /// deterministic path (same mixing constant used in `WidgetContext`).
    pub fn mix_with_index(&self, idx: usize) -> Self {
        WidgetId(
            self.0
                .wrapping_mul(0x9E3779B97F4A7C15u64)
                .wrapping_add(idx as u64 + 1),
        )
    }

    /// Mix this id with a key string to derive a child WidgetId along the
    /// deterministic path (same mixing constant used in `WidgetContext`).
    pub fn mix_with_key(&self, key: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut s);
        let key_hash = s.finish();
        WidgetId(
            self.0
                .wrapping_mul(0x9E3779B97F4A7C15u64)
                .wrapping_add(key_hash),
        )
    }
}

pub struct WidgetResponse<M> {
    /// The user-defined message
    pub message: Option<M>,

    /// If Some(id), this widget want to grab the keyboard focus.
    pub focus_request: Option<WidgetId>,

    /// Did the widget consume this event? (Stops propagation)
    pub handled: bool,
}

impl<M> Default for WidgetResponse<M> {
    fn default() -> Self {
        Self {
            message: None,
            focus_request: None,
            handled: false,
        }
    }
}

pub struct WidgetContext {
    pub editors: HashMap<String, EditorRef>,
    // id stack for deterministic widget id generation
    pub id_stack: Vec<u64>,
    // Mapping from layout NodeId -> computed WidgetId for this frame
    pub node_to_widget: HashMap<NodeId, WidgetId>,

    pub font_system: FontSystem,

    pub window: Option<Arc<Window>>,
}

impl WidgetContext {
    pub fn new(window: Option<Arc<Window>>) -> Self {
        // Embed a font so we are guaranteed to have one available.
        // Eg: we can't get the system font on ios platform
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(alloc::sync::Arc::new(font_data));
        // font_system.db_mut().load_font_data(font_data);
        let font_system = FontSystem::new_with_fonts([binary]);

        Self {
            editors: HashMap::new(),
            id_stack: vec![0x9E3779B97F4A7C15u64],
            node_to_widget: HashMap::new(),
            font_system,
            window,
        }
    }

    pub fn reset_id_stack(&mut self) {
        self.id_stack.clear();
        self.id_stack.push(0x9E3779B97F4A7C15u64);
        // clear per-frame node->widget mapping
        self.node_to_widget.clear();
    }

    pub fn push_index(&mut self, idx: usize) {
        let parent = *self.id_stack.last().unwrap();
        let child = parent
            .wrapping_mul(0x9E3779B97F4A7C15u64)
            .wrapping_add(idx as u64 + 1);
        self.id_stack.push(child);
    }

    pub fn push_key(&mut self, key: &str) {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut s);
        let key_hash = s.finish();
        let parent = *self.id_stack.last().unwrap();
        let child = parent
            .wrapping_mul(0x9E3779B97F4A7C15u64)
            .wrapping_add(key_hash);
        self.id_stack.push(child);
    }

    pub fn pop(&mut self) {
        if self.id_stack.len() > 1 {
            self.id_stack.pop();
        }
    }

    pub fn current_widget_id(&self) -> WidgetId {
        WidgetId(*self.id_stack.last().unwrap())
    }

    pub fn record_node_widget(&mut self, node: NodeId) {
        let wid = self.current_widget_id();
        self.node_to_widget.insert(node, wid);
    }

    pub fn get_widget_id(&self, node: NodeId) -> Option<WidgetId> {
        self.node_to_widget.get(&node).copied()
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
mod containers;
mod rectangle;
mod text;

pub use button::Button;
pub use containers::Column;
pub use containers::Row;
pub use rectangle::Rectangle;
pub use text::Text;
pub use text::TextEdit;
