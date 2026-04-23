# Cursor Icon System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a general cursor icon system that allows widgets to request cursor changes when the pointer hovers over them.

**Architecture:** Add CursorIcon enum to input module, extend WidgetResponse and InteractionResponse with cursor field, track current cursor in WindowState, and update cursor via winit when it changes.

**Tech Stack:** Rust, winit for window/cursor control

---

## Task 1: Add CursorIcon Enum

**Files:**
- Modify: `vexo/src/input/event.rs`

- [ ] **Step 1: Add CursorIcon enum after Modifiers struct (around line 219)**

```rust
// ============================================================================
// CURSOR ICON
// ============================================================================

/// Mouse cursor icon types.
///
/// These mirror winit's cursor types but live in the platform-agnostic
/// input module for use in widget event handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorIcon {
    /// Default arrow cursor.
    #[default]
    Default,
    /// Hand pointer (for clickable elements like buttons).
    Pointer,
    /// I-beam text cursor (for text input).
    Text,
    /// Crosshair for precision selection.
    Crosshair,
    /// Move cursor for drag operations.
    Move,
    /// Not-allowed cursor for disabled actions.
    NotAllowed,
    /// Horizontal resize cursor.
    ResizeHorizontal,
    /// Vertical resize cursor.
    ResizeVertical,
}
```

- [ ] **Step 2: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add vexo/src/input/event.rs
git commit -m "feat(input): add CursorIcon enum for cursor types"
```

---

## Task 2: Add Cursor Field to WidgetResponse

**Files:**
- Modify: `vexo/src/widgets/mod.rs`

- [ ] **Step 1: Import CursorIcon at top of file (line 4)**

```rust
use crate::input::{CursorIcon, InputEvent};
```

- [ ] **Step 2: Add cursor field to WidgetResponse struct (lines 113-126)**

Replace the struct with:

```rust
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
```

- [ ] **Step 3: Update Default impl for WidgetResponse (lines 128-137)**

```rust
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
```

- [ ] **Step 4: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/mod.rs
git commit -m "feat(widgets): add cursor field to WidgetResponse"
```

---

## Task 3: Add Cursor Field to InteractionResponse

**Files:**
- Modify: `vexo/src/testable/interact.rs`

- [ ] **Step 1: Import CursorIcon (line 4)**

```rust
use crate::input::{CursorIcon, InputEvent, Modifiers};
```

- [ ] **Step 2: Add cursor field to InteractionResponse struct (lines 72-82)**

Replace the struct with:

```rust
/// Response from widget event handling.
#[derive(Debug)]
pub struct InteractionResponse<M> {
    /// User-defined message to emit.
    pub message: Option<M>,
    /// Request to change focus.
    pub focus_request: Option<FocusRequest>,
    /// Whether the event was consumed.
    pub handled: bool,
    /// Whether to clear focus from the currently focused widget.
    pub clear_focus: bool,
    /// Request to change the mouse cursor.
    pub cursor: Option<CursorIcon>,
}
```

- [ ] **Step 3: Update Default impl for InteractionResponse (lines 93-102)**

```rust
impl<M> Default for InteractionResponse<M> {
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
```

- [ ] **Step 4: Update all InteractionResponse constructor methods (lines 104-144)**

```rust
impl<M> InteractionResponse<M> {
    /// Create a response indicating the event was not handled.
    pub fn ignored() -> Self {
        Self::default()
    }

    /// Create a response indicating the event was handled.
    pub fn handled() -> Self {
        Self {
            handled: true,
            ..Self::default()
        }
    }

    /// Create a response with a user message.
    pub fn with_message(message: M) -> Self {
        Self {
            message: Some(message),
            handled: true,
            ..Self::default()
        }
    }

    /// Create a response requesting focus.
    pub fn request_focus(id: WidgetId) -> Self {
        Self {
            focus_request: Some(FocusRequest::Gain(id)),
            handled: true,
            ..Self::default()
        }
    }

    /// Create a response requesting focus to be cleared.
    pub fn clear_focus() -> Self {
        Self {
            focus_request: Some(FocusRequest::Clear),
            handled: true,
            ..Self::default()
        }
    }
}
```

