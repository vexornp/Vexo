# Context Menu on Message Bubbles — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a floating context menu at the cursor when the user right-clicks a message bubble in the chat screen, via a reusable `ContextMenu` widget triple in `vexo_uikit`.

**Architecture:** Fix winit's button mapping (hardcoded to `Primary`) so right-click produces `PointerButton::Secondary`. Add `on_secondary_press` to `GestureDetector` (position-aware callback). Gate the gesture arena on `Primary` so right-click never triggers `on_tap` or scroll. Build a `ContextMenuController`/`ContextMenu`/`context_menu_trigger` triple in `vexo_uikit` that uses a `Signal<Option<Point<Logical>>>` for open/close + position, and an `Rc<RefCell<Vec<MenuItem>>>` for items (kept outside the Signal because `Rc<dyn Fn()>` is `!Send + !Sync`, violating `signal_value`'s bound). Wire it into `ChatScreen`.

**Tech Stack:** Rust, winit 0.31.0-beta.2, Taffy layout, glyphon text, Vexo three-tree widget framework.

## Global Constraints

- `Signal<T>` requires `T: PartialEq + Clone + Send + Sync + 'static` for `signal_value` / `derive`. `Point<Logical>` satisfies this (`Copy + Send + Sync`); `Vec<MenuItem>` does not (contains `Rc<dyn Fn()>` which is `!Send + !Sync`).
- `Signal::set` requires `T: PartialEq + Copy`; `set_from` requires `T: PartialEq + Clone`. `Option<Point<Logical>>: Copy + PartialEq` — use `set`.
- winit 0.31 `WindowEvent::PointerButton` has fields: `device_id: Option<DeviceId>`, `state: ElementState`, `position: PhysicalPosition<f64>`, `primary: bool`, `button: ButtonSource`. `ButtonSource::Mouse(MouseButton)` where `MouseButton::{Left, Right, Middle, Back, Forward, ...}`. `ButtonSource` is `Clone` (not `Copy`).
- Hit-test order in `Stack`: children tested in **reverse** (last child = topmost = tested first). See `vexo/src/hit_test.rs:395`.
- Non-positioned children in `Stack` flow in a column flexbox (they do NOT overlap). `Positioned` children are taken out of flow and overlap. The dismiss barrier must be `Positioned` with all insets 0.
- `MouseRegion` is `pub(crate)` — not usable from `vexo_uikit`. No hover effects in v1.
- Run `cargo build` after each crate's edits, `cargo test` after each feature slice. Never assume tests pass.
- Never run `cargo run -p desktop_demo` — ask the user.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `vexo/src/input/event.rs` | Modify | Map winit `ButtonSource` → `PointerButton` in `from_winit` |
| `vexo/src/widgets/gesture_detector.rs` | Modify | Add `on_secondary_press` field + `on_event` logic + tests |
| `vexo/src/widgets/mod.rs` | Modify | Add `Widget::on_secondary_press` fluent API |
| `vexo/src/event_handler.rs` | Modify | Gate arena creation/resolution on `Primary` button |
| `vexo_uikit/src/context_menu.rs` | Create | `MenuItem`, `ContextMenuController`, `ContextMenu` host, `context_menu_trigger`, `menu_view` |
| `vexo_uikit/src/lib.rs` | Modify | Export `context_menu` module |
| `shared_app/src/chats/chat_screen.rs` | Modify | Wire controller + host + trigger; update tests |

---

## Task 1: Fix winit → PointerButton mapping

**Files:**
- Modify: `vexo/src/input/event.rs:272-292`
- Test: `vexo/src/input/event.rs` (new test in `#[cfg(test)]` module)

**Interfaces:**
- Produces: `InputEvent::from_winit` now produces `PointerButton::Secondary` for right-click, `Tertiary` for middle-click. All downstream code sees the correct `button` field.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` module in `vexo/src/input/event.rs` (if no test module exists, create one at the bottom of the file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::PhysicalPosition;

    fn make_pointer_button_event(button: winit::event::ButtonSource) -> winit::event::WindowEvent {
        winit::event::WindowEvent::PointerButton {
            device_id: Some(winit::event::DeviceId::from_raw(0)),
            state: winit::event::ElementState::Pressed,
            position: PhysicalPosition::new(100.0, 100.0),
            primary: true,
            button,
        }
    }

    #[test]
    fn from_winit_maps_right_click_to_secondary() {
        let event = make_pointer_button_event(winit::event::ButtonSource::Mouse(
            winit::event::MouseButton::Right,
        ));
        let scale = ScaleSource::default();
        let pos = Point::<Logical>::new(0.0, 0.0);
        let result = InputEvent::from_winit(&event, &scale, pos).unwrap();
        match result {
            InputEvent::PointerButton { button, .. } => {
                assert_eq!(button, PointerButton::Secondary);
            }
            _ => panic!("expected PointerButton event"),
        }
    }

    #[test]
    fn from_winit_maps_middle_click_to_tertiary() {
        let event = make_pointer_button_event(winit::event::ButtonSource::Mouse(
            winit::event::MouseButton::Middle,
        ));
        let scale = ScaleSource::default();
        let pos = Point::<Logical>::new(0.0, 0.0);
        let result = InputEvent::from_winit(&event, &scale, pos).unwrap();
        match result {
            InputEvent::PointerButton { button, .. } => {
                assert_eq!(button, PointerButton::Tertiary);
            }
            _ => panic!("expected PointerButton event"),
        }
    }

    #[test]
    fn from_winit_maps_left_click_to_primary() {
        let event = make_pointer_button_event(winit::event::ButtonSource::Mouse(
            winit::event::MouseButton::Left,
        ));
        let scale = ScaleSource::default();
        let pos = Point::<Logical>::new(0.0, 0.0);
        let result = InputEvent::from_winit(&event, &scale, pos).unwrap();
        match result {
            InputEvent::PointerButton { button, .. } => {
                assert_eq!(button, PointerButton::Primary);
            }
            _ => panic!("expected PointerButton event"),
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib input::event::tests`
Expected: FAIL — `from_winit` currently hardcodes `PointerButton::Primary` for all buttons, so the Secondary and Tertiary assertions fail.

