# Retain-Mode Smoke Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add keyboard toggle to switch between immediate and retain mode in the existing demo app.

**Architecture:** Add `use_retain_mode` flag to State, handle 'R' key to toggle, implement `retain_view()` with simple widget tree, sync flag to WindowState.

**Tech Stack:** Rust, vexo retain module, Application trait

---

## File Structure

**Files to modify:**
- `shared_app/src/lib.rs` - Add flag, message, update handler, retain_view()
- `vexo/src/window.rs` - Add method to sync retain mode flag from app state

---

### Task 1: Add ToggleRetainMode message

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add ToggleRetainMode variant to Message enum**

```rust
// Update Message enum (around line 5):

#[derive(Debug, Clone)]
pub enum Message {
    None,
    Clicked,
    CounterOutput(CounterOutput),
    ToggleRetainMode,  // New variant
}
```

- [ ] **Step 2: Handle ToggleRetainMode in Application::update()**

```rust
// Update Application::update() (around line 104):

fn update(state: &mut Self::State, message: Self::Message) {
    match message {
        Message::Clicked => {
            state.click_count += 1;
        }
        Message::CounterOutput(CounterOutput::CountReached(_n)) => {
            state.milestones += 1;
        }
        Message::None => {}
        Message::ToggleRetainMode => {
            // This message is handled by WindowState, not the app state
            // The retain mode toggle is a framework-level concern
        }
    }
}
```

- [ ] **Step 3: Run build to verify compilation**

Run: `cargo build -p shared_app`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat: add ToggleRetainMode message"
```

---

### Task 2: Add keyboard handler for 'R' key

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add keyboard event handler to view**

The immediate-mode widgets handle events through `on_event()`. We need to add a keyboard listener. The simplest approach is to add a key handler widget or modify the root column to capture keyboard events.

Add a key handler at the top of the view:

```rust
// In Application::view(), add keyboard handling wrapper (around line 116):

fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>> {
    use vexo::input::{InputEvent, Key, NamedKey};

    let text_content = format!("You clicked {} times!", state.click_count);
    let milestone_text = format!("Milestones reached: {}", state.milestones);
    let mode_text = if state.use_retain_mode {
        "Mode: RETAIN (press R to switch)"
    } else {
        "Mode: IMMEDIATE (press R to switch)"
    };

    vexo::column![
        // Mode indicator
        vexo::text!(mode_text)
            .font_size(16.0)
            .padding(8.0),
        // Title
        vexo::text!("ScrollView Demo")
            .font_size(28.0),
        // ... rest of existing view ...
    ]
    .align(vexo::layout::AlignItems::Center)
    .fill()
    .background(vexo::Color::WHITE)
    .boxed()
}
```

Note: The keyboard handling needs to be done through the widget system. We'll add a transparent overlay widget that captures 'R' key presses.

- [ ] **Step 2: Add keyboard capture widget**

Create a custom widget that captures 'R' key and emits ToggleRetainMode. Add this helper before the State struct:

```rust
// Add near top of file after imports:

/// A widget that captures 'R' key presses to toggle retain mode.
struct RetainModeKeyHandler;

impl vexo::widgets::Widget<Message> for RetainModeKeyHandler {
    fn key(&self) -> Option<&str> {
        Some("retain-mode-handler")
    }

    fn layout(
        &mut self,
        layout_ctx: &mut vexo::layout::LayoutContext,
        _widget_ctx: &mut vexo::widgets::WidgetContext,
    ) -> vexo::layout::LayoutNodeId {
        layout_ctx.create_leaf(&vexo::layout::Layout::default())
    }

    fn apply_layout(&mut self, _layout: vexo::layout::ComputedLayout) {}

    fn draw(
        &self,
        _layout_view: &vexo::layout::LayoutView,
        _node: vexo::layout::LayoutNodeId,
        _renderer: &mut vexo::renderer::UiBatcher,
        _offset: vexo::core::Point<vexo::core::Logical>,
        _focused_id: Option<vexo::core::WidgetId>,
        _cursor_blink: &vexo::CursorBlinkState,
        _widget_ctx: &mut vexo::widgets::WidgetContext,
    ) {
        // No drawing - this is just an event handler
    }