- [ ] **Step 5: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 6: Commit**

```bash
git add vexo/src/testable/interact.rs
git commit -m "feat(testable): add cursor field to InteractionResponse"
```

---

## Task 4: Add Cursor State and Resolution to WindowState

**Files:**
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Import CursorIcon (around line 41)**

Add to the existing use statement:

```rust
use crate::input::{CursorIcon, InputEvent};
```

- [ ] **Step 2: Add current_cursor field to WindowState struct (lines 48-68)**

Add after `cursor_blink` field:

```rust
pub struct WindowState<A: Application + 'static> {
    // ... existing fields ...

    // Cursor blink state (global - only one focused widget at a time)
    cursor_blink: CursorBlinkState,

    // Current cursor icon (for detecting changes)
    current_cursor: CursorIcon,
}
```

- [ ] **Step 3: Initialize current_cursor in WindowState::new (lines 119-143)**

Add to the initialization:

```rust
Ok(Self {
    backend,
    window: Some(window),
    batcher: UiBatcher::new(),
    layout_engine,
    root_widget,
    root_node_id,
    user_app_state: A::new(),
    _phantom: std::marker::PhantomData,
    focused_widget_id: None,
    widget_context: ctx,
    cursor_blink: CursorBlinkState::new(),
    current_cursor: CursorIcon::default(),
})
```

- [ ] **Step 4: Add winit cursor mapping function after CursorBlinkState impl (around line 117)**

```rust
/// Convert CursorIcon to winit's Cursor type.
fn winit_cursor_from_icon(icon: CursorIcon) -> winit::window::Cursor {
    use winit::window::Cursor;
    match icon {
        CursorIcon::Default => Cursor::Default,
        CursorIcon::Pointer => Cursor::Pointer,
        CursorIcon::Text => Cursor::Text,
        CursorIcon::Crosshair => Cursor::Crosshair,
        CursorIcon::Move => Cursor::Move,
        CursorIcon::NotAllowed => Cursor::NotAllowed,
        CursorIcon::ResizeHorizontal => Cursor::ResizeHorizontal,
        CursorIcon::ResizeVertical => Cursor::ResizeVertical,
    }
}
```

- [ ] **Step 5: Add cursor resolution in handle_window_event (lines 376-435)**

After the existing widget_response handling, add cursor resolution. Replace the entire `handle_window_event` method with:

```rust
fn handle_window_event(
    &mut self,
    _event_loop: &dyn ActiveEventLoop,
    _window_id: winit::window::WindowId,
    event: &winit::event::WindowEvent,
) {
    // Convert winit event to InputEvent
    let input_event = crate::input::InputEvent::from_winit(
        event,
        self.widget_context.scale.clone(),
    );

    // Only process events that convert to InputEvent
    let Some(input_event) = input_event else {
        return;
    };

    // Pass the event to the root widget (which passes it down)
    let layout_view = LayoutView::new(self.layout_engine.as_ref());
    let widget_response = self.root_widget.on_event(
        &layout_view,
        self.root_node_id,
        Point::new(0.0, 0.0),
        &input_event,
        self.focused_widget_id,
        &mut self.widget_context,
    );

    // Handle cursor changes
    if let Some(cursor) = widget_response.cursor {
        if cursor != self.current_cursor {
            self.current_cursor = cursor;
            if let Some(window) = &self.window {
                window.set_cursor(winit_cursor_from_icon(cursor));
            }
        }
    } else if self.current_cursor != CursorIcon::Default {
        // No cursor requested - reset to default
        self.current_cursor = CursorIcon::Default;
        if let Some(window) = &self.window {
            window.set_cursor(winit_cursor_from_icon(CursorIcon::Default));
        }
    }

    // Handle Framework Logic
    if let Some(focus_request) = widget_response.focus_request {
        self.focused_widget_id = Some(focus_request);
        println!("Focus requested by widget: {:?}", focus_request);
    } else if widget_response.clear_focus {
        self.focused_widget_id = None;
    } else if !widget_response.handled {
        if let crate::input::InputEvent::PointerButton {
            state: crate::input::ButtonState::Pressed,
            ..
        } = input_event
        {
            // Click outside any focusable widget - clear focus
            self.focused_widget_id = None;
        }
    }

    // Check if event if handled, notify if needed
    if widget_response.handled {
        println!("Event handled by widget");
        // Reset cursor blink on keyboard input
        if let crate::input::InputEvent::Keyboard { .. } = input_event {
            self.cursor_blink.reset();
        }
    }

    //  Handle User Logic
    if let Some(msg) = widget_response.message {
        println!("User message received: {:?}", msg);
        self.update(msg);
    }
}
```