- [ ] **Step 3: Write minimal implementation**

In `vexo/src/input/event.rs`, replace the `WindowEvent::PointerButton` arm of `from_winit` (lines 272-292). Change `button: _` to `button` and add the mapping:

```rust
            WindowEvent::PointerButton {
                state,
                button,
                position,
                ..
            } => {
                let physical = Point::<Physical>::new(position.x as f32, position.y as f32);
                let logical = physical.to_logical(scale);

                let button_state = match state {
                    ElementState::Pressed => ButtonState::Pressed,
                    ElementState::Released => ButtonState::Released,
                };

                let pointer_button = match button {
                    winit::event::ButtonSource::Mouse(winit::event::MouseButton::Right) => {
                        PointerButton::Secondary
                    }
                    winit::event::ButtonSource::Mouse(winit::event::MouseButton::Middle) => {
                        PointerButton::Tertiary
                    }
                    _ => PointerButton::Primary,
                };

                Some(InputEvent::PointerButton {
                    position: logical,
                    button: pointer_button,
                    state: button_state,
                })
            }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib input::event::tests`
Expected: PASS — all three mapping tests pass.

- [ ] **Step 5: Run full vexo test suite to check for regressions**

Run: `cargo test -p vexo`
Expected: PASS — no regressions. (Existing tests use synthetic `InputEvent`s with `PointerButton::Primary`, which is unaffected.)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/input/event.rs
git commit -m "fix(vexo): map winit right/middle click to Secondary/Tertiary PointerButton

from_winit hardcoded all mouse buttons to Primary, making right-click
indistinguishable from left-click. Map ButtonSource::Mouse(Right) to
Secondary and Middle to Tertiary; everything else stays Primary."
```

---

## Task 2: GestureDetector::on_secondary_press

**Files:**
- Modify: `vexo/src/widgets/gesture_detector.rs` (struct fields, builder, element, on_event, clone, set_widget_from_widget, rebuild)
- Test: `vexo/src/widgets/gesture_detector.rs` (new tests in existing `#[cfg(test)]` module)

**Interfaces:**
- Produces: `GestureDetector::on_secondary_press(impl FnMut(Point<Logical>) + 'static) -> Self` — fires on `Secondary`+`Pressed` with the global cursor position. When set, takes precedence over `on_press` for Secondary presses.

- [ ] **Step 1: Write the failing tests**

Add these tests to the existing `#[cfg(test)]` mod in `vexo/src/widgets/gesture_detector.rs`:

```rust
    #[test]
    fn test_on_secondary_press_fires_with_position() {
        let captured_pos = Rc::new(Cell::new(Point::<Logical>::new(-1.0, -1.0)));
        let pos_clone = captured_pos.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_secondary_press = Some(Rc::new(RefCell::new(move |pos| {
            pos_clone.set(pos);
        })));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(42.0, 17.0),
            button: crate::input::PointerButton::Secondary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(result.is_some(), "on_secondary_press should claim the event");
        assert_eq!(captured_pos.get(), Point::new(42.0, 17.0));
    }

    #[test]
    fn test_on_secondary_press_does_not_fire_on_primary() {
        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_secondary_press = Some(Rc::new(RefCell::new(move |_pos| {
            called_clone.set(true);
        })));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(!called.get(), "on_secondary_press must not fire on Primary");
        assert!(result.is_none(), "Primary with no on_press should not claim");
    }

    #[test]
    fn test_secondary_press_skips_on_press_when_both_set() {
        let secondary_called = Rc::new(Cell::new(false));
        let press_called = Rc::new(Cell::new(false));
        let sec_clone = secondary_called.clone();
        let press_clone = press_called.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_secondary_press = Some(Rc::new(RefCell::new(move |_pos| {
            sec_clone.set(true);
        })));
        elem.on_press = Some(Rc::new(RefCell::new(move || {
            press_clone.set(true);
        })));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Secondary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(secondary_called.get(), "on_secondary_press should fire");
        assert!(!press_called.get(), "on_press should be skipped");
        assert!(result.is_some());
    }

    #[test]
    fn test_secondary_press_falls_through_to_on_press_when_not_set() {
        let press_called = Rc::new(Cell::new(false));
        let press_clone = press_called.clone();

        let mut elem = GestureDetectorElement::new();
        // No on_secondary_press set — only on_press.
        elem.on_press = Some(Rc::new(RefCell::new(move || {
            press_clone.set(true);
        })));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Secondary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(press_called.get(), "on_press should fire as fall-through");
        assert!(result.is_some());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib widgets::gesture_detector::tests::test_on_secondary_press`
Expected: FAIL — `GestureDetectorElement` has no `on_secondary_press` field; compilation error.

- [ ] **Step 3: Add the `on_secondary_press` field to `GestureDetector` widget**

In `vexo/src/widgets/gesture_detector.rs`, add the field to the `GestureDetector` struct (after `on_tap`, around line 71):

```rust
    /// Callback invoked when the secondary (right) mouse button is pressed
    /// inside the child bounds. Receives the global cursor position.
    on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>)>>>,
```

Update `GestureDetector::new` (around line 77) to initialize it:

```rust
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            on_press: None,
            on_release: None,
            on_tap: None,
            on_secondary_press: None,
        }
    }
```

Add the builder method (after `on_tap`, around line 110):

```rust
    /// Set the callback for secondary (right-click) button press events.
    /// Receives the global cursor position (window-logical coordinates).
    /// When set, this takes precedence over `on_press` for Secondary presses.
    pub fn on_secondary_press(
        mut self,
        callback: impl FnMut(Point<Logical>) + 'static,
    ) -> Self {
        self.on_secondary_press = Some(Rc::new(RefCell::new(callback)));
        self
    }
```

Update `Clone for GestureDetector` (around line 118) to clone the new field:

```rust
impl Clone for GestureDetector {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            on_press: self.on_press.clone(),
            on_release: self.on_release.clone(),
            on_tap: self.on_tap.clone(),
            on_secondary_press: self.on_secondary_press.clone(),
        }
    }
}
```

- [ ] **Step 4: Add the field to `GestureDetectorElement` and wire it**

Add the field to `GestureDetectorElement` struct (after `on_tap`, around line 176):