    fn on_event(
        &mut self,
        _layout_view: &vexo::layout::LayoutView,
        _node: vexo::layout::LayoutNodeId,
        _offset: vexo::core::Point<vexo::core::Logical>,
        event: &vexo::input::InputEvent,
        _focused_id: Option<vexo::core::WidgetId>,
        _widget_ctx: &mut vexo::widgets::WidgetContext,
    ) -> vexo::widgets::WidgetResponse<Message> {
        use vexo::input::{InputEvent, Key, NamedKey};

        if let InputEvent::Keyboard { key, state, .. } = event {
            if *state == vexo::input::ButtonState::Pressed {
                if let Key::Named(NamedKey::Character(c)) = key {
                    if c == "r" || c == "R" {
                        return vexo::widgets::WidgetResponse {
                            message: Some(Message::ToggleRetainMode),
                            handled: true,
                            ..Default::default()
                        };
                    }
                }
            }
        }
        vexo::widgets::WidgetResponse::default()
    }
}
```

- [ ] **Step 3: Add the key handler to the view**

```rust
// In Application::view(), wrap the column with the key handler:

fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>> {
    let text_content = format!("You clicked {} times!", state.click_count);
    let milestone_text = format!("Milestones reached: {}", state.milestones);
    let mode_text = if state.use_retain_mode {
        "Mode: RETAIN (press R to switch)"
    } else {
        "Mode: IMMEDIATE (press R to switch)"
    };

    vexo::column![
        // Mode indicator
        vexo::text!(mode_text)
            .font_size(16.0)
            .padding(8.0),
        // Title
        vexo::text!("ScrollView Demo")
            .font_size(28.0),
        // ... rest of existing view content ...
    ]
    .align(vexo::layout::AlignItems::Center)
    .fill()
    .background(vexo::Color::WHITE)
    .boxed()
}
```

Actually, the simpler approach is to use a Stack widget to overlay the key handler. But since Stack may not exist, let's use a different approach: add the key handling to an existing widget.

Looking at the codebase, the cleanest approach is to add a global keyboard event handler in WindowState that checks for 'R' key and calls a method on Application.

Let me revise this task to use a simpler approach.

- [ ] **Step 2 (revised): Add keyboard handling in WindowState**

Modify `vexo/src/window.rs` to check for 'R' key in the keyboard event handler:

```rust
// In handle_window_event(), modify the KeyboardInput handler (around line 262):

WindowEvent::KeyboardInput { event: key_event, .. } => {
    // Handle Escape key for app exit (framework-level shortcut)
    if matches!(
        key_event,
        KeyEvent {
            physical_key: PhysicalKey::Code(KeyCode::Escape),
            state: ElementState::Pressed,
            repeat: false,
            ..
        }
    ) {
        event_loop.exit();
        return;
    }

    // Handle 'R' key to toggle retain mode
    if matches!(
        key_event,
        KeyEvent {
            physical_key: PhysicalKey::Code(KeyCode::KeyR),
            state: ElementState::Pressed,
            repeat: false,
            ..
        }
    ) {
        self.toggle_retain_mode();
        return;
    }

    // Other keyboard input goes to widgets
    if let Some(input_event) =
        InputEvent::from_winit(event, self.widget_context.scale)
    {
        self.process_input_event(input_event);
    }
}
```

- [ ] **Step 3: Add toggle_retain_mode() method to WindowState**

```rust
// In WindowState impl, add after set_retain_mode():

/// Toggle retain mode and sync with application state.
fn toggle_retain_mode(&mut self) {
    self.use_retain_mode = !self.use_retain_mode;
    if let Some(win) = &self.window {
        win.request_redraw();
    }
    println!("Retain mode: {}", self.use_retain_mode);
}
```

- [ ] **Step 4: Run build to verify compilation**

Run: `cargo build -p vexo`
Expected: Build succeeds

- [ ] **Step 5: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat: add 'R' key handler to toggle retain mode"
```

---

### Task 3: Implement retain_view() in Application

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add retain module import**

```rust
// At top of file, update imports:

use vexo::{widgets::Widget, Application, WidgetExt, retain, Color};
```

- [ ] **Step 2: Implement retain_view() in Application impl**

```rust
// In Application impl for State, add after view():

fn retain_view(_state: &Self::State) -> Option<Box<dyn retain::Widget>> {
    // Simple widget tree to test retain-mode rendering:
    // Background(Color::BLUE)
    // └── Border(Color::BLACK, 2.0)
    //     └── Text("Retain Mode Active")
    Some(Box::new(
        retain::Background::new(
            Box::new(
                retain::Border::new(
                    Box::new(retain::Text::new("Retain Mode Active")),
                    Color::BLACK,
                    2.0,
                )
            ),
            Color::BLUE,
        )
    ))
}
```

