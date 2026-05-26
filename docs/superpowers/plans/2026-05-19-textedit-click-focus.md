# TextEdit Click-to-Focus Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable click-to-focus and click-outside-unfocus for the retain-mode TextEdit widget, with visual focus indicator (border color change).

**Architecture:** The focus plumbing already exists in the framework (EventHandler clears focus on click-outside, StatefulElement requests focus on click-inside). We need to: (1) make focus state available during `StatefulWidget::build()` so TextEdit can render differently when focused, (2) update TextEdit's `build()` to wrap content in a DecoratedContainer with focus-dependent border, and (3) add a TextEdit to the demo app for testing.

**Tech Stack:** Rust, vexo retain-mode three-tree architecture (Widget/Element/RenderObject), glyphon text rendering

---

### Task 1: Add focused_element to BuildOwner

The `BuildOwner` is already accessible from `BuildContext`, so adding `focused_element` here avoids threading it through `ElementContext` (which has 13+ construction sites). The pipeline will update `BuildOwner::focused_element` before each frame's reconcile/rebuild cycle.

**Files:**
- Modify: `vexo/src/retain/build_owner.rs:43-75`

- [ ] **Step 1: Write the failing test**

Add a test to `vexo/src/retain/build_owner_tests.rs` that verifies `BuildOwner` can store and retrieve a focused element:

```rust
#[test]
fn test_build_owner_focused_element() {
    let mut owner = BuildOwner::new();
    let key = {
        let mut sm: slotmap::SlotMap<super::super::id::ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    };

    assert!(owner.focused_element().is_none());

    owner.set_focused_element(Some(key));
    assert_eq!(owner.focused_element(), Some(key));

    owner.set_focused_element(None);
    assert!(owner.focused_element().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_build_owner_focused_element`
Expected: FAIL — `focused_element` method does not exist

- [ ] **Step 3: Write minimal implementation**

Add `focused_element` field to `BuildOwner` in `vexo/src/retain/build_owner.rs`:

```rust
pub struct BuildOwner {
    dirty_elements: RefCell<Vec<ElementKey>>,
    dirty_set: RefCell<HashSet<ElementKey>>,
    building: HashSet<ElementKey>,
    global_keys: RefCell<GlobalKeyRegistry>,
    /// Currently focused element, accessible during build for focus-dependent rendering.
    focused_element: RefCell<Option<ElementKey>>,
}
```

Update `BuildOwner::new()`:

```rust
pub fn new() -> Self {
    Self {
        dirty_elements: RefCell::new(Vec::new()),
        dirty_set: RefCell::new(HashSet::new()),
        building: HashSet::new(),
        global_keys: RefCell::new(GlobalKeyRegistry::new()),
        focused_element: RefCell::new(None),
    }
}
```

Add accessor methods:

```rust
/// Get the currently focused element.
pub fn focused_element(&self) -> Option<ElementKey> {
    *self.focused_element.borrow()
}

/// Set the currently focused element.
pub fn set_focused_element(&self, element: Option<ElementKey>) {
    *self.focused_element.borrow_mut() = element;
}
```

Uses `RefCell` for interior mutability (same pattern as `dirty_elements`), so `set_focused_element()` can be called from event handlers that only have `&BuildOwner`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_build_owner_focused_element`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/build_owner.rs vexo/src/retain/build_owner_tests.rs
git commit -m "feat: add focused_element to BuildOwner for focus-dependent rendering"
```

---

### Task 2: Sync focused_element from pipeline to BuildOwner before builds

The pipeline's `focused_element` (stored in `ThreeTreePipeline`) must be synced to `BuildOwner::focused_element` before each frame's reconcile/rebuild cycle, so that `StatefulWidget::build()` can access it via `BuildContext`.

**Files:**
- Modify: `vexo/src/retain/pipeline.rs:174-207` (reconcile, update methods)

- [ ] **Step 1: Write the failing test**

Add a test that verifies `BuildOwner::focused_element()` is set after `pipeline.update()`:

```rust
#[test]
fn test_pipeline_syncs_focused_element_to_build_owner() {
    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(Text::new("Hello")));

    // Set focus on the root element
    let root_id = pipeline.element_registry().root().unwrap();
    pipeline.set_focus(Some(root_id));

    // After update, BuildOwner should have the focused element
    pipeline.update(Box::new(Text::new("Hello")));
    assert_eq!(pipeline.build_owner().focused_element(), Some(root_id));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_pipeline_syncs_focused_element_to_build_owner`
Expected: FAIL — `focused_element()` returns `None` because sync doesn't happen yet