```rust
    on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>)>>>,
```

Update `GestureDetectorElement::new` (around line 182):

```rust
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            on_press: None,
            on_release: None,
            on_tap: None,
            on_secondary_press: None,
            focus_attachment: None,
        }
    }
```

Update `set_widget_from_widget` (around line 196):

```rust
    fn set_widget_from_widget(&mut self, widget: &GestureDetector) {
        self.key = widget.key.clone();
        self.on_press = widget.on_press.clone();
        self.on_release = widget.on_release.clone();
        self.on_tap = widget.on_tap.clone();
        self.on_secondary_press = widget.on_secondary_press.clone();
        self.widget = Some(widget.clone_boxed());
    }
```

Update `set_widget` in the `RenderObjectElement` impl (around line 224):

```rust
    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
            self.key = gd.key.clone();
            self.on_press = gd.on_press.clone();
            self.on_release = gd.on_release.clone();
            self.on_tap = gd.on_tap.clone();
            self.on_secondary_press = gd.on_secondary_press.clone();
        }
        self.widget = Some(widget);
    }
```

Update `rebuild` (around line 373):

```rust
            if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
                self.on_press = gd.on_press.clone();
                self.on_release = gd.on_release.clone();
                self.on_tap = gd.on_tap.clone();
                self.on_secondary_press = gd.on_secondary_press.clone();
            }
```

- [ ] **Step 5: Update `on_event` to handle Secondary button**

Replace the `on_event` method (lines 314-342) with:

```rust
    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        if let InputEvent::PointerButton {
            state,
            position,
            button,
        } = event
        {
            if context.bounds().contains(position) {
                match state {
                    ButtonState::Pressed => {
                        // Secondary (right-click) with on_secondary_press set:
                        // fire it with position, claim the event, skip on_press.
                        if *button == crate::input::PointerButton::Secondary {
                            if let Some(callback) = &self.on_secondary_press {
                                (callback.borrow_mut())(*position);
                                return Some(Box::new(()));
                            }
                            // Fall through to on_press for Secondary when
                            // on_secondary_press is not set (backward-compat:
                            // dismiss barrier closes on any button).
                        }
                        if let Some(callback) = &self.on_press {
                            (callback.borrow_mut())();
                            return Some(Box::new(()));
                        }
                    }
                    ButtonState::Released => {
                        if let Some(callback) = &self.on_release {
                            (callback.borrow_mut())();
                            return Some(Box::new(()));
                        }
                    }
                }
            }
        }
        None
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::gesture_detector`
Expected: PASS — all 4 new tests pass, plus all existing tests.

- [ ] **Step 7: Run full vexo test suite**

Run: `cargo test -p vexo`
Expected: PASS — no regressions.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/widgets/gesture_detector.rs
git commit -m "feat(vexo): add GestureDetector::on_secondary_press for right-click