Note: This always returns a widget tree. The `WindowState.use_retain_mode` flag controls whether it's used.

- [ ] **Step 3: Run build to verify compilation**

Run: `cargo build -p shared_app`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat: implement retain_view() with simple widget tree"
```

---

### Task 4: Test the smoke test

**Files:**
- None (manual testing)

- [ ] **Step 1: Run the app**

Run: `cargo run -p desktop_demo`
Expected: App starts in immediate mode

- [ ] **Step 2: Manual test checklist**

1. App starts showing immediate mode UI with "Mode: IMMEDIATE (press R to switch)"
2. Press 'R' - screen shows "Retain Mode Active" with blue background and black border
3. Press 'R' again - returns to immediate mode UI
4. No crashes or GPU errors
5. Console shows "Retain mode: true" / "Retain mode: false" on toggle

- [ ] **Step 3: Mark task complete if all tests pass**

---

### Task 5: Add hint text to immediate mode view

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add hint text to view()**

```rust
// In Application::view(), add hint text at top:

fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>> {
    let text_content = format!("You clicked {} times!", state.click_count);
    let milestone_text = format!("Milestones reached: {}", state.milestones);

    vexo::column![
        // Hint for retain mode toggle
        vexo::text!("Press R to toggle retain mode")
            .font_size(14.0)
            .padding(4.0),
        // Title
        vexo::text!("ScrollView Demo")
            .font_size(28.0),
        // ScrollView with many items to demonstrate scrolling
        vexo::widgets::ScrollView::new()
            .with_key("demo-scroll")
            .width(350.0)
            .height(300.0)
            .push(vexo::text!("Scrollable Content").font_size(20.0))
            .push(vexo::text!("─────────────────────"))
            .push(vexo::text!("Item 1: Scroll wheel works!").padding(8.0))
            .push(vexo::text!("Item 2: Drag to scroll").padding(8.0))
            .push(vexo::text!("Item 3: Use arrow keys").padding(8.0))
            .push(vexo::text!("Item 4: Page Up/Down too").padding(8.0))
            .push(vexo::text!("─────────────────────"))
            .push(vexo::text!("Item 5").padding(8.0))
            .push(vexo::text!("Item 6").padding(8.0))
            .push(vexo::text!("Item 7").padding(8.0))
            .push(vexo::text!("Item 8").padding(8.0))
            .push(vexo::text!("Item 9").padding(8.0))
            .push(vexo::text!("Item 10").padding(8.0))
            .push(vexo::text!("Item 11").padding(8.0))
            .push(vexo::text!("Item 12").padding(8.0))
            .push(vexo::text!("Item 13").padding(8.0))
            .push(vexo::text!("Item 14").padding(8.0))
            .push(vexo::text!("Item 15").padding(8.0))
            .push(vexo::text!("Item 16").padding(8.0))
            .push(vexo::text!("Item 17").padding(8.0))
            .push(vexo::text!("Item 18").padding(8.0))
            .push(vexo::text!("Item 19").padding(8.0))
            .push(vexo::text!("Item 20 - End of list!").padding(8.0))
            .background(vexo::Color::rgb(0.95, 0.95, 0.98))
            .border(vexo::Color::GRAY, 1.0)
            .corner_radius(8.0)
            .boxed(),
        // Counter Component with message mapping
        vexo::component!(CounterComponent, "counter", |output| Message::CounterOutput(output)),
        // Milestone display
        vexo::text!(milestone_text)
            .font_size(18.0)
            .padding(10.0),
    ]
    .align(vexo::layout::AlignItems::Center)
    .fill()
    .background(vexo::Color::WHITE)
    .boxed()
}
```

- [ ] **Step 2: Run build to verify compilation**

Run: `cargo build -p shared_app`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat: add hint text for retain mode toggle"
```

---

## Summary

This plan adds a retain-mode smoke test to the existing demo:

1. **Task 1**: Add `ToggleRetainMode` message (handled by WindowState)
2. **Task 2**: Add 'R' key handler in WindowState to toggle `use_retain_mode`
3. **Task 3**: Implement `retain_view()` with simple widget tree (Background + Border + Text)
4. **Task 4**: Manual testing to verify toggle works
5. **Task 5**: Add hint text to immediate mode view

After completion:
- Press 'R' toggles between immediate and retain mode
- Retain mode shows blue rectangle with black border and "Retain Mode Active" text
- Immediate mode continues working unchanged