- [ ] **Step 6: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 7: Commit**

```bash
git add vexo/src/lib.rs
git commit -m "feat: add cursor state tracking and resolution to WindowState"
```

---

## Task 5: Update TextEdit Widget to Request Text Cursor

**Files:**
- Modify: `vexo/src/widgets/text_edit.rs`

- [ ] **Step 1: Import CursorIcon (line 7)**

```rust
use crate::input::{CursorIcon, InputEvent, ButtonState, Key, NamedKey};
```

- [ ] **Step 2: Add cursor request on PointerMoved event in on_event (around line 277)**

Find the `on_event` method and add cursor handling. The method needs to return a cursor when pointer is inside bounds. Replace the `on_event` method with:

```rust
fn on_event(
    &mut self,
    layout_view: &LayoutView,
    node: LayoutNodeId,
    offset: Point<Logical>,
    event: &InputEvent,
    focused_id: Option<WidgetId>,
    widget_context: &mut WidgetContext,
) -> WidgetResponse<M> {
    // Derive our WidgetId from the editor_id (explicit key)
    let my_id = WidgetId::from_key(&self.editor_id);
    let is_focused = focused_id == Some(my_id);

    // Get our bounds for cursor and click detection
    let bounds_check = |layout_view: &LayoutView, position: Point<Logical>| -> bool {
        if let Some(layout) = layout_view.get_layout(node) {
            let abs_x = offset.x + layout.x();
            let abs_y = offset.y + layout.y();
            let rect = Rect::from_xywh(abs_x, abs_y, layout.width(), layout.height());
            rect.contains(position)
        } else {
            false
        }
    };

    // Handle PointerMoved - request text cursor when hovering
    if let InputEvent::PointerMoved { position } = event {
        if bounds_check(layout_view, *position) {
            return WidgetResponse {
                message: None,
                focus_request: None,
                handled: false,
                clear_focus: false,
                cursor: Some(CursorIcon::Text),
            };
        }
    }

    if !is_focused {
        // Check for click to grab focus
        if let InputEvent::PointerButton {
            state: ButtonState::Pressed,
            position,
            ..
        } = event
        {
            if bounds_check(layout_view, *position) {
                return WidgetResponse {
                    message: None,
                    focus_request: Some(my_id),
                    handled: true,
                    clear_focus: false,
                    cursor: Some(CursorIcon::Text),
                };
            }
        }
        return WidgetResponse::default();
    }

    // We are focused, so handle keyboard input
    let editor_rc = widget_context.get_or_create_editor(&self.editor_id, &self.initial_text);
    let mut editor_ref = editor_rc.borrow_mut();

    match event {
        InputEvent::ModifiersChanged { modifiers } => {
            // Store modifiers for later use if needed
            let _ctrl_pressed = modifiers.control;
        }
        InputEvent::PointerButton {
            state: ButtonState::Pressed,
            position,
            ..
        } => {
            // Check if click is inside our bounds
            if bounds_check(layout_view, *position) {
                // Click inside - retain focus
                return WidgetResponse {
                    message: None,
                    focus_request: None,
                    handled: true,
                    clear_focus: false,
                    cursor: Some(CursorIcon::Text),
                };
            }
        }
        InputEvent::Keyboard { key, state: ButtonState::Pressed, text, modifiers } => {
            // Handle keyboard input
            match key {
                Key::Named(NamedKey::Backspace) => {
                    editor_ref.action(Action::Backspace);
                }
                Key::Named(NamedKey::Delete) => {
                    editor_ref.action(Action::Delete);
                }
                Key::Named(NamedKey::Enter) => {
                    editor_ref.action(Action::Enter);
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    let motion = if modifiers.control {
                        Motion::LeftWord
                    } else {
                        Motion::Left
                    };
                    editor_ref.action(Action::Motion(motion));
                }
                Key::Named(NamedKey::ArrowRight) => {
                    let motion = if modifiers.control {
                        Motion::RightWord
                    } else {
                        Motion::Right
                    };
                    editor_ref.action(Action::Motion(motion));
                }
                Key::Named(NamedKey::ArrowUp) => {
                    editor_ref.action(Action::Motion(Motion::Up));
                }
                Key::Named(NamedKey::ArrowDown) => {
                    editor_ref.action(Action::Motion(Motion::Down));
                }
                Key::Named(NamedKey::Home) => {
                    editor_ref.action(Action::Motion(Motion::Home));
                }
                Key::Named(NamedKey::End) => {
                    editor_ref.action(Action::Motion(Motion::End));
                }
                Key::Character(ch) => {
                    // Handle shortcuts
                    if modifiers.control && ch == "a" {
                        // Select all
                        editor_ref.action(Action::Motion(Motion::Home));
                        // Note: full selection would need more work
                    } else if let Some(text) = text {
                        for c in text.chars() {
                            editor_ref.action(Action::Insert(c));
                        }
                    }
                }
                _ => {}
            }
            return WidgetResponse {
                message: None,
                focus_request: None,
                handled: true,
                clear_focus: false,
                cursor: Some(CursorIcon::Text),
            };
        }
        _ => {}
    }

    WidgetResponse {
        message: None,
        focus_request: None,
        handled: false,
        clear_focus: false,
        cursor: Some(CursorIcon::Text),
    }
}
```