- [ ] **Step 3: Write minimal implementation**

In `vexo/src/retain/pipeline.rs`, add a helper method and call it before reconcile/update:

```rust
/// Sync focused_element to BuildOwner so StatefulWidget::build() can access it.
fn sync_focus_to_build_owner(&self) {
    self.build_owner.set_focused_element(self.focused_element);
}
```

Call `self.sync_focus_to_build_owner()` at the start of `ThreeTreePipeline::reconcile()` and `ThreeTreePipeline::update()`:

```rust
pub(crate) fn reconcile(&mut self, root_widget: Box<dyn Widget>) {
    self.sync_focus_to_build_owner();
    Reconciler::reconcile(/* ... */);
}

pub fn update(&mut self, root_widget: Box<dyn Widget>) {
    self.sync_focus_to_build_owner();
    Reconciler::update(/* ... */);
}
```

Also sync in `perform_rebuilds()`:

```rust
pub fn perform_rebuilds(&mut self) {
    self.sync_focus_to_build_owner();
    Reconciler::perform_rebuilds(/* ... */);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_pipeline_syncs_focused_element_to_build_owner`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/pipeline.rs
git commit -m "feat: sync focused_element from pipeline to BuildOwner before builds"
```

---

### Task 3: Add is_focused() to BuildContext

`BuildContext` is what `StatefulWidget::build()` receives. Add an `is_focused()` method so widgets can check if they're focused during build.

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs:180-215` (BuildContext struct and impl)

- [ ] **Step 1: Write the failing test**

Add a test that verifies `BuildContext::is_focused()` works:

```rust
#[test]
fn test_build_context_is_focused() {
    let (element_id, _state, mut dirty, mut render_objects, _, build_owner, _dirty_sender, _child_ops) = create_test_context();

    // Set focused element on BuildOwner
    build_owner.set_focused_element(Some(element_id));

    let mut ctx = BuildContext {
        element_id,
        dirty: &mut dirty,
        render_objects: &mut render_objects,
        build_owner: &build_owner,
    };

    assert!(ctx.is_focused());

    // Clear focus
    build_owner.set_focused_element(None);
    assert!(!ctx.is_focused());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_build_context_is_focused`
Expected: FAIL — `is_focused` method does not exist

- [ ] **Step 3: Write minimal implementation**

Add `is_focused()` to `BuildContext` in `vexo/src/retain/stateful_widget.rs`:

```rust
impl<'a> BuildContext<'a> {
    // ... existing methods ...

    /// Check if this element is currently focused.
    pub fn is_focused(&self) -> bool {
        self.build_owner.focused_element() == Some(self.element_id)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_build_context_is_focused`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "feat: add is_focused() to BuildContext for focus-dependent build"
```

---

### Task 4: Update TextEdit::build() to render focus-dependent border

Currently `TextEdit::build()` returns a bare `Text` widget. Change it to return a `DecoratedContainer` wrapping the `Text`, with different border color/width depending on focus state.

**Files:**
- Modify: `vexo/src/retain/widgets/text_edit.rs:336-346` (TextEdit::build)

- [ ] **Step 1: Write the failing test**

Add a test that verifies `TextEdit::build()` produces a `DecoratedContainer` with different styles for focused vs unfocused:

```rust
#[test]
fn test_text_edit_build_unfocused_has_gray_border() {
    let mut fs = create_test_font_system();
    let controller = TextEditingController::new("Hello", &mut fs);
    let text_edit = TextEdit::new(controller.clone());

    let mut state = TextEditState::default();
    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(text_edit.clone()));

    // Build without focus
    let root_id = pipeline.element_registry().root().unwrap();
    let mut build_ctx = retain::BuildContext {
        element_id: root_id,
        dirty: &mut pipeline.dirty,
        render_objects: &mut pipeline.render_objects,
        build_owner: &pipeline.build_owner,
    };

    let widget = text_edit.build(&mut state, &mut build_ctx);
    // Should produce a DecoratedContainer (the widget tree from build)
    let dc = widget.as_any().downcast_ref::<retain::DecoratedContainer>();
    assert!(dc.is_some());
    let dc = dc.unwrap();
    let border = dc.style_ref().border.as_ref().unwrap();
    // Unfocused: gray border
    assert_eq!(border.color, crate::core::Color::rgb(0.6, 0.6, 0.6));
}
```

Note: The exact test structure may need adjustment based on how `build()` is invoked. The key assertion is that the border color differs between focused and unfocused.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_text_edit_build_unfocused_has_gray_border`
Expected: FAIL — `TextEdit::build()` returns a `Text`, not a `DecoratedContainer`