Fires on Secondary+Pressed with the global cursor position. When set,
takes precedence over on_press for right-clicks. When not set, falls
through to on_press (backward-compat for dismiss barriers that close
on any button)."
```

---

## Task 3: Widget::on_secondary_press fluent API

**Files:**
- Modify: `vexo/src/widgets/mod.rs:197-218` (add trait method after `on_tap`)
- Test: `vexo/src/widgets/mod.rs` (or `gesture_detector.rs` tests)

**Interfaces:**
- Produces: `Widget::on_secondary_press(self, impl FnMut(Point<Logical>) + 'static) -> Box<dyn Widget>` — wraps in `GestureDetector`.

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/widgets/gesture_detector.rs` tests:

```rust
    #[test]
    fn test_widget_trait_on_secondary_press() {
        use crate::core::Logical;
        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        // Use the Widget trait method on a Text widget.
        let widget: Box<dyn Widget> = Text::new("Right-click me")
            .on_secondary_press(move |_pos: Point<Logical>| {
                called_clone.set(true);
            });

        // Verify it wrapped in a GestureDetector.
        assert!(widget.as_any().downcast_ref::<GestureDetector>().is_some());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib widgets::gesture_detector::tests::test_widget_trait_on_secondary_press`
Expected: FAIL — `on_secondary_press` not found on `Widget` trait.

- [ ] **Step 3: Add the trait method**

In `vexo/src/widgets/mod.rs`, after the `on_tap` method (around line 218), add:

```rust
    fn on_secondary_press(
        self,
        callback: impl FnMut(crate::core::Point<crate::core::Logical>) + 'static,
    ) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(GestureDetector::new(self).on_secondary_press(callback))
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib widgets::gesture_detector::tests::test_widget_trait_on_secondary_press`
Expected: PASS.

- [ ] **Step 5: Run full vexo test suite**

Run: `cargo test -p vexo`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/mod.rs vexo/src/widgets/gesture_detector.rs
git commit -m "feat(vexo): add Widget::on_secondary_press fluent API

Wraps in GestureDetector::new(self).on_secondary_press(callback),
mirroring on_press/on_tap."
```

---

## Task 4: Gate gesture arena on Primary button

**Files:**
- Modify: `vexo/src/event_handler.rs:235-268` (press block), `vexo/src/event_handler.rs:323-373` (release block)
- Test: `vexo/src/integration_tests.rs` (new test)

**Interfaces:**
- Produces: right-click (`Secondary`) never creates/resolves the gesture arena. `on_tap` (arena-mediated) and drag/scroll recognizers only fire for `Primary`. `on_press`/`on_release` (immediate, non-arena) still fire for all buttons via the bubble phase.

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/integration_tests.rs` (in the `#[cfg(test)]` module, near the existing `test_pipeline_handle_event` tests):

```rust
    #[test]
    fn test_secondary_press_does_not_fire_on_tap() {
        use std::cell::Cell;
        use std::rc::Rc;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::core::Point;
        use crate::widgets::GestureDetector;
        use crate::Text;

        let tap_count = Rc::new(Cell::new(0u32));
        let tap_clone = tap_count.clone();

        // A tappable widget: on_tap increments the counter.
        let widget: Box<dyn Widget> = GestureDetector::new(Text::new("Tap me"))
            .on_tap(move || {
                tap_clone.set(tap_clone.get() + 1);
            });

        let mut pipeline: ThreeTreePipeline =
            ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(widget);

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Primary press+release at (5, 5) — should fire on_tap.
        let primary_press = InputEvent::PointerButton {
            position: Point::new(5.0, 5.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let primary_release = InputEvent::PointerButton {
            position: Point::new(5.0, 5.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(5.0, 5.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        pipeline.handle_event(
            Point::new(5.0, 5.0),
            &primary_release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(tap_count.get(), 1, "Primary tap should fire on_tap");

        // Secondary press+release at (5, 5) — should NOT fire on_tap.
        let secondary_press = InputEvent::PointerButton {
            position: Point::new(5.0, 5.0),
            button: PointerButton::Secondary,
            state: ButtonState::Pressed,
        };
        let secondary_release = InputEvent::PointerButton {
            position: Point::new(5.0, 5.0),
            button: PointerButton::Secondary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(5.0, 5.0),
            &secondary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        pipeline.handle_event(
            Point::new(5.0, 5.0),
            &secondary_release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(
            tap_count.get(),
            1,
            "Secondary press must NOT fire on_tap (arena should be gated on Primary)"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib integration_tests::tests::test_secondary_press_does_not_fire_on_tap`
Expected: FAIL — the tap count increments to 2 because the arena is not gated; Secondary press+release resolves the tap recognizer.

- [ ] **Step 3: Implement arena gating**

In `vexo/src/event_handler.rs`, inside `handle_pointer_event`, after the `is_press`/`is_release`/`is_move` declarations (around line 250), add:

```rust
        let button = match event {
            InputEvent::PointerButton { button, .. } => Some(*button),
            _ => None,
        };
        let is_primary = button == Some(crate::input::PointerButton::Primary);
```

Change the press block (line 253) from:

```rust
        if is_press {
```

to:

```rust
        if is_press && is_primary {
```

Change the release block (line 325) from:

```rust
        if is_release {
```

to:

```rust
        if is_release && is_primary {
```

(The move block stays unchanged — moves are button-agnostic and the arena only exists if a Primary press created it. If no arena exists, `if let Some(arena) = current_arena.as_mut()` is `None` and the block is skipped.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib integration_tests::tests::test_secondary_press_does_not_fire_on_tap`
Expected: PASS — Secondary press+release does not increment the tap counter.

- [ ] **Step 5: Run full vexo test suite**

Run: `cargo test -p vexo`
Expected: PASS — no regressions. Existing tests use `Primary` for all synthetic events, which is unaffected.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/event_handler.rs vexo/src/integration_tests.rs
git commit -m "fix(vexo): gate gesture arena on Primary button

Right-click (Secondary) no longer creates or resolves the gesture arena,
so on_tap and drag/scroll recognizers only fire for Primary. This fixes
a latent bug: after the winit button mapping fix, right-click would have
fired on_tap on tappable widgets (e.g. Send button would send on
right-click). on_press/on_release (immediate, non-arena) still fire for
all buttons via the bubble phase."
```

---

## Task 5: MenuItem + ContextMenuController

**Files:**
- Create: `vexo_uikit/src/context_menu.rs`
- Modify: `vexo_uikit/src/lib.rs` (add module + exports)
- Test: `vexo_uikit/src/context_menu.rs` (unit tests)

**Interfaces:**
- Produces: `MenuItem { label: String, on_select: Rc<dyn Fn()> }` (public, `Clone`), `ContextMenuController` (public, `Clone`) with `new()`, `show(position, items)`, `close()`, `position_signal()`, `items_snapshot()`.

- [ ] **Step 1: Write the failing test**

Create `vexo_uikit/src/context_menu.rs` with the test module first:

```rust
//! Context menu widget trio: `MenuItem`, `ContextMenuController`, `ContextMenu` host.
//!
//! Mirrors the `ScrollController` pattern: the screen owns a controller,
//! wraps its root in `ContextMenu::new(child, controller)`, and wraps each
//! right-clickable element in `context_menu_trigger(child, controller, items)`.

use std::cell::RefCell;
use std::rc::Rc;

use vexo::core::{Logical, Point};
use vexo::signal::Signal;
use vexo::{Component, RenderContext, SimpleState, Widget};

// ============================================================================
// MenuItem
// ============================================================================

/// A single context menu entry.
///
/// `on_select` is `Rc<dyn Fn()>` (not `FnMut`) — it is cloned into each
/// `GestureDetector::on_tap` closure. `Rc` makes cloning cheap and avoids
/// `Send + Sync` bounds that `Arc` would impose (the closures capture
/// single-threaded `Rc`-based controllers).
#[derive(Clone)]
pub struct MenuItem {
    pub label: String,
    pub on_select: Rc<dyn Fn()>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, on_select: Rc<dyn Fn()>) -> Self {
        Self {
            label: label.into(),
            on_select,
        }
    }
}

// ============================================================================
// ContextMenuController
// ============================================================================

/// Controller for a context menu — owns open/close state and the current items.
///
/// Created by the screen's caller (alongside `ScrollController::new()`), held
/// as a field, `.clone()`d into triggers and the host. The `Signal` and
/// `Rc<RefCell>` share underlying state across clones, so widget-struct
/// recreation on rebuild doesn't lose menu state.
///
/// The `Signal` carries only `Option<Point<Logical>>` (position when open,
/// `None` when closed) — not the items. This is because `MenuItem` contains
/// `Rc<dyn Fn()>` which is `!Send + !Sync`, violating `signal_value`'s
/// `T: Send + Sync` bound. Items are stored separately in `Rc<RefCell<...>>`.
#[derive(Clone)]
pub struct ContextMenuController {
    position: Signal<Option<Point<Logical>>>,
    items: Rc<RefCell<Vec<MenuItem>>>,
}

impl ContextMenuController {
    pub fn new() -> Self {
        Self {
            position: Signal::new(None),
            items: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Open the menu at `position` with the given `items`.
    pub fn show(&self, position: Point<Logical>, items: Vec<MenuItem>) {
        *self.items.borrow_mut() = items;
        self.position.set(Some(position));
    }

    /// Close the menu.
    pub fn close(&self) {
        self.position.set(None);
    }

    /// The position signal — read by the `ContextMenu` host via `signal_value`.
    pub fn position_signal(&self) -> &Signal<Option<Point<Logical>>> {
        &self.position
    }

    /// Snapshot the current items (clones the `Vec<MenuItem>` out of the
    /// `RefCell` so the borrow is released immediately). Called by the host
    /// during `render()`.
    pub fn items_snapshot(&self) -> Vec<MenuItem> {
        self.items.borrow().clone()
    }
}

impl Default for ContextMenuController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_show_close() {
        let controller = ContextMenuController::new();
        assert_eq!(controller.position_signal().get(), None);
        assert!(controller.items_snapshot().is_empty());

        let items = vec![
            MenuItem::new("Copy", Rc::new(|| {})),
            MenuItem::new("Delete", Rc::new(|| {})),
        ];
        controller.show(Point::new(100.0, 200.0), items);

        assert_eq!(
            controller.position_signal().get(),
            Some(Point::new(100.0, 200.0))
        );
        assert_eq!(controller.items_snapshot().len(), 2);
        assert_eq!(controller.items_snapshot()[0].label, "Copy");

        controller.close();
        assert_eq!(controller.position_signal().get(), None);
        // Items remain in the cell after close — host doesn't render them
        // because position is None. They'll be overwritten on next show().
    }

    #[test]
    fn test_controller_clone_shares_state() {
        let controller = ContextMenuController::new();
        let cloned = controller.clone();

        controller.show(Point::new(50.0, 60.0), vec![MenuItem::new("A", Rc::new(|| {}))]);

        // The clone sees the same state (shared via Signal's Arc + Rc).
        assert_eq!(cloned.position_signal().get(), Some(Point::new(50.0, 60.0)));
        assert_eq!(cloned.items_snapshot().len(), 1);
    }
}
```

- [ ] **Step 2: Export the module from `vexo_uikit`**

In `vexo_uikit/src/lib.rs`, add (after the `button` module, around line 21):

```rust
pub mod context_menu;
pub use context_menu::{ContextMenuController, MenuItem};
```

- [ ] **Step 3: Run test to verify it fails (compilation)**

Run: `cargo test -p vexo_uikit --lib context_menu::tests`
Expected: FAIL — `vexo::signal::Signal` is not the correct path. Check the actual re-export path.

- [ ] **Step 4: Fix the import path and verify**

Check how `Signal` is imported in other `vexo_uikit` files. In `vexo_uikit/src/button.rs`:
```rust
use vexo::{..., Signal, ...};
```

So `Signal` is re-exported from `vexo` root. Fix the import in `context_menu.rs`:

Change:
```rust
use vexo::signal::Signal;
```
to:
```rust
use vexo::Signal;
```

Run: `cargo test -p vexo_uikit --lib context_menu::tests`
Expected: PASS — both controller tests pass.

- [ ] **Step 5: Run full vexo_uikit test suite**

Run: `cargo test -p vexo_uikit`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/context_menu.rs vexo_uikit/src/lib.rs
git commit -m "feat(vexo_uikit): add MenuItem and ContextMenuController

MenuItem { label, on_select: Rc<dyn Fn()> } — Clone, no Send+Sync bound.
ContextMenuController holds Signal<Option<Point<Logical>>> (position,
Send+Sync) + Rc<RefCell<Vec<MenuItem>>> (items, not in Signal to avoid
Send+Sync bound violation). show/close/items_snapshot API mirrors
ScrollController."
```

---

## Task 6: ContextMenu host widget + menu_view + trigger

**Files:**
- Modify: `vexo_uikit/src/context_menu.rs` (add `ContextMenu` Component, `menu_view`, `context_menu_trigger`)
- Test: `vexo_uikit/src/context_menu.rs` (integration tests with `ThreeTreePipeline`)

**Interfaces:**
- Consumes: `vexo::{Stack, Positioned, DecoratedBox, GestureDetector, Text, WithLayout, Layout, Style, BoxShadow, Color, Theme, Widget, SimpleState, Component, RenderContext}` (all public), `vexo::core::{Logical, Point}`, `vexo::ThreeTreePipeline`, `vexo::animation::AnimationTicker`, `vexo::layout::TaffyLayoutEngine`, `vexo::input::{InputEvent, ButtonState, PointerButton, Modifiers}`, `vexo::ScaleSource`, `vexo::platform::{Clipboard, stub_clipboard::StubClipboard}`, `vexo::resource::new_font_system`.
- Produces: `ContextMenu { controller, child }` (Component), `context_menu_trigger(child, controller, items) -> Box<dyn Widget>`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)]` module in `vexo_uikit/src/context_menu.rs`:

```rust
    use vexo::core::Size;
    use vexo::input::{ButtonState, InputEvent, Modifiers, PointerButton};
    use vexo::layout::TaffyLayoutEngine;
    use vexo::platform::stub_clipboard::StubClipboard;
    use vexo::platform::Clipboard;
    use vexo::render_objects::PositionedRenderObject;
    use vexo::resource::new_font_system;
    use vexo::animation::AnimationTicker;
    use vexo::render_object::RenderObject;
    use vexo::render_objects::TextRenderObject;
    use vexo::RenderObjectKey;
    use vexo::RenderObjectRegistry;
    use vexo::ScaleSource;
    use vexo::Stack;
    use vexo::Text;
    use vexo::Theme;
    use vexo::ThreeTreePipeline;
    use std::sync::Arc;

    fn test_clipboard() -> Arc<dyn Clipboard> {
        Arc::new(StubClipboard)
    }

    fn find_text_in_tree(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        needle: &str,
    ) -> bool {
        let ro = match reg.get(key) {
            Some(ro) => ro,
            None => return false,
        };
        if ro
            .as_any()
            .downcast_ref::<TextRenderObject>()
            .map_or(false, |t| t.content().contains(needle))
        {
            return true;
        }
        for &child in ro.children() {
            if find_text_in_tree(reg, child, needle) {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_host_closed_has_only_content() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // When closed, the render tree should NOT contain the menu items.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            !find_text_in_tree(ro_reg, root, "Copy"),
            "menu item 'Copy' should not be rendered when menu is closed"
        );
    }

    #[test]
    fn test_host_open_renders_menu_at_position() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open the menu at (100, 200).
        controller.show(
            Point::new(100.0, 200.0),
            vec![MenuItem::new("Copy", Rc::new(|| {}))],
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // The menu item text should now appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "menu item 'Copy' should be rendered when menu is open"
        );
    }

    #[test]
    fn test_item_tap_fires_on_select_and_closes() {
        let selected = Rc::new(std::cell::Cell::new(false));
        let selected_clone = selected.clone();

        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            Point::new(10.0, 10.0),
            vec![MenuItem::new("Copy", Rc::new(move || {
                selected_clone.set(true);
            }))],
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Tap the menu item. The item text is at (10, 10) with padding 8 +
        // text size. A click at (15, 15) should be inside the item row.
        // We need to find the actual position via layout — but for a
        // simple test, click at a position within the menu area.
        // The menu is Positioned at (10, 10), the item row starts at (10, 10)
        // in window coords. Text padding is 8px, so clicking at (15, 15)
        // hits the first item row.
        let primary_press = InputEvent::PointerButton {
            position: Point::new(15.0, 15.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let primary_release = InputEvent::PointerButton {
            position: Point::new(15.0, 15.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(15.0, 15.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        pipeline.handle_event(
            Point::new(15.0, 15.0),
            &primary_release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(selected.get(), "on_select should have fired");
        // After the tap, the menu should close (controller.close() called
        // by the item's on_tap closure). The Signal::set triggers a rebuild;
        // perform_rebuilds processes it.
        pipeline.perform_rebuilds();
        assert_eq!(
            controller.position_signal().get(),
            None,
            "menu should be closed after item tap"
        );
    }

    #[test]
    fn test_barrier_dismiss_on_outside_click() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            Point::new(10.0, 10.0),
            vec![MenuItem::new("Copy", Rc::new(|| {}))],
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Click far away from the menu — should hit the barrier and close.
        let primary_press = InputEvent::PointerButton {
            position: Point::new(350.0, 550.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(350.0, 550.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        pipeline.perform_rebuilds();
        assert_eq!(
            controller.position_signal().get(),
            None,
            "menu should be closed after clicking outside (barrier dismiss)"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit --lib context_menu::tests`
Expected: FAIL — `ContextMenu` is not defined yet; compilation error.

- [ ] **Step 3: Implement `ContextMenu` host, `menu_view`, and `context_menu_trigger`**

Add to `vexo_uikit/src/context_menu.rs` (before the `#[cfg(test)]` module), after the `ContextMenuController` impl:

```rust
// ============================================================================
// ContextMenu host (Component)
// ============================================================================

/// A host widget that renders a context menu overlay on top of its child.
///
/// Wrap the screen's root content in `ContextMenu::new(content, controller)`.
/// When `controller.show(pos, items)` is called (e.g. by a
/// `context_menu_trigger` on right-click), the host rebuilds and shows a
/// floating menu at `pos` with a full-size dismiss barrier behind it.
///
/// The host must be OUTSIDE any `ScrollView` (which clips with `overflow:
/// Hidden`). Place it at the screen root so the menu floats above all content.
#[derive(Clone)]
pub struct ContextMenu {
    controller: ContextMenuController,
    child: Box<dyn Widget>,
}

impl ContextMenu {
    pub fn new(child: impl Widget + 'static, controller: ContextMenuController) -> Self {
        Self {
            controller,
            child: Box::new(child),
        }
    }
}

impl Component for ContextMenu {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = vexo::Theme::of(ctx);
        let position = ctx.signal_value(self.controller.position_signal());
        let controller = self.controller.clone();

        let mut stack = vexo::Stack::new().push(self.child.clone_boxed());

        if let Some(pos) = position {
            let items = self.controller.items_snapshot();
            let ctrl_for_barrier = controller.clone();
            let ctrl_for_menu = controller.clone();

            // Child 1: dismiss barrier — full-size transparent press target.
            // Positioned with all insets 0 so it overlaps the content (non-
            // positioned children flow in the Stack's column, they don't
            // overlap). Hit-tested AFTER the menu (reverse order) but BEFORE
            // the content.
            let barrier = vexo::Positioned::new(
                vexo::GestureDetector::new(vexo::Text::new(""))
                    .on_press(move || ctrl_for_barrier.close()),
            )
            .left(0.0)
            .top(0.0)
            .right(0.0)
            .bottom(0.0);

            stack = stack.push(barrier);

            // Child 2: the menu itself, positioned at the click coordinates.
            let menu = menu_view(&items, ctrl_for_menu, &theme);
            let positioned_menu = vexo::Positioned::new(menu)
                .left(pos.x)
                .top(pos.y);
            stack = stack.push(positioned_menu);
        }

        stack.boxed()
    }
}

// ============================================================================
// menu_view — builds the visual menu from items
// ============================================================================

fn menu_view(
    items: &[MenuItem],
    controller: ContextMenuController,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    let column = vexo::column! {
        for item in items {
            let on_select = Rc::clone(&item.on_select);
            let ctrl = controller.clone();
            vexo::GestureDetector::new(
                vexo::WithLayout::new(
                    vexo::Text::new(item.label.as_str()).with_color(theme.on_surface),
                    vexo::Layout::default().padding(8.0).width(160.0),
                ),
            )
            .on_tap(move || {
                on_select();
                ctrl.close();
            })
        }
    };

    vexo::DecoratedBox::with_style(
        vexo::WithLayout::new(column, vexo::Layout::default().min_width(160.0)),
        vexo::Style::default()
            .corner_radius(8.0)
            .background(theme.surface)
            .border(theme.outline, 1.0)
            .shadow(
                vexo::BoxShadow::new(vexo::Color::BLACK.with_alpha(0.25))
                    .blur(6.0)
                    .offset(0.0, 2.0),
            ),
    )
    .boxed()
}

// ============================================================================
// context_menu_trigger — sugar for wrapping a child with right-click detection
// ============================================================================

/// Wrap `child` with a right-click handler that opens the context menu at the
/// cursor position with the given `items`.
///
/// Equivalent to:
/// ```ignore
/// child.on_secondary_press(move |pos| controller.show(pos, items))
/// ```
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    items: Vec<MenuItem>,
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    child.on_secondary_press(move |pos| {
        ctrl.show(pos, items.clone());
    })
}
```

- [ ] **Step 4: Fix compilation errors — check imports**

The `context_menu.rs` file needs imports for all the `vexo::` types used. Update the top imports to:

```rust
use std::cell::RefCell;
use std::rc::Rc;

use vexo::core::{Logical, Point};
use vexo::Signal;
use vexo::{Component, RenderContext, SimpleState, Widget};
```

Add `ContextMenu` to the re-exports in `vexo_uikit/src/lib.rs`:

```rust
pub use context_menu::{ContextMenu, ContextMenuController, MenuItem};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit --lib context_menu`
Expected: PASS — all 6 tests pass (2 controller + 4 host/behavioral).

If the `test_item_tap_fires_on_select_and_closes` test fails because the tap position (15, 15) doesn't hit the item row, adjust the tap position to match the actual layout. The menu is `Positioned` at (10, 10); the item row starts at (10, 10) in window coords with 8px padding. The text itself is at approximately (10+8, 10+8) = (18, 18). Try clicking at (20, 20) or (50, 20) instead.

- [ ] **Step 6: Run full vexo_uikit test suite**

Run: `cargo test -p vexo_uikit`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add vexo_uikit/src/context_menu.rs vexo_uikit/src/lib.rs
git commit -m "feat(vexo_uikit): add ContextMenu host, menu_view, context_menu_trigger

ContextMenu is a Component that wraps content in a Stack. When the
controller's position signal is Some(pos), it mounts a full-size
Positioned dismiss barrier (on_press → close) + a Positioned menu at
the cursor. menu_view renders items as a DecoratedBox column of tappable
rows. context_menu_trigger wraps a child with on_secondary_press."
```

---

## Task 7: Wire ContextMenu into ChatScreen

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (struct, Clone impl, render, existing tests, new tests)
- Test: `shared_app/src/chats/chat_screen.rs` (update 4 existing + add 2 new)

**Interfaces:**
- Consumes: `vexo_uikit::{ContextMenu, ContextMenuController, MenuItem, context_menu_trigger}` — all from Task 5+6.

- [ ] **Step 1: Update `ChatScreen` struct and `Clone` impl**

In `shared_app/src/chats/chat_screen.rs`, add `context_menu` field to the struct (after `scroll_controller`, around line 23):

```rust
pub(crate) struct ChatScreen {
    pub(crate) conv_id: ConvId,
    pub(crate) messages: Signal<Vec<Message>>,
    pub(crate) avatar_bytes: Rc<[u8]>,
    pub(crate) me_avatar_bytes: Rc<[u8]>,
    pub(crate) on_send: Rc<dyn Fn(&str)>,
    pub(crate) scroll_controller: ScrollController,
    pub(crate) context_menu: ContextMenuController,
}
```

Update `Clone for ChatScreen` (around line 26):

```rust
impl Clone for ChatScreen {
    fn clone(&self) -> Self {
        Self {
            conv_id: self.conv_id.clone(),
            messages: self.messages.clone(),
            avatar_bytes: Rc::clone(&self.avatar_bytes),
            me_avatar_bytes: Rc::clone(&self.me_avatar_bytes),
            on_send: Rc::clone(&self.on_send),
            scroll_controller: self.scroll_controller.clone(),
            context_menu: self.context_menu.clone(),
        }
    }
}
```

Add the import at the top (in the `use vexo_uikit::...` line, around line 12):

```rust
use vexo_uikit::{Button, ButtonVariant, ContextMenu, ContextMenuController, KeyboardAvoider, MenuItem};
```

- [ ] **Step 2: Update `render` to wrap root in `ContextMenu` and wrap each bubble**

In `ChatScreen::render` (around line 115-126), wrap each message bubble with `context_menu_trigger`:

Change:
```rust
        let list = column! {
            for msg in &messages {
                build_message_bubble(
                    msg,
                    state.them_avatar(&self.avatar_bytes).clone(),
                    state.me_avatar(&self.me_avatar_bytes).clone(),
                    &theme,
                )
            }
        }
```

to:
```rust
        let ctrl = self.context_menu.clone();
        let list = column! {
            for msg in &messages {
                context_menu_trigger(
                    build_message_bubble(
                        msg,
                        state.them_avatar(&self.avatar_bytes).clone(),
                        state.me_avatar(&self.me_avatar_bytes).clone(),
                        &theme,
                    ),
                    ctrl.clone(),
                    placeholder_menu_items(),
                )
            }
        }
```

Add the `placeholder_menu_items` function (after `build_input_bar`, around line 248):

```rust
fn placeholder_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("Copy", Rc::new(|| log::debug!("context menu: Copy"))),
        MenuItem::new("Reply", Rc::new(|| log::debug!("context menu: Reply"))),
        MenuItem::new("Delete", Rc::new(|| log::debug!("context menu: Delete"))),
    ]
}
```

At the end of `render` (around line 164-168), wrap the return value in `ContextMenu`:

Change:
```rust
        DecoratedBox::with_style(
            KeyboardAvoider::new(content),
            Style::default().background(theme.background),
        )
        .boxed()
```

to:
```rust
        let decorated = DecoratedBox::with_style(
            KeyboardAvoider::new(content),
            Style::default().background(theme.background),
        )
        .boxed();

        ContextMenu::new(decorated, self.context_menu.clone()).render_to_widget()
```

Wait — `ContextMenu` is a `Component`, not a `Widget` directly. `Component` implements `Widget`, so `ContextMenu::new(decorated, self.context_menu.clone())` can be `.boxed()`. Fix:

```rust
        ContextMenu::new(decorated, self.context_menu.clone()).boxed()
```

But `boxed()` requires `Self: Sized + 'static` and `ContextMenu` derives `Clone` — need to check if `Component` auto-implements `Widget`. Yes: `Component` is a sub-trait of `Widget` (see `stateful_widget.rs`). So `ContextMenu::new(...).boxed()` works.

Actually, `render()` must return `Box<dyn Widget>`. `ContextMenu` implements `Widget` (via `Component: Widget`). So:

```rust
        ContextMenu::new(decorated, self.context_menu.clone()).boxed()
```

Replace the entire tail of `render` (the `DecoratedBox::with_style(...)` block) with:

```rust
        let decorated = DecoratedBox::with_style(
            KeyboardAvoider::new(content),
            Style::default().background(theme.background),
        );

        ContextMenu::new(decorated, self.context_menu.clone()).boxed()
```

- [ ] **Step 3: Update existing 4 test constructors**

In each of the 4 existing tests, add `context_menu: ContextMenuController::new()` to the `ChatScreen { ... }` constructor. For example, in `test_chat_screen_renders_messages` (around line 281):

```rust
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
        }
        .boxed();
```

Do the same for:
- `test_chat_screen_reads_live_messages_from_signal` (around line 304)
- `test_chat_screen_input_bar_pinned_to_bottom_with_few_messages` (around line 379)
- `test_chat_screen_input_bar_uses_theme_colors` (around line 448)

- [ ] **Step 4: Write new tests**

Add these tests to the `#[cfg(test)]` module in `chat_screen.rs`:

```rust
    #[test]
    fn test_right_click_bubble_opens_context_menu() {
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
            context_menu: controller.clone(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Before right-click: no "Copy" in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            !find_text_in_tree(ro_reg, root, "Copy"),
            "menu should not be visible before right-click"
        );

        // Right-click at a position inside the first message bubble.
        // The message list is inside a ScrollView with 12px padding.
        // The first bubble starts at approximately (12 + 32 + 8, 12) = (52, 12)
        // (avatar 32px + gap 8px + 12px list padding). Click at (60, 20).
        let secondary_press = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(60.0, 20.0),
            button: vexo::input::PointerButton::Secondary,
            state: vexo::input::ButtonState::Pressed,
        };
        pipeline.handle_event(
            vexo::core::Point::new(60.0, 20.0),
            &secondary_press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::ScaleSource::default(),
            &std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard),
        );
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // After right-click: "Copy" should appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "menu item 'Copy' should appear in render tree after right-clicking a bubble"
        );
    }

    #[test]
    fn test_left_click_bubble_does_not_open_context_menu() {
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
            context_menu: controller.clone(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Left-click at a position inside the first message bubble.
        let primary_press = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(60.0, 20.0),
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Pressed,
        };
        let primary_release = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(60.0, 20.0),
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Released,
        };
        pipeline.handle_event(
            vexo::core::Point::new(60.0, 20.0),
            &primary_press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::ScaleSource::default(),
            &std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard),
        );
        pipeline.handle_event(
            vexo::core::Point::new(60.0, 20.0),
            &primary_release,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::ScaleSource::default(),
            &std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard),
        );
        pipeline.perform_rebuilds();

        assert_eq!(
            controller.position_signal().get(),
            None,
            "left-click should NOT open the context menu"
        );
    }
```

You'll also need to add the `find_text_in_tree` helper to the test module if it's not already there (it exists in the existing tests — check if it's reusable; if it's a nested fn inside another test, copy it to the module level).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p shared_app --lib chats::chat_screen`
Expected: PASS — all 6 tests pass (4 existing + 2 new).

If the right-click test fails because (60, 20) doesn't hit a bubble, adjust the position. The messages are in a ScrollView with 12px padding; the first message is "them" (left-aligned: avatar + bubble). The avatar is 32px wide + 8px gap, so the bubble starts at ~52px x. At y=12 (top padding) the bubble should be hit. Try (60, 20) or (80, 20).

- [ ] **Step 6: Run full shared_app test suite**

Run: `cargo test -p shared_app`
Expected: PASS.

- [ ] **Step 7: Run full workspace test suite**

Run: `cargo test`
Expected: PASS — all crates green.

- [ ] **Step 8: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "feat(shared_app): wire context menu into ChatScreen

Add context_menu: ContextMenuController field. Wrap the screen root in
ContextMenu (outside ScrollView so menu isn't clipped). Wrap each message
bubble in context_menu_trigger with placeholder Copy/Reply/Delete items.
Left-click does not open the menu (arena gated on Primary)."
```

---

## Self-Review

**1. Spec coverage:**
- ✅ winit button mapping (§2.1) → Task 1
- ✅ `GestureDetector::on_secondary_press` (§2.2) → Task 2
- ✅ Arena gating on Primary (§2.3) → Task 4
- ✅ `Widget::on_secondary_press` fluent API (§2.4) → Task 3
- ✅ `MenuItem` (§3.1) → Task 5
- ✅ `ContextMenuController` (§3.3) → Task 5
- ✅ `ContextMenu` host (§3.4) → Task 6
- ✅ `menu_view` (§4.1) → Task 6
- ✅ `context_menu_trigger` (§3.5) → Task 6
- ✅ Dismiss behavior — barrier (§4.2) → Task 6 (barrier test)
- ✅ Dismiss behavior — item tap (§4.2) → Task 6 (item tap test)
- ✅ ChatScreen wiring (§4.3) → Task 7
- ✅ Placeholder items (§4.3) → Task 7
- ✅ Behavior matrix tests (§2) → Task 2 (4 unit tests) + Task 4 (integration)
- ✅ Existing test regression (§5.4) → Task 7 (update 4 constructors)
- ✅ New ChatScreen tests (§5.4) → Task 7 (right-click opens, left-click doesn't)

**2. Placeholder scan:** No "TBD", "TODO", or "implement later" found. All code blocks contain actual implementation. One note: the tap position in `test_item_tap_fires_on_select_and_closes` may need adjustment based on actual layout — the plan explicitly says to adjust if it fails. This is a test-data tuning note, not a placeholder.

**3. Type consistency:**
- `on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>)>>>` — consistent across widget, element, builder, clone, set_widget, rebuild.
- `ContextMenuController` fields: `position: Signal<Option<Point<Logical>>>`, `items: Rc<RefCell<Vec<MenuItem>>>` — consistent across struct, new, show, close, position_signal, items_snapshot.
- `MenuItem { label: String, on_select: Rc<dyn Fn()> }` — consistent.
- `context_menu_trigger(child, controller, items)` — matches usage in Task 7.
- `ContextMenu::new(child, controller)` — matches usage in Task 7.
- `controller.position_signal().get()` returns `Option<Point<Logical>>` — consistent in tests.

No issues found.