- [ ] **Step 3: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/text_edit.rs
git commit -m "feat(text_edit): request I-beam cursor on hover"
```

---

## Task 6: Update Button Widget to Request Pointer Cursor

**Files:**
- Modify: `vexo/src/widgets/button.rs`

- [ ] **Step 1: Import CursorIcon (line 6)**

```rust
use crate::input::{CursorIcon, InputEvent, ButtonState};
```

- [ ] **Step 2: Update on_event to return cursor on hover (lines 134-179)**

Replace the `on_event` method with:

```rust
fn on_event(
    &mut self,
    layout_view: &LayoutView,
    node: LayoutNodeId,
    offset: Point<Logical>,
    event: &InputEvent,
    focused_id: Option<WidgetId>,
    widget_context: &mut WidgetContext,
) -> WidgetResponse<M> {
    if let Some(layout) = layout_view.get_layout(node) {
        let x = offset.x + layout.x();
        let y = offset.y + layout.y();

        let rect = Rect::<Logical>::from_xywh(x, y, layout.width(), layout.height());

        // Handle pointer moved - request pointer cursor when hovering
        if let InputEvent::PointerMoved { position } = event {
            if rect.contains(position) {
                return WidgetResponse {
                    message: None,
                    focus_request: None,
                    handled: false,
                    clear_focus: false,
                    cursor: Some(CursorIcon::Pointer),
                };
            }
        }

        // Handle pointer button events
        if let InputEvent::PointerButton {
            state: ButtonState::Pressed,
            position,
            ..
        } = event
        {
            if rect.contains(position) {
                return WidgetResponse {
                    message: Some(self.on_press.clone()),
                    focus_request: None,
                    handled: true,
                    clear_focus: true, // Clear focus from other widgets
                    cursor: Some(CursorIcon::Pointer),
                };
            }
        }

        // Child event propagation
        let child_ids = layout_view.children(node);
        if let Some(content_node) = child_ids.get(0) {
            let content_offset = Point::new(x, y);
            return self.content.on_event(
                layout_view,
                *content_node,
                content_offset,
                event,
                focused_id,
                widget_context,
            );
        }
    }

    WidgetResponse::default()
}
```

- [ ] **Step 3: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/button.rs
git commit -m "feat(button): request pointer cursor on hover"
```

