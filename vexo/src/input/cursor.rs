use std::cell::RefCell;
use std::rc::Rc;

/// System-provided cursor kinds that map to platform cursor icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemCursorKind {
    Arrow,
    Pointer,
    Text,
    Crosshair,
    Move,
    NotAllowed,
    ResizeHorizontal,
    ResizeVertical,
}

impl SystemCursorKind {
    pub fn to_winit(self) -> winit::cursor::CursorIcon {
        match self {
            Self::Arrow => winit::cursor::CursorIcon::Default,
            Self::Pointer => winit::cursor::CursorIcon::Pointer,
            Self::Text => winit::cursor::CursorIcon::Text,
            Self::Crosshair => winit::cursor::CursorIcon::Crosshair,
            Self::Move => winit::cursor::CursorIcon::Move,
            Self::NotAllowed => winit::cursor::CursorIcon::NotAllowed,
            Self::ResizeHorizontal => winit::cursor::CursorIcon::EwResize,
            Self::ResizeVertical => winit::cursor::CursorIcon::NsResize,
        }
    }
}

/// Three-state cursor intent, matching Flutter's MouseCursor design.
///
/// - `Defer`: This region has no opinion; fall through to the next region behind it.
/// - `System`: Request a specific system cursor (e.g., I-beam for text).
/// - `Uncontrolled`: Block cursor changes from regions behind this one, but don't change the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MouseCursor {
    #[default]
    Defer,
    System(SystemCursorKind),
    Uncontrolled,
}

/// Annotation carried by MouseRegion render objects, collected during hit test
/// to resolve the active cursor and dispatch hover events.
pub struct MouseTrackerAnnotation {
    pub cursor: MouseCursor,
    pub on_enter: Option<Rc<RefCell<dyn FnMut()>>>,
    pub on_exit: Option<Rc<RefCell<dyn FnMut()>>>,
    pub opaque: bool,
}

impl std::fmt::Debug for MouseTrackerAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MouseTrackerAnnotation")
            .field("cursor", &self.cursor)
            .field("on_enter", &self.on_enter.is_some())
            .field("on_exit", &self.on_exit.is_some())
            .field("opaque", &self.opaque)
            .finish()
    }
}

impl MouseTrackerAnnotation {
    pub fn new(cursor: MouseCursor) -> Self {
        Self { cursor, on_enter: None, on_exit: None, opaque: true }
    }

    pub fn with_on_enter(mut self, callback: Rc<RefCell<dyn FnMut()>>) -> Self {
        self.on_enter = Some(callback);
        self
    }

    pub fn with_on_exit(mut self, callback: Rc<RefCell<dyn FnMut()>>) -> Self {
        self.on_exit = Some(callback);
        self
    }

    pub fn with_opaque(mut self, opaque: bool) -> Self {
        self.opaque = opaque;
        self
    }
}

impl Clone for MouseTrackerAnnotation {
    fn clone(&self) -> Self {
        Self {
            cursor: self.cursor,
            on_enter: self.on_enter.clone(),
            on_exit: self.on_exit.clone(),
            opaque: self.opaque,
        }
    }
}