- [ ] **Step 3: Write minimal implementation**

Update `TextEdit::build()` in `vexo/src/retain/widgets/text_edit.rs`:

```rust
impl StatefulWidget for TextEdit {
    type State = TextEditState;

    fn build(
        &self,
        _state: &mut TextEditState,
        ctx: &mut BuildContext,
    ) -> Box<dyn Widget> {
        let is_focused = ctx.is_focused();

        let border_color = if is_focused {
            crate::core::Color::rgb(0.2, 0.4, 0.8) // Blue border when focused
        } else {
            crate::core::Color::rgb(0.6, 0.6, 0.6) // Gray border when unfocused
        };

        let border_width = if is_focused { 2.0 } else { 1.0 };

        let style = crate::retain::Style::new()
            .background(crate::core::Color::WHITE)
            .border(border_color, border_width)
            .corner_radius(4.0)
            .padding(8.0);

        Box::new(
            crate::retain::DecoratedContainer::new(
                Box::new(super::Text::new(self.controller.text()).with_font_size(self.controller.font_size()))
            )
            .style(style)
        )
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_text_edit_build_unfocused_has_gray_border`
Expected: PASS

- [ ] **Step 5: Run all TextEdit tests**

Run: `cargo test -p vexo text_edit`
Expected: All existing tests pass

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs
git commit -m "feat: TextEdit::build() renders focus-dependent border via DecoratedContainer"
```

---

### Task 5: Add TextEdit to the demo app

Add a `retain::TextEdit` to `shared_app/src/lib.rs` so the click-to-focus feature can be tested visually. The `TextEditingController` must persist across frames, so store it in the app's `State` struct.

**Files:**
- Modify: `shared_app/src/lib.rs:172-270` (State struct, retain_view)

- [ ] **Step 1: Add TextEditingController to State**

Update the `State` struct and `Application` trait impl:

```rust
pub struct State {
    click_count: u32,
    milestones: u32,
    text_editor_controller: Option<vexo::retain::TextEditingController>,
}
```

Update `Application::new()`:

```rust
fn new() -> Self::State {
    Self {
        click_count: 0,
        milestones: 0,
        text_editor_controller: None,
    }
}
```

- [ ] **Step 2: Update retain_view to include TextEdit**

Update `retain_view()` to lazily initialize the controller and include a TextEdit:

```rust
fn retain_view(state: &mut Self::State) -> Option<Box<dyn retain::Widget>> {
    // Lazily initialize the TextEdit controller (needs FontSystem, which we don't
    // have here). We'll use a static for now.
    // Note: This is a temporary approach - a proper solution would pass FontSystem
    // through retain_view or use a different initialization pattern.

    let controller = state.text_editor_controller.as_ref()?;

    Some(Box::new(
        retain::Column::new()
            .push(retain::Text::new("Retain Mode Demo"))
            .push(RetainCounter {
                label: "Stateful Counter".to_string(),
            })
            .push(retain::TextEdit::new(controller.clone())),
    ))
}
```

However, there's a problem: `TextEditingController::new()` requires `&mut FontSystem`, which isn't available in `retain_view()`. We need to initialize the controller elsewhere.

**Better approach**: Change `Application::retain_view` to also receive `&mut FontSystem`, or initialize the controller in `new()` using a static FontSystem.

**Simplest approach for now**: Use a `lazy_static` or `thread_local` FontSystem for controller initialization. But that's heavyweight.

**Pragmatic approach**: Initialize the controller in `retain_view()` using a local `FontSystem` each time (it's expensive but works for a demo). Or better, change `retain_view` to take `&mut FontSystem`.

Let me check the `Application` trait signature again. Currently it's `fn retain_view(state: &Self::State)`. We need to change it to also pass `FontSystem`.

Actually, the simplest approach that doesn't require changing the `Application` trait: store the controller as a `OnceCell` pattern. Initialize it once, clone it on subsequent calls.

**Simplest working approach**: Change `retain_view` signature to `fn retain_view(state: &mut Self::State, font_system: &mut glyphon::FontSystem)`. This requires updating the `Application` trait, `WindowState`, and the call sites.

Update `vexo/src/lib.rs` Application trait:

```rust
fn retain_view(state: &mut Self::State, font_system: &mut glyphon::FontSystem) -> Option<Box<dyn retain::Widget>> {
    let _ = (state, font_system);
    None
}
```

Update `WindowState::view_retain()`:

```rust
fn view_retain(&mut self) -> Option<Box<dyn RetainWidget>> {
    A::retain_view(&mut self.user_app_state, &mut self.widget_context.font_system)
}
```

Update `shared_app/src/lib.rs`:

```rust
fn retain_view(state: &mut Self::State, font_system: &mut glyphon::FontSystem) -> Option<Box<dyn retain::Widget>> {
    // Lazily initialize the controller
    if state.text_editor_controller.is_none() {
        state.text_editor_controller = Some(
            vexo::retain::TextEditingController::new("Type here...", font_system)
        );
    }

    let controller = state.text_editor_controller.as_ref().unwrap();

    Some(Box::new(
        retain::Column::new()
            .push(retain::Text::new("Retain Mode Demo"))
            .push(RetainCounter {
                label: "Stateful Counter".to_string(),
            })
            .push(retain::TextEdit::new(controller.clone())),
    ))
}
```

- [ ] **Step 3: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add vexo/src/lib.rs vexo/src/window.rs shared_app/src/lib.rs
git commit -m "feat: add TextEdit to demo app with focus support"
```