---

## Task 7: Update Container Widgets to Propagate Cursor

**Files:**
- Modify: `vexo/src/widgets/column.rs`
- Modify: `vexo/src/widgets/row.rs`

- [ ] **Step 1: Update column.rs on_event to propagate cursor from children (lines 252-278)**

Replace the `on_event` method with:

```rust
fn on_event(
    &mut self,
    layout_view: &LayoutView,
    node: LayoutNodeId,
    offset: Point<Logical>,
    event: &InputEvent,
    focused_id: Option<WidgetId>,
    widget_context: &mut WidgetContext,
) -> WidgetResponse<M> {
    if let Some(layout) = layout_view.get_layout(node) {
        let child_ids = layout_view.children(node);
        let my_offset = Point::new(
            offset.x + layout.x(),
            offset.y + layout.y(),
        );

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response =
                child.on_event(layout_view, child_node_id, my_offset, event, focused_id, widget_context);

            // Propagate handled events, focus requests, or cursor requests
            if child_response.handled || child_response.focus_request.is_some() || child_response.cursor.is_some() {
                return child_response;
            }
        }
    }
    WidgetResponse::default()
}
```

- [ ] **Step 2: Update row.rs on_event to propagate cursor from children (lines 244-270)**

Replace the `on_event` method with:

```rust
fn on_event(
    &mut self,
    layout_view: &LayoutView,
    node: LayoutNodeId,
    offset: Point<Logical>,
    event: &InputEvent,
    focused_id: Option<WidgetId>,
    widget_context: &mut WidgetContext,
) -> WidgetResponse<M> {
    if let Some(layout) = layout_view.get_layout(node) {
        let child_ids = layout_view.children(node);
        let my_offset = Point::new(
            offset.x + layout.x(),
            offset.y + layout.y(),
        );

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response =
                child.on_event(layout_view, child_node_id, my_offset, event, focused_id, widget_context);

            // Propagate handled events, focus requests, or cursor requests
            if child_response.handled || child_response.focus_request.is_some() || child_response.cursor.is_some() {
                return child_response;
            }
        }
    }
    WidgetResponse::default()
}
```

- [ ] **Step 3: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/column.rs vexo/src/widgets/row.rs
git commit -m "feat(containers): propagate cursor requests from children"
```

---

## Task 8: Build and Test

- [ ] **Step 1: Build the entire workspace**

Run: `cargo build`
Expected: Compiles successfully with no errors

- [ ] **Step 2: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Application launches

- [ ] **Step 3: Manual test - hover over text edit**

Move mouse over text edit area. Expected: Cursor changes to I-beam (vertical bar).

- [ ] **Step 4: Manual test - hover over button**

Move mouse over button. Expected: Cursor changes to pointer (hand).

- [ ] **Step 5: Manual test - move outside widgets**

Move mouse outside any interactive widget. Expected: Cursor returns to default arrow.

- [ ] **Step 6: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: cursor system final adjustments"
```

---

## Verification

1. **Unit tests:** Run `cargo test -p vexo` to verify all tests pass
2. **Manual test:** Desktop demo shows correct cursor changes
3. **Code review:** All WidgetResponse returns include cursor field appropriately