---

### Task 6: Verify click-to-focus works end-to-end

Run the desktop demo and manually verify:
1. Click inside the TextEdit → blue border appears (focused)
2. Type text → text appears in the TextEdit
3. Click outside the TextEdit → gray border appears (unfocused)
4. Type text → nothing happens (no focus)

**Files:**
- No code changes — manual verification

- [ ] **Step 1: Run the desktop demo**

Run: `cargo run -p desktop_demo`

- [ ] **Step 2: Verify click-to-focus**

Click inside the TextEdit area. The border should change from gray to blue.

- [ ] **Step 3: Verify keyboard input works when focused**

Type some characters. They should appear in the TextEdit.

- [ ] **Step 4: Verify click-outside-unfocus**

Click outside the TextEdit (on the counter or empty space). The border should change from blue to gray.

- [ ] **Step 5: Verify keyboard input doesn't work when unfocused**

Type some characters. Nothing should happen (no text appears).

---

### Task 7: Add integration test for focus behavior

Add an automated test that verifies the focus flow through the pipeline.

**Files:**
- Modify: `vexo/src/retain/widgets/text_edit.rs` (add tests at bottom)

- [ ] **Step 1: Write the test**

```rust
#[test]
fn test_text_edit_focus_on_click_inside() {
    let mut fs = create_test_font_system();
    let controller = TextEditingController::new("Hello", &mut fs);
    let text_edit = TextEdit::new(controller.clone());

    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(text_edit));

    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

    // Simulate a click inside the TextEdit
    let event = InputEvent::PointerButton {
        position: Point::new(10.0, 10.0),
        button: crate::input::PointerButton::Primary,
        state: crate::input::ButtonState::Pressed,
    };

    let _ = pipeline.handle_event(
        Point::new(10.0, 10.0),
        &event,
        Modifiers::default(),
        &mut fs,
    );

    // The TextEdit element should now be focused
    assert!(pipeline.focused_element().is_some());
}

#[test]
fn test_text_edit_unfocus_on_click_outside() {
    let mut fs = create_test_font_system();
    let controller = TextEditingController::new("Hello", &mut fs);
    let text_edit = TextEdit::new(controller.clone());

    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(text_edit));

    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

    // First, click inside to focus
    let click_inside = InputEvent::PointerButton {
        position: Point::new(10.0, 10.0),
        button: crate::input::PointerButton::Primary,
        state: crate::input::ButtonState::Pressed,
    };
    let _ = pipeline.handle_event(Point::new(10.0, 10.0), &click_inside, Modifiers::default(), &mut fs);
    assert!(pipeline.focused_element().is_some());

    // Now click outside (far away)
    let click_outside = InputEvent::PointerButton {
        position: Point::new(700.0, 500.0),
        button: crate::input::PointerButton::Primary,
        state: crate::input::ButtonState::Pressed,
    };
    let _ = pipeline.handle_event(Point::new(700.0, 500.0), &click_outside, Modifiers::default(), &mut fs);

    // Focus should be cleared
    assert!(pipeline.focused_element().is_none());
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vexo test_text_edit_focus_on_click_inside test_text_edit_unfocus_on_click_outside`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs
git commit -m "test: add integration tests for TextEdit click-to-focus behavior"
```

---

### Task 8: Run full test suite

- [ ] **Step 1: Run all vexo tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 2: Run all shared_app tests**

Run: `cargo test -p shared_app`
Expected: All tests pass

- [ ] **Step 3: Run full workspace build**

Run: `cargo build`
Expected: Compiles successfully
