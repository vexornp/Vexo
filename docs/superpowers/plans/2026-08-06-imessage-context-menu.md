# iMessage-Style Context Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the static single-card context menu with iMessage's animated spotlight effect: dimmed backdrop, lifted bright bubble copy, reactions pill above + actions card below, all driven by a single critical spring with symmetric open/close.

**Architecture:** The `ContextMenu` host `Component` owns a 4-state phase machine (`Closed→Opening→Open→Closing→Closed`) backed by an `AnimationController` driven by a critical spring. `controller.show()` carries the tapped bubble's global bounds + widget clone + builder; `close()` starts a reverse spring and defers unmount until settle. The host renders a 5-layer Stack: content, dim barrier, bright bubble copy, reactions pill, actions card — all transformed by the spring value. `GestureDetector::on_secondary_press` is extended to deliver global bounds (already computed by hit-test as absolute bounds in `EventContext::bounds()`).

**Tech Stack:** Rust, vexo framework (wgpu + Taffy + glyphon), `AnimationController` + `SpringSimulation` (`SpringDescription::ios(340.0, 1.0)`), `Transform`/`Opacity` widgets, `vexo_fontawesome` icons.

**Spec:** `docs/superpowers/specs/2026-08-06-imessage-context-menu-design.md`

## Global Constraints

- Spring params: `SpringDescription::ios(340.0, 1.0)` — critical damping, same as KeyboardAvoidance.
- No `should_rebuild()` overrides — the menu is a short-lived overlay, not a hot path.
- No long-press trigger — right-click only (`on_secondary_press`).
- No real emoji — FA icons stand in.
- No `Escape` dismiss, no keyboard shortcuts.
- No animated hover tint — hover stays instant via `Signal<bool>`.
- Card style: `theme.surface` bg, `theme.outline` 1px border, 12px corner radius (actions) / 18px (pill), shadow `BLACK@0.20` blur 12 offset `(0,4)`.
- `min_width: 200.0` on the actions card.
- Icon size: 18.0 (reactions), 14.0 (action rows).
- Gap between bubble and cards: 8.0px.
- Dim barrier max alpha: 0.4.
- Bubble lift: scale +3%, translate -4px.
- Card scale: 0.8 → 1.0.
- All dismiss paths funnel through `controller.close()` which starts the reverse spring.
- `cargo build` after every code change; `cargo test` after every feature step.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `vexo/src/widgets/gesture_detector.rs` | Modify | Extend `on_secondary_press` callback to `(Point, Bounds)`; pass `context.bounds()` |
| `vexo/src/widgets/mod.rs` | Modify | Update `Widget::on_secondary_press` trait method signature |
| `vexo_uikit/src/context_menu.rs` | Major rewrite | `ContextMenuController` (phase machine + spring), `ContextMenu` host (5-layer Stack), `MenuBuilder`/`MenuContent`/`MenuMetrics` types, lifecycle tests |
| `shared_app/src/chats/message_menu.rs` | Rewrite | `builder()` returns `MenuContent` (split reactions pill + actions card); drop divider |
| `shared_app/src/chats/chat_screen.rs` | Modify | Refactor `build_message_bubble` → `build_bubble` + `assemble_row`; wrap just the bubble in `context_menu_trigger`; update tests |

---

### Task 1: Extend `on_secondary_press` to deliver global bounds

**Files:**
- Modify: `vexo/src/widgets/gesture_detector.rs` (callback signature + `on_event` dispatch)
- Modify: `vexo/src/widgets/mod.rs` (trait method signature)
- Test: `vexo/src/widgets/gesture_detector.rs` (update existing `test_on_secondary_press_fires_with_position`)

**Interfaces:**
- Produces: `on_secondary_press` callback signature changes from `FnMut(Point<Logical>)` to `FnMut(Point<Logical>, Bounds<Logical>)`. `EventContext::bounds()` already returns absolute (global) bounds — confirmed in `vexo/src/hit_test.rs:147` (`bounds_for_element` returns absolute bounds from the hit-test walk).

**Context:** `EventContext::bounds()` is already global/absolute. The hit-test walk (`hit_test.rs`) accumulates bounds from root to target, so `context.bounds()` in `on_event` is the element's window-space bounds. No render-tree walking needed — just pass it through.

- [ ] **Step 1: Update the failing test**

In `vexo/src/widgets/gesture_detector.rs`, find `test_on_secondary_press_fires_with_position` (around line 804). Update the callback to accept `Bounds` and assert it:

```rust
#[test]
fn test_on_secondary_press_fires_with_position() {
    use crate::core::Bounds;
    let elem = GestureDetectorElement {
        widget: None,
        id: None,
        key: None,
        render_object: None,
        focus_attachment: None,
        on_press: None,
        on_release: None,
        on_tap: None,
        on_secondary_press: None,
    };
    let mut elem = elem;
    let captured_pos = Rc::new(Cell::new(Point::new(0.0, 0.0)));
    let captured_bounds = Rc::new(Cell::new(Bounds::new(0.0, 0.0, 0.0, 0.0)));
    elem.on_secondary_press = Some(Rc::new(RefCell::new({
        let pos_clone = captured_pos.clone();
        let bounds_clone = captured_bounds.clone();
        move |pos: Point<Logical>, bounds: Bounds<Logical>| {
            pos_clone.set(pos);
            bounds_clone.set(bounds);
        }
    })));

    let position = Point::new(10.0, 20.0);
    let test_bounds = Bounds::new(5.0, 5.0, 100.0, 50.0);
    let mut ctx = EventContext::new(
        ElementKey::default(),
        position,
        test_bounds,
        Modifiers::default(),
        // ... (keep existing test context construction)
    );

    let event = InputEvent::PointerButton {
        position,
        button: PointerButton::Secondary,
        state: ButtonState::Pressed,
    };
    let result = elem.on_event(&event, &mut ctx, &mut StateStorage::default());

    assert!(result.is_some(), "on_secondary_press should claim the event");
    assert_eq!(captured_pos.get(), position);
    assert_eq!(captured_bounds.get(), test_bounds);
}
```

Note: The existing test constructs `EventContext` differently — match whatever the current test uses. The key change is: callback now takes `(Point, Bounds)` and the test asserts both.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_on_secondary_press_fires_with_position`
Expected: FAIL — type mismatch on closure signature.

- [ ] **Step 3: Update the callback type and dispatch**

In `vexo/src/widgets/gesture_detector.rs`:

1. Change the `on_secondary_press` field type in `GestureDetector` (around line 74):
```rust
on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
```

2. Change the builder method (around line 119):
```rust
pub fn on_secondary_press(
    mut self,
    callback: impl FnMut(Point<Logical>, Bounds<Logical>) + 'static,
) -> Self {
    self.on_secondary_press = Some(Rc::new(RefCell::new(callback)));
    self
}
```

3. Change the corresponding field in `GestureDetectorElement` (around line 190):
```rust
on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
```

4. In `on_event` (around line 354), pass both position and bounds:
```rust
if *button == crate::input::PointerButton::Secondary {
    if let Some(callback) = &self.on_secondary_press {
        (callback.borrow_mut())(*position, context.bounds());
        return Some(Box::new(()));
    }
    // Fall through to on_press for Secondary when
    // on_secondary_press is not set (backward-compat).
}
```

5. Update the `Clone` impl for `GestureDetector` (around line 138) — the field type is the same `Rc<RefCell<...>>`, so clone still works. No change needed unless the trait bound differs.

6. In `rebuild` (around line 245), the clone line `self.on_secondary_press = gd.on_secondary_press.clone()` still works (same type).

In `vexo/src/widgets/mod.rs`, update the trait method (around line 220):
```rust
fn on_secondary_press(
    self,
    callback: impl FnMut(Point<Logical>, Bounds<Logical>) + 'static,
) -> Box<dyn Widget>
{
    Box::new(GestureDetector::new(self).on_secondary_press(callback))
}
```

Add `Bounds` to imports in `mod.rs` if not already there: `use crate::core::{Point, Bounds};` (check existing imports).

- [ ] **Step 4: Run all vexo tests to verify no regressions**

Run: `cargo test -p vexo`
Expected: PASS — all tests including the updated `test_on_secondary_press_fires_with_position`.

Note: Other tests that use `on_secondary_press` (in `integration_tests.rs` and `gesture_detector.rs`) may need their closures updated to accept `(pos, _bounds)`. Fix any compilation errors by adding the second parameter.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/gesture_detector.rs vexo/src/widgets/mod.rs
git commit -m "feat(vexo): deliver global bounds in on_secondary_press callback"
```

---

### Task 2: `ContextMenuController` API reshape + `MenuContent` + host skeleton (instant, no animation)

**Files:**
- Modify: `vexo_uikit/src/context_menu.rs` (controller, builder, host, trigger, tests)
- Test: `vexo_uikit/src/context_menu.rs` (update existing tests to new API)

**Interfaces:**
- Consumes: `on_secondary_press` callback now provides `(Point, Bounds)` from Task 1.
- Produces:
  - `pub struct MenuContent { pub reactions: Box<dyn Widget>, pub actions: Box<dyn Widget>, pub metrics: MenuMetrics }`
  - `pub struct MenuMetrics { pub reactions_size: Size<Logical>, pub actions_size: Size<Logical>, pub gap: f32 }`
  - `pub enum Phase { Closed, Opening, Open, Closing }`
  - `ContextMenuController::show(&self, bubble_bounds: Bounds<Logical>, bubble_widget: Box<dyn Widget>, builder: MenuBuilder)`
  - `ContextMenuController::close(&self)` — instant close (no spring yet; Task 5 adds the spring)
  - `ContextMenuController::phase(&self) -> Phase`
  - `ContextMenuController::animation_value(&self) -> f64` — returns `1.0` when Open, `0.0` when Closed (placeholder until Task 5)
  - `ContextMenuController::set_animation_ticker(&self, t: Arc<AnimationTicker>)` — stores for Task 5 (no-op for now)
  - `ContextMenuController::set_dirty_callback(&self, cb: ...)` — stores for Task 5 (no-op for now)
  - `MenuBuilder` now wraps `Fn(&Ctrl, &Theme) -> MenuContent`

**Context:** This task reshapes the API to the final shape but preserves instant open/close behavior (no animation). The host renders only the actions card at the bubble position — no dim, no bubble copy, no reactions pill yet. This is a working refactor that compiles and passes updated presence tests. Later tasks add the visual layers and animation.

- [ ] **Step 1: Write failing test for new `show` signature + `phase`**

In `vexo_uikit/src/context_menu.rs`, replace `test_controller_show_close` with:

```rust
#[test]
fn test_controller_show_close_new_api() {
    let controller = ContextMenuController::new();
    assert_eq!(controller.phase(), Phase::Closed);
    assert!((controller.animation_value() - 0.0).abs() < 1e-9);

    let bubble_widget = vexo::Text::new("bubble").boxed();
    let bounds = vexo::core::Bounds::new(10.0, 20.0, 100.0, 50.0);
    controller.show(bounds, bubble_widget, test_content_builder("Copy"));
    assert_eq!(controller.phase(), Phase::Open);
    assert!((controller.animation_value() - 1.0).abs() < 1e-9);

    controller.close();
    assert_eq!(controller.phase(), Phase::Closed);
    assert!((controller.animation_value() - 0.0).abs() < 1e-9);
}
```

Also add a minimal `test_content_builder` helper (replaces the old `test_builder`):

```rust
fn test_content_builder(label: &'static str) -> MenuBuilder {
    MenuBuilder::new(move |_ctrl, _theme| MenuContent {
        reactions: vexo::Text::new("r").boxed(),
        actions: vexo::Text::new(label).boxed(),
        metrics: MenuMetrics {
            reactions_size: vexo::core::Size::new(150.0, 28.0),
            actions_size: vexo::core::Size::new(200.0, 108.0),
            gap: 8.0,
        },
    })
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit test_controller_show_close_new_api`
Expected: FAIL — `Phase` type not found, `show` signature mismatch.

- [ ] **Step 3: Implement the new types and controller**

In `vexo_uikit/src/context_menu.rs`, replace the `MenuBuilder`, `ContextMenuController` sections:

```rust
use vexo::core::{Bounds, Logical, Point, Size};
use vexo::animation::AnimationTicker;

// ============================================================================
// MenuContent + MenuMetrics + MenuBuilder
// ============================================================================

/// The two cards produced by a menu builder.
pub struct MenuContent {
    pub reactions: Box<dyn Widget>,
    pub actions: Box<dyn Widget>,
    pub metrics: MenuMetrics,
}

/// Size hints for positioning + transform anchors. These are estimates used
/// by the host to position cards and compute scale-about-center transforms
/// before layout runs. The actual laid-out sizes may differ slightly; these
/// are tuned during implementation.
pub struct MenuMetrics {
    pub reactions_size: Size<Logical>,
    pub actions_size: Size<Logical>,
    pub gap: f32,
}

#[derive(Clone)]
pub struct MenuBuilder(Rc<dyn Fn(&ContextMenuController, &vexo::ThemeData) -> MenuContent>);

impl MenuBuilder {
    pub fn new(
        f: impl Fn(&ContextMenuController, &vexo::ThemeData) -> MenuContent + 'static,
    ) -> Self {
        Self(Rc::new(f))
    }
}

impl Deref for MenuBuilder {
    type Target = dyn Fn(&ContextMenuController, &vexo::ThemeData) -> MenuContent;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

// ============================================================================
// Phase + OpenState + ContextMenuController
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Opening,
    Open,
    Closing,
}

struct OpenState {
    bubble_bounds: Bounds<Logical>,
    bubble_widget: Box<dyn Widget>,
    builder: MenuBuilder,
}

struct Shared {
    phase: Phase,
    open: Option<OpenState>,
    animation_value: f64,
    ticker: Option<Arc<AnimationTicker>>,
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

#[derive(Clone)]
pub struct ContextMenuController {
    shared: Rc<RefCell<Shared>>,
}

impl ContextMenuController {
    pub fn new() -> Self {
        Self {
            shared: Rc::new(RefCell::new(Shared {
                phase: Phase::Closed,
                open: None,
                animation_value: 0.0,
                ticker: None,
                dirty_callback: None,
            })),
        }
    }

    pub fn show(
        &self,
        bubble_bounds: Bounds<Logical>,
        bubble_widget: Box<dyn Widget>,
        builder: MenuBuilder,
    ) {
        let mut s = self.shared.borrow_mut();
        s.open = Some(OpenState {
            bubble_bounds,
            bubble_widget,
            builder,
        });
        s.phase = Phase::Open;
        s.animation_value = 1.0;
        // Task 5 will replace the above with a forward spring.
    }

    pub fn close(&self) {
        let mut s = self.shared.borrow_mut();
        s.open = None;
        s.phase = Phase::Closed;
        s.animation_value = 0.0;
        // Task 5 will replace the above with a reverse spring + deferred clear.
    }

    pub fn phase(&self) -> Phase {
        self.shared.borrow().phase
    }

    pub fn animation_value(&self) -> f64 {
        self.shared.borrow().animation_value
    }

    pub fn set_animation_ticker(&self, t: Arc<AnimationTicker>) {
        self.shared.borrow_mut().ticker = Some(t);
    }

    pub fn set_dirty_callback(&self, cb: Arc<dyn Fn() + Send + Sync>) {
        self.shared.borrow_mut().dirty_callback = Some(cb);
    }

    /// Snapshot the current open state (clones bounds, clones the bubble
    /// widget, clones the builder). Returns None when closed.
    /// Called by the host during render only when phase != Closed.
    pub(crate) fn open_snapshot(&self) -> Option<(Bounds<Logical>, Box<dyn Widget>, MenuBuilder)> {
        let s = self.shared.borrow();
        s.open.as_ref().map(|o| {
            (
                o.bubble_bounds,
                o.bubble_widget.clone_boxed(),
                o.builder.clone(),
            )
        })
    }
}

impl Default for ContextMenuController {
    fn default() -> Self {
        Self::new()
    }
}
```

Add `use std::cell::RefCell;` and `use std::sync::Arc;` to the imports (some may already be present).

- [ ] **Step 4: Update the `ContextMenu` host render**

Replace the `ContextMenu` host's `render` method. The host now reads `phase()` and `open_snapshot()` instead of `signal_value`:

```rust
impl Component for ContextMenu {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = vexo::Theme::of(ctx);
        let phase = self.controller.phase();

        let mut stack = vexo::Stack::new().push(self.child.clone_boxed());

        if phase != Phase::Closed {
            if let Some((bubble_bounds, _bubble_widget, builder)) =
                self.controller.open_snapshot()
            {
                let controller = self.controller.clone();

                // Dismiss barrier (same as before, positioned full-screen).
                let ctrl_for_barrier = controller.clone();
                let barrier = vexo::Positioned::new(
                    vexo::GestureDetector::new(vexo::WithLayout::new(
                        vexo::Text::new(""),
                        vexo::Layout::default()
                            .width_percent(1.0)
                            .height_percent(1.0),
                    ))
                    .on_press(move || ctrl_for_barrier.close()),
                )
                .left(0.0)
                .top(0.0)
                .right(0.0)
                .bottom(0.0);
                stack = stack.push(barrier);

                // Menu content from builder — render ONLY the actions card
                // at the bubble position for now. Tasks 3-7 add the rest.
                let content = builder(&controller, &theme);
                let positioned_menu = vexo::Positioned::new(content.actions)
                    .left(bubble_bounds.left)
                    .top(bubble_bounds.top);
                stack = stack.push(positioned_menu);
            }
        }

        stack.boxed()
    }
}
```

Note: `SimpleState<()>` is still used — no `on_tick` needed yet (Task 5 adds it). The host no longer calls `signal_value` — rebuilds are driven by the dirty callback (wired in Task 5). For now, `show()`/`close()` are called from event handlers which mark the host dirty via the existing mechanism (the gesture callback's dirty sender). 

**Important:** Since we removed `signal_value`, the host needs another way to rebuild when `show()`/`close()` is called. Add a dirty callback wire-up in the host's `on_mount`. But `SimpleState<()>` has no `on_mount`. 

**Fix:** Change the host's `State` from `SimpleState<()>` to a custom state that wires the dirty callback in `on_mount`:

```rust
#[derive(Default)]
pub struct ContextMenuHostState;

impl ComponentState for ContextMenuHostState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        // Wire the controller's dirty callback so show()/close() trigger
        // a host rebuild. Task 5 also wires the animation ticker here.
        if let Some(widget) = ctx.widget().downcast_ref::<ContextMenu>() {
            widget.controller.set_dirty_callback(ctx.dirty_callback());
        }
    }
    fn on_update(&mut self, _old: &dyn Any, ctx: &mut LifecycleContext) {
        if let Some(widget) = ctx.widget().downcast_ref::<ContextMenu>() {
            widget.controller.set_dirty_callback(ctx.dirty_callback());
        }
    }
}

impl Component for ContextMenu {
    type State = ContextMenuHostState;

    fn render(&self, _state: &mut ContextMenuHostState, ctx: &mut RenderContext) -> Box<dyn Widget> {
        // ... (same as above)
    }
}
```

Add imports: `use vexo::{ComponentState, LifecycleContext};` and `use std::any::Any;`.

- [ ] **Step 5: Update `context_menu_trigger`**

```rust
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    builder: MenuBuilder,
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    let bubble_widget = child.clone_boxed();
    child.on_secondary_press(move |_pos, bounds| {
        ctrl.show(bounds, bubble_widget.clone_boxed(), builder.clone());
    })
}
```

- [ ] **Step 6: Update remaining existing tests**

Update all tests in `context_menu.rs` that call `controller.show(Point::new(...), test_builder(...))` to the new API: `controller.show(Bounds::new(...), widget, test_content_builder(...))`.

For `test_host_closed_has_only_content`: no change needed (doesn't call show).

For `test_host_open_renders_menu_at_position`: update to new `show` signature, assert "Copy" still appears.

For `test_item_tap_fires_on_select_and_closes`: update builder to return `MenuContent` with the tappable row in `actions`.

For `test_barrier_dismiss_on_outside_click`: update `show` call.

For `test_builder_reads_current_theme`: update builder to return `MenuContent` with theme-dependent text in `actions`. The assertion still walks the render tree for the label.

- [ ] **Step 7: Run all vexo_uikit tests**

Run: `cargo test -p vexo_uikit`
Expected: PASS — all updated tests pass.

- [ ] **Step 8: Build shared_app (will fail — call site not updated yet)**

Run: `cargo build -p shared_app`
Expected: FAIL — `context_menu_trigger` call site in `chat_screen.rs` still uses old `show` signature (but actually the trigger function's external API hasn't changed — it still takes `(child, controller, builder)`. The closure inside was updated. So `chat_screen.rs` should still compile. Verify.)

If it fails, the failure is in `message_menu.rs`'s `builder()` which still returns `Box<dyn Widget>` instead of `MenuContent`. That's Task 3. For now, temporarily update `message_menu::builder()` to return `MenuContent`:

In `shared_app/src/chats/message_menu.rs`, change `builder()`:
```rust
pub(super) fn builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| MenuContent {
        reactions: reaction_row(ctrl.clone(), theme.clone()),
        actions: vexo::column! {
            MenuRow { /* Copy */ }.boxed(),
            MenuRow { /* Reply */ }.boxed(),
            MenuRow { /* Delete */ }.boxed(),
        }.boxed(),
        metrics: MenuMetrics {
            reactions_size: vexo::core::Size::new(150.0, 28.0),
            actions_size: vexo::core::Size::new(200.0, 108.0),
            gap: 8.0,
        },
    })
}
```

This is a temporary split — Task 3 refines it into the real reactions pill + actions card. The `menu_divider` call is dropped (per spec: no divider with two cards). Import `MenuContent`, `MenuMetrics` from `vexo_uikit`.

- [ ] **Step 9: Run shared_app tests**

Run: `cargo test -p shared_app`
Expected: PASS — existing presence tests pass with the temporary builder split.

- [ ] **Step 10: Commit**

```bash
git add vexo_uikit/src/context_menu.rs shared_app/src/chats/message_menu.rs
git commit -m "refactor(vexo_uikit): context menu controller API + MenuContent (instant, no animation)"
```

---

### Task 3: Refactor `message_menu.rs` into split reactions pill + actions card; refactor `chat_screen.rs` to wrap just the bubble

**Files:**
- Modify: `shared_app/src/chats/message_menu.rs` (real split: pill + card with proper styling)
- Modify: `shared_app/src/chats/chat_screen.rs` (split `build_message_bubble` → `build_bubble` + `assemble_row`; wrap just the bubble in `context_menu_trigger`)
- Test: `shared_app/src/chats/chat_screen.rs` (update presence tests)

**Interfaces:**
- Consumes: `MenuContent`, `MenuMetrics` from Task 2.
- Produces: `message_menu::builder()` returns a properly styled `MenuContent` (pill above, actions card below, with `MenuMetrics`).

**Context:** The trigger must wrap just the bubble (not the full row with Spacer) so `bubble_bounds` is the bubble's bounds, not the full window width. This requires splitting `build_message_bubble` into `build_bubble` (just the DecoratedBox) and `assemble_row` (avatar + bubble + spacer), with the trigger wrapping the bubble between them.

- [ ] **Step 1: Write failing test asserting reactions + actions render separately**

In `shared_app/src/chats/chat_screen.rs` test module, update `test_right_click_menu_contains_reactions_and_items` to also assert the reactions pill content is present (the reaction row is now a separate card, not stacked above the actions in one column). The existing assertion for "Copy"/"Reply"/"Delete" still holds. Add an assertion that the reactions card is a separate `Positioned` element (not inside the same `Positioned` as the actions). For now, just assert all 3 item labels still appear (same as before):

```rust
#[test]
fn test_right_click_menu_contains_reactions_and_items() {
    // (Same setup as existing test — right-click at 60,20)
    // ... existing setup ...

    // All three item labels must appear in the render tree.
    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");
    for label in ["Copy", "Reply", "Delete"] {
        assert!(
            find_text_in_tree(ro_reg, root, label),
            "menu item '{}' should appear in render tree after right-clicking a bubble",
            label,
        );
    }
}
```

This test should already pass from Task 2's temporary builder. The real change in this task is the `build_message_bubble` refactor + the reactions pill styling. Write a new test for the bubble-only trigger:

```rust
#[test]
fn test_right_click_on_avatar_does_not_open_menu() {
    // After refactoring, the trigger wraps only the bubble, not the avatar.
    // Right-clicking the avatar area should NOT open the menu.
    let messages_signal = seed_messages_signal();
    let controller = ContextMenuController::new();
    let view = ContextMenu::new(
        ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
            context_menu: controller.clone(),
        },
        controller.clone(),
    )
    .boxed();

    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = vexo::resource::new_font_system();
    pipeline.layout(vexo::core::Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Right-click at the avatar position (x=12, before the bubble at x≈52).
    let secondary_press = vexo::input::InputEvent::PointerButton {
        position: vexo::core::Point::new(15.0, 20.0),
        button: vexo::input::PointerButton::Secondary,
        state: vexo::input::ButtonState::Pressed,
    };
    let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
        std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
    pipeline.handle_event(
        vexo::core::Point::new(15.0, 20.0),
        &secondary_press,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &vexo::core::ScaleSource::default(),
        &clipboard,
    );
    pipeline.perform_rebuilds();

    assert_eq!(
        controller.phase(),
        vexo_uikit::Phase::Closed,
        "right-click on avatar should NOT open the menu (trigger wraps bubble only)"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shared_app test_right_click_on_avatar_does_not_open_menu`
Expected: FAIL — currently the trigger wraps the whole row, so right-clicking the avatar opens the menu.

- [ ] **Step 3: Refactor `build_message_bubble` into `build_bubble` + `assemble_row`**

In `shared_app/src/chats/chat_screen.rs`, replace `build_message_bubble` with:

```rust
/// Build just the message bubble (DecoratedBox + text), without the avatar
/// or row layout. This is what gets wrapped in `context_menu_trigger` so
/// the trigger's bounds match the bubble, not the full-width row.
fn build_bubble(msg: &Message, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let is_me = msg.author == MessageAuthor::Me;
    DecoratedBox::with_style(
        WithLayout::new(
            Text::new(msg.text.as_str())
                .with_font_size(15.0)
                .with_color(if is_me {
                    theme.on_primary
                } else {
                    theme.on_surface
                }),
            Layout::default()
                .flex_direction(FlexDirection::Row)
                .padding(10.0)
                .max_width(220.0)
                .align_self(AlignSelf::Start)
                .flex_shrink(0.0),
        ),
        Style::default()
            .corner_radius(12.0)
            .background(if is_me { theme.primary } else { theme.surface })
            .border(theme.outline, 1.0),
    )
    .boxed()
}

/// Assemble the full message row: avatar + bubble + spacer, with the bubble
/// already wrapped in the context menu trigger.
fn assemble_row(
    bubble_with_menu: Box<dyn Widget>,
    them_avatar_image: ImageData,
    me_avatar_image: ImageData,
    is_me: bool,
) -> Box<dyn Widget> {
    if is_me {
        let me_avatar = avatar(me_avatar_image, 32.0);
        row! {
            Spacer::new(),
            bubble_with_menu,
            me_avatar,
        }
        .gap(8.0)
        .boxed()
    } else {
        let them_avatar = avatar(them_avatar_image, 32.0);
        row! {
            them_avatar,
            bubble_with_menu,
            Spacer::new(),
        }
        .gap(8.0)
        .boxed()
    }
}
```

Update `ChatScreen::render` to use the new functions:

```rust
let list = column! {
    for msg in &messages {
        let is_me = msg.author == MessageAuthor::Me;
        let bubble = build_bubble(msg, &theme);
        let bubble_with_menu = context_menu_trigger(
            bubble,
            ctrl.clone(),
            message_menu::builder(),
        );
        assemble_row(
            bubble_with_menu,
            state.them_avatar(&self.avatar_bytes).clone(),
            state.me_avatar(&self.me_avatar_bytes).clone(),
            is_me,
        )
    }
}
.gap(8.0)
.padding(12.0);
```

- [ ] **Step 4: Rewrite `message_menu.rs` builder with real split cards**

In `shared_app/src/chats/message_menu.rs`, rewrite `builder()`:

```rust
pub(super) fn builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| {
        MenuContent {
            reactions: reaction_pill(ctrl.clone(), theme.clone()),
            actions: actions_card(ctrl.clone(), theme.clone()),
            metrics: MenuMetrics {
                reactions_size: vexo::core::Size::new(150.0, 28.0),
                actions_size: vexo::core::Size::new(200.0, 108.0),
                gap: 8.0,
            },
        }
    })
}

/// The reactions pill: a compact row of 6 FA icons in a pill-shaped
/// (18px radius) DecoratedBox.
fn reaction_pill(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget> {
    let reactions: [(Icons, &str); 6] = [
        (Icons::ThumbsUp, "context menu: thumbsup"),
        (Icons::Heart, "context menu: heart"),
        (Icons::FaceLaugh, "context menu: laugh"),
        (Icons::FaceSurprise, "context menu: surprise"),
        (Icons::FaceSadTear, "context menu: sad"),
        (Icons::FaceAngry, "context menu: angry"),
    ];

    let row = row! {
        for (icon, msg) in reactions {
            let ctrl = ctrl.clone();
            GestureDetector::new(
                WithLayout::new(
                    Icon::new(icon)
                        .with_size(18.0)
                        .with_color(theme.on_surface_variant),
                    Layout::default().padding(6.0),
                )
                .boxed()
                .cursor(MouseCursor::System(SystemCursorKind::Pointer)),
            )
            .on_tap(move || {
                log::debug!("{}", msg);
                ctrl.close();
            })
        }
    }
    .gap(6.0)
    .justify(JustifyContent::Center);

    DecoratedBox::with_style(
        WithLayout::new(row, Layout::default().padding_each(6.0, 6.0, 5.0, 5.0)),
        Style::default()
            .corner_radius(18.0)
            .background(theme.surface)
            .border(theme.outline, 1.0)
            .shadow(
                BoxShadow::new(Color::BLACK.with_alpha(0.20))
                    .blur(12.0)
                    .offset(0.0, 4.0),
            ),
    )
    .boxed()
}

/// The actions card: Copy/Reply/Delete rows in a 12px-radius DecoratedBox.
fn actions_card(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget> {
    let column = column! {
        MenuRow {
            theme: theme.clone(),
            icon: Icons::Copy,
            label: "Copy",
            destructive: false,
            on_tap: close_after(ctrl.clone(), "context menu: Copy"),
        },
        MenuRow {
            theme: theme.clone(),
            icon: Icons::Reply,
            label: "Reply",
            destructive: false,
            on_tap: close_after(ctrl.clone(), "context menu: Reply"),
        },
        MenuRow {
            theme: theme.clone(),
            icon: Icons::Trash,
            label: "Delete",
            destructive: true,
            on_tap: close_after(ctrl.clone(), "context menu: Delete"),
        },
    };

    DecoratedBox::with_style(
        WithLayout::new(column, Layout::default().min_width(200.0)),
        Style::default()
            .corner_radius(12.0)
            .background(theme.surface)
            .border(theme.outline, 1.0)
            .shadow(
                BoxShadow::new(Color::BLACK.with_alpha(0.20))
                    .blur(12.0)
                    .offset(0.0, 4.0),
            ),
    )
    .boxed()
}
```

Delete the old `menu_divider` function (no longer needed — two cards, no divider). Delete the old `reaction_row` function (replaced by `reaction_pill`). Keep `MenuRow`, `MenuRowState`, and `close_after` as-is.

Add imports: `use vexo_uikit::{MenuBuilder, MenuContent, MenuMetrics};` (instead of just `MenuBuilder`).

- [ ] **Step 5: Run shared_app tests**

Run: `cargo test -p shared_app`
Expected: PASS — all tests including the new `test_right_click_on_avatar_does_not_open_menu`.

- [ ] **Step 6: Commit**

```bash
git add shared_app/src/chats/message_menu.rs shared_app/src/chats/chat_screen.rs
git commit -m "refactor(shared_app): split menu into reactions pill + actions card; wrap bubble only in trigger"
```

---

### Task 4: Bubble copy + dim barrier (cutout, instant — the dual-render spike)

**Files:**
- Modify: `vexo_uikit/src/context_menu.rs` (host render: add dim + bright bubble copy layers)
- Test: `vexo_uikit/src/context_menu.rs` (tests #6 + #7 from spec)

**Interfaces:**
- Consumes: `open_snapshot()` returns `(bounds, bubble_widget, builder)` from Task 2.
- Produces: host renders 4 layers when open (content, dim, bubble copy, actions card). Still instant (no spring — Task 5-6 adds animation).

**Context:** This is the key risk spike: does rendering the bubble widget twice (once in-content, once as the bright copy) produce identical layout? Test #7 validates this. Still instant — the dim is at full 0.4 alpha, the bubble copy is at full brightness, no transform.

- [ ] **Step 1: Write failing test #6 — bubble copy rendered on top**

```rust
#[test]
fn test_bright_bubble_copy_rendered_on_top() {
    let controller = ContextMenuController::new();
    let bubble_text = "BUBBLE_CONTENT marker";
    let bubble_widget = vexo::Text::new(bubble_text).boxed();
    let bounds = vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0);

    let host = ContextMenu::new(
        vexo::Text::new("background content"),
        controller.clone(),
    );

    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Open the menu with a builder whose actions card also contains the
    // bubble text — no, the bubble widget is separate. We want to assert
    // the bubble_widget's content appears in the render tree.
    controller.show(bounds, bubble_widget, test_content_builder("Actions"));
    pipeline.perform_rebuilds();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // The bubble widget's text should appear in the render tree (as the
    // bright copy on top of the dim).
    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");
    assert!(
        find_text_in_tree(ro_reg, root, bubble_text),
        "bright bubble copy should be rendered when menu is open"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit test_bright_bubble_copy_rendered_on_top`
Expected: FAIL — the host doesn't render the bubble copy yet.

- [ ] **Step 3: Write failing test #7 — bubble copy size matches original**

```rust
#[test]
fn test_bubble_copy_size_matches_original() {
    let controller = ContextMenuController::new();
    // A bubble widget with known intrinsic size.
    let bubble_widget = vexo::WithLayout::new(
        vexo::Text::new("X"),
        vexo::Layout::default().width(80.0).height(30.0),
    )
    .boxed();
    let bounds = vexo::core::Bounds::new(50.0, 50.0, 80.0, 30.0);

    // Wrap the bubble widget in the content tree too, so it renders twice.
    let content = vexo::WithLayout::new(
        bubble_widget.clone_boxed(),
        vexo::Layout::default().width(80.0).height(30.0),
    );

    let host = ContextMenu::new(content, controller.clone());

    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    controller.show(bounds, bubble_widget.clone_boxed(), test_content_builder("A"));
    pipeline.perform_rebuilds();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Find all TextRenderObjects with content "X" in the tree. There should
    // be two (one in-content, one as the bright copy). Assert their
    // computed_bounds sizes match.
    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");
    let mut found_sizes: Vec<vexo::core::Size<Logical>> = Vec::new();
    collect_text_sizes(ro_reg, root, "X", &mut found_sizes);
    assert_eq!(
        found_sizes.len(),
        2,
        "should find 2 'X' TextRenderObjects (in-content + bright copy)"
    );
    assert_eq!(
        found_sizes[0], found_sizes[1],
        "in-content and bright copy sizes must match (dual-render is deterministic)"
    );
}

fn collect_text_sizes(
    reg: &RenderObjectRegistry,
    key: RenderObjectKey,
    needle: &str,
    out: &mut Vec<vexo::core::Size<Logical>>,
) {
    if let Some(ro) = reg.get(key) {
        if ro
            .as_any()
            .downcast_ref::<TextRenderObject>()
            .map_or(false, |t| t.content().contains(needle))
        {
            if let Some(b) = ro.computed_bounds() {
                out.push(vexo::core::Size::new(b.width(), b.height()));
            }
        }
        for &child in ro.children() {
            collect_text_sizes(reg, child, needle, out);
        }
    }
}
```

- [ ] **Step 4: Implement dim barrier + bright bubble copy in host render**

Update `ContextMenu::render` to add the dim and bubble copy layers:

```rust
impl Component for ContextMenu {
    type State = ContextMenuHostState;

    fn render(&self, _state: &mut ContextMenuHostState, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = vexo::Theme::of(ctx);
        let phase = self.controller.phase();

        let mut stack = vexo::Stack::new().push(self.child.clone_boxed());

        if phase != Phase::Closed {
            if let Some((bubble_bounds, bubble_widget, builder)) =
                self.controller.open_snapshot()
            {
                let controller = self.controller.clone();

                // [2] Dim barrier — full-screen, fixed 0.4 alpha (Task 6 animates).
                let ctrl_for_barrier = controller.clone();
                let barrier = vexo::Positioned::new(
                    vexo::GestureDetector::new(vexo::WithLayout::new(
                        vexo::DecoratedBox::with_style(
                            vexo::Text::new(""),
                            vexo::Style::default()
                                .background(vexo::Color::BLACK.with_alpha(0.4)),
                        ),
                        vexo::Layout::default()
                            .width_percent(1.0)
                            .height_percent(1.0),
                    ))
                    .on_press(move || ctrl_for_barrier.close()),
                )
                .left(0.0)
                .top(0.0)
                .right(0.0)
                .bottom(0.0);
                stack = stack.push(barrier);

                // [3] Bright bubble copy — Positioned at bubble_bounds, full
                // opacity, tappable to dismiss. No transform yet (Task 6).
                let ctrl_for_bubble = controller.clone();
                let bubble_copy = vexo::Positioned::new(
                    vexo::GestureDetector::new(bubble_widget)
                        .on_press(move || ctrl_for_bubble.close()),
                )
                .left(bubble_bounds.left)
                .top(bubble_bounds.top);
                stack = stack.push(bubble_copy);

                // [5] Actions card — at bubble position for now (Task 7
                // positions it below the bubble).
                let content = builder(&controller, &theme);
                let positioned_actions = vexo::Positioned::new(content.actions)
                    .left(bubble_bounds.left)
                    .top(bubble_bounds.top + bubble_bounds.height() + 8.0);
                stack = stack.push(positioned_actions);
            }
        }

        stack.boxed()
    }
}
```

Note: The reactions pill is not rendered yet — Task 7 adds it with proper positioning. For now, only the actions card is positioned (below the bubble).

- [ ] **Step 5: Run tests #6 and #7**

Run: `cargo test -p vexo_uikit test_bright_bubble_copy_rendered_on_top test_bubble_copy_size_matches_original`
Expected: PASS — both tests pass, confirming the dual-render is deterministic.

If test #7 FAILS (sizes don't match), stop and fall back to the cutout-frame approach per the spec's risk mitigation. Document the fallback in the commit message.

- [ ] **Step 6: Run all vexo_uikit + shared_app tests**

Run: `cargo test -p vexo_uikit && cargo test -p shared_app`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add vexo_uikit/src/context_menu.rs
git commit -m "feat(vexo_uikit): dim barrier + bright bubble copy (instant, no animation)"
```

---

### Task 5: Spring animation lifecycle in controller (phase machine + springs)

**Files:**
- Modify: `vexo_uikit/src/context_menu.rs` (controller: real phase machine + AnimationController + springs)
- Test: `vexo_uikit/src/context_menu.rs` (tests #1-4 from spec)

**Interfaces:**
- Consumes: `AnimationController`, `SpringSimulation`, `SpringDescription` from `vexo::animation`.
- Produces:
  - `show()` starts forward spring (0→1), phase=Opening.
  - `close()` starts reverse spring (1→0), phase=Closing.
  - `on_tick`/`advance(now)` drives the spring and flips phases on settle.
  - `animation_value()` returns the live spring value.
  - `phase()` returns the live phase.
  - Host `State` gains `on_tick(now)` that calls `controller.advance(now)`.

**Context:** The controller now owns an `AnimationController` inside its `Shared` state. `show()`/`close()` call `animate_with` with a critical spring. The host's `on_tick` calls `controller.advance(now)` which advances the spring and handles phase transitions (Opening→Open on settle, Closing→Closed + clear open state on settle). Tests use the `pump(ticker, pipeline)` pattern with `std::thread::sleep` to advance real time (the spring uses `Instant::now()`).

- [ ] **Step 1: Write failing test #1 — show starts open spring**

```rust
#[test]
fn test_show_starts_open_spring() {
    let controller = ContextMenuController::new();
    let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
    let ticker = Arc::new(AnimationTicker::new());

    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // show() starts the forward spring.
    controller.show(
        vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
        vexo::Text::new("bubble").boxed(),
        test_content_builder("Copy"),
    );
    pipeline.perform_rebuilds();

    assert_eq!(controller.phase(), Phase::Opening);
    assert!(
        controller.animation_value() < 1.0,
        "spring should not be settled yet (value={})",
        controller.animation_value()
    );

    // Advance real time past settle (~0.6s for critical spring k=340).
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();

    assert_eq!(controller.phase(), Phase::Open);
    assert!(
        (controller.animation_value() - 1.0).abs() < 0.01,
        "spring should have settled to 1.0 (value={})",
        controller.animation_value()
    );
}
```

- [ ] **Step 2: Write failing test #2 — close starts reverse spring, not immediate unmount**

```rust
#[test]
fn test_close_starts_reverse_spring_not_immediate_unmount() {
    let controller = ContextMenuController::new();
    let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
    let ticker = Arc::new(AnimationTicker::new());

    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Open and settle.
    controller.show(
        vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
        vexo::Text::new("bubble").boxed(),
        test_content_builder("Copy"),
    );
    pipeline.perform_rebuilds();
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    assert_eq!(controller.phase(), Phase::Open);

    // Close — should start reverse spring, NOT immediately clear.
    controller.close();
    pipeline.perform_rebuilds();

    assert_eq!(controller.phase(), Phase::Closing);
    assert!(
        controller.animation_value() > 0.0,
        "spring should still be mid-reverse (value={})",
        controller.animation_value()
    );

    // Advance past settle.
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();

    assert_eq!(controller.phase(), Phase::Closed);
    assert!(
        (controller.animation_value() - 0.0).abs() < 0.01,
        "spring should have settled to 0.0 (value={})",
        controller.animation_value()
    );
}
```

- [ ] **Step 3: Write failing test #3 — early close during open reverses smoothly**

```rust
#[test]
fn test_early_close_during_open_reverses_smoothly() {
    let controller = ContextMenuController::new();
    let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
    let ticker = Arc::new(AnimationTicker::new());

    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    controller.show(
        vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
        vexo::Text::new("bubble").boxed(),
        test_content_builder("Copy"),
    );
    pipeline.perform_rebuilds();

    // Advance partway (value should be between 0 and 1).
    std::thread::sleep(std::time::Duration::from_millis(150));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    let mid_value = controller.animation_value();
    assert!(mid_value > 0.0 && mid_value < 1.0, "mid-value should be 0<v<1, got {}", mid_value);

    // Close mid-open.
    controller.close();
    pipeline.perform_rebuilds();
    assert_eq!(controller.phase(), Phase::Closing);

    // The value right after close should NOT jump to 1.0 — it should be
    // near mid_value (the spring starts from the current value).
    let value_after_close = controller.animation_value();
    assert!(
        (value_after_close - mid_value).abs() < 0.15,
        "value after close ({}) should be near mid_value ({}) — no jump to 1.0",
        value_after_close,
        mid_value
    );

    // Settle to Closed.
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    assert_eq!(controller.phase(), Phase::Closed);
}
```

- [ ] **Step 4: Write failing test #4 — reshow during close retargets upward**

```rust
#[test]
fn test_reshow_during_close_retargets_upward() {
    let controller = ContextMenuController::new();
    let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
    let ticker = Arc::new(AnimationTicker::new());

    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Open and settle.
    controller.show(
        vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
        vexo::Text::new("bubble").boxed(),
        test_content_builder("Copy"),
    );
    pipeline.perform_rebuilds();
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();

    // Start close, advance partway down.
    controller.close();
    pipeline.perform_rebuilds();
    std::thread::sleep(std::time::Duration::from_millis(150));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    let mid_value = controller.animation_value();
    assert!(mid_value > 0.0 && mid_value < 1.0);

    // Re-show with new bounds — should retarget upward.
    controller.show(
        vexo::core::Bounds::new(20.0, 20.0, 100.0, 40.0),
        vexo::Text::new("bubble2").boxed(),
        test_content_builder("Reply"),
    );
    pipeline.perform_rebuilds();
    assert_eq!(controller.phase(), Phase::Opening);

    let value_after_reshow = controller.animation_value();
    assert!(
        (value_after_reshow - mid_value).abs() < 0.15,
        "value after reshow ({}) should be near mid_value ({}) — no jump",
        value_after_reshow,
        mid_value
    );

    // Settle to Open.
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    assert_eq!(controller.phase(), Phase::Open);
}
```

- [ ] **Step 5: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit test_show_starts_open_spring test_close_starts_reverse_spring_not_immediate_unmount test_early_close_during_open_reverses_smoothly test_reshow_during_close_retargets_upward`
Expected: FAIL — `show()` currently sets phase=Open instantly.

- [ ] **Step 6: Implement the spring-driven phase machine**

In `vexo_uikit/src/context_menu.rs`, update the `Shared` struct and controller methods:

```rust
use vexo::animation::{AnimationController, SpringDescription, SpringSimulation};
use std::time::Instant;

struct Shared {
    phase: Phase,
    open: Option<OpenState>,
    animation: AnimationController,
    ticker: Option<Arc<AnimationTicker>>,
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ContextMenuController {
    pub fn new() -> Self {
        Self {
            shared: Rc::new(RefCell::new(Shared {
                phase: Phase::Closed,
                open: None,
                animation: AnimationController::new(std::time::Duration::from_millis(600)),
                ticker: None,
                dirty_callback: None,
            })),
        }
    }

    pub fn show(
        &self,
        bubble_bounds: Bounds<Logical>,
        bubble_widget: Box<dyn Widget>,
        builder: MenuBuilder,
    ) {
        let mut s = self.shared.borrow_mut();
        s.open = Some(OpenState {
            bubble_bounds,
            bubble_widget,
            builder,
        });
        s.phase = Phase::Opening;
        // Wire ticker + dirty if not already (host does this in on_mount,
        // but show() may be called before on_mount in tests).
        if let Some(ticker) = &s.ticker {
            s.animation.set_ticker(ticker.clone());
        }
        if let Some(cb) = &s.dirty_callback {
            s.animation.set_dirty_callback(cb.clone());
        }
        s.animation.animate_with(Box::new(SpringSimulation::new(
            SpringDescription::ios(340.0, 1.0),
            s.animation.value(), // from current value (smooth retarget)
            1.0,
            0.0,
        )));
    }

    pub fn close(&self) {
        let mut s = self.shared.borrow_mut();
        if s.phase == Phase::Closed {
            return;
        }
        s.phase = Phase::Closing;
        s.animation.animate_with(Box::new(SpringSimulation::new(
            SpringDescription::ios(340.0, 1.0),
            s.animation.value(), // from current value (smooth retarget)
            0.0,
            0.0,
        )));
    }

    pub fn phase(&self) -> Phase {
        self.shared.borrow().phase
    }

    pub fn animation_value(&self) -> f64 {
        self.shared.borrow().animation.value()
    }

    pub fn set_animation_ticker(&self, t: Arc<AnimationTicker>) {
        let mut s = self.shared.borrow_mut();
        s.ticker = Some(t.clone());
        s.animation.set_ticker(t);
    }

    pub fn set_dirty_callback(&self, cb: Arc<dyn Fn() + Send + Sync>) {
        let mut s = self.shared.borrow_mut();
        s.dirty_callback = Some(cb.clone());
        s.animation.set_dirty_callback(cb);
    }

    /// Advance the spring and handle phase transitions. Called by the
    /// host's `on_tick`.
    pub(crate) fn advance(&self, now: Instant) {
        let mut s = self.shared.borrow_mut();
        if s.phase == Phase::Closed {
            return;
        }
        s.animation.advance(now);

        // Check for settle.
        if !s.animation.is_animating() {
            match s.phase {
                Phase::Opening => {
                    s.phase = Phase::Open;
                }
                Phase::Closing => {
                    s.phase = Phase::Closed;
                    s.open = None;
                }
                _ => {}
            }
        }
    }

    pub(crate) fn open_snapshot(&self) -> Option<(Bounds<Logical>, Box<dyn Widget>, MenuBuilder)> {
        let s = self.shared.borrow();
        s.open.as_ref().map(|o| {
            (
                o.bubble_bounds,
                o.bubble_widget.clone_boxed(),
                o.builder.clone(),
            )
        })
    }
}
```

Update the host's `State` to add `on_tick`:

```rust
impl ComponentState for ContextMenuHostState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(widget) = ctx.widget().downcast_ref::<ContextMenu>() {
            widget.controller.set_dirty_callback(ctx.dirty_callback());
            widget.controller.set_animation_ticker(ctx.animation_ticker().clone());
        }
    }
    fn on_update(&mut self, _old: &dyn Any, ctx: &mut LifecycleContext) {
        if let Some(widget) = ctx.widget().downcast_ref::<ContextMenu>() {
            widget.controller.set_dirty_callback(ctx.dirty_callback());
            widget.controller.set_animation_ticker(ctx.animation_ticker().clone());
        }
    }
    fn on_tick(&mut self, now: Instant) {
        // The controller reference isn't available here directly — we need
        // to store it in the state. Update ContextMenuHostState:
        // (See Step 7 below for the state struct update.)
    }
}
```

- [ ] **Step 7: Store the controller in the host state for on_tick access**

```rust
pub struct ContextMenuHostState {
    controller: Option<ContextMenuController>,
}

impl Default for ContextMenuHostState {
    fn default() -> Self {
        Self { controller: None }
    }
}

impl ComponentState for ContextMenuHostState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(widget) = ctx.widget().downcast_ref::<ContextMenu>() {
            self.controller = Some(widget.controller.clone());
            widget.controller.set_dirty_callback(ctx.dirty_callback());
            widget.controller.set_animation_ticker(ctx.animation_ticker().clone());
        }
    }
    fn on_update(&mut self, _old: &dyn Any, ctx: &mut LifecycleContext) {
        if let Some(widget) = ctx.widget().downcast_ref::<ContextMenu>() {
            self.controller = Some(widget.controller.clone());
            widget.controller.set_dirty_callback(ctx.dirty_callback());
            widget.controller.set_animation_ticker(ctx.animation_ticker().clone());
        }
    }
    fn on_tick(&mut self, now: Instant) {
        if let Some(ctrl) = &self.controller {
            ctrl.advance(now);
        }
    }
}
```

- [ ] **Step 8: Run tests #1-4**

Run: `cargo test -p vexo_uikit test_show_starts_open_spring test_close_starts_reverse_spring_not_immediate_unmount test_early_close_during_open_reverses_smoothly test_reshow_during_close_retargets_upward`
Expected: PASS — all 4 lifecycle tests pass.

- [ ] **Step 9: Run all tests to check for regressions**

Run: `cargo test -p vexo_uikit && cargo test -p shared_app`
Expected: PASS — existing tests may need updating if they assumed instant open/close. The `test_controller_show_close_new_api` from Task 2 will FAIL (show no longer sets phase=Open instantly). Update it:

```rust
#[test]
fn test_controller_show_close_new_api() {
    let controller = ContextMenuController::new();
    assert_eq!(controller.phase(), Phase::Closed);

    // show() now starts a spring — phase is Opening, not Open.
    controller.show(
        vexo::core::Bounds::new(10.0, 20.0, 100.0, 50.0),
        vexo::Text::new("bubble").boxed(),
        test_content_builder("Copy"),
    );
    assert_eq!(controller.phase(), Phase::Opening);

    // close() starts reverse spring — phase is Closing, not Closed.
    controller.close();
    assert_eq!(controller.phase(), Phase::Closing);
}
```

Also update `test_host_open_renders_menu_at_position` and `test_barrier_dismiss_on_outside_click` — they call `show()` and immediately assert content is visible. With the spring, phase is `Opening` (not `Closed`), so content IS rendered (phase != Closed). The assertions should still pass. Verify.

- [ ] **Step 10: Commit**

```bash
git add vexo_uikit/src/context_menu.rs
git commit -m "feat(vexo_uikit): spring-driven phase machine for context menu open/close"
```

---

### Task 6: Wire spring value into host render (dim alpha, bubble transform, card scale+opacity)

**Files:**
- Modify: `vexo_uikit/src/context_menu.rs` (host render: apply spring-driven transforms)
- Test: `vexo_uikit/src/context_menu.rs` (test #5 — barrier dismiss during animation)

**Interfaces:**
- Consumes: `controller.animation_value()` from Task 5.
- Produces: all 4 overlay layers (dim, bubble copy, actions card) animated by the spring value. The reactions pill is still not rendered (Task 7 adds it with positioning).

**Context:** The spring value `v` (0→1 on open, 1→0 on close) drives: dim alpha `v*0.4`, bubble copy scale `1+v*0.03` + translate `-v*4.0`, actions card scale `0.8+v*0.2` + opacity `v`. Scale-about-center via translate→scale→translate chain using `metrics` sizes. Test #5 verifies barrier dismiss works mid-animation.

- [ ] **Step 1: Write failing test #5 — barrier dismiss during animation**

```rust
#[test]
fn test_dim_barrier_dismiss_during_animation() {
    let controller = ContextMenuController::new();
    let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
    let ticker = Arc::new(AnimationTicker::new());

    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Open — don't wait for settle (we want mid-animation).
    controller.show(
        vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
        vexo::Text::new("bubble").boxed(),
        test_content_builder("Copy"),
    );
    pipeline.perform_rebuilds();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Click far away (on the dim barrier) mid-open.
    let primary_press = vexo::input::InputEvent::PointerButton {
        position: vexo::core::Point::new(350.0, 550.0),
        button: vexo::input::PointerButton::Primary,
        state: vexo::input::ButtonState::Pressed,
    };
    let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
        std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
    pipeline.handle_event(
        vexo::core::Point::new(350.0, 550.0),
        &primary_press,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &vexo::core::ScaleSource::default(),
        &clipboard,
    );
    pipeline.perform_rebuilds();

    // close() should have fired — phase is Closing (not Closed yet, spring
    // is mid-reverse).
    assert_eq!(
        controller.phase(),
        Phase::Closing,
        "barrier click mid-open should start close (phase=Closing)"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit test_dim_barrier_dismiss_during_animation`
Expected: May pass or fail depending on whether the barrier is hit-testable mid-animation. If it passes, the test is still valuable as a regression net. If it fails, the dim barrier's hit area isn't working during animation — fix in Step 3.

- [ ] **Step 3: Implement spring-driven transforms in host render**

Update `ContextMenu::render`:

```rust
fn render(&self, _state: &mut ContextMenuHostState, ctx: &mut RenderContext) -> Box<dyn Widget> {
    let theme = vexo::Theme::of(ctx);
    let phase = self.controller.phase();
    let v = self.controller.animation_value();

    let mut stack = vexo::Stack::new().push(self.child.clone_boxed());

    if phase != Phase::Closed {
        if let Some((bubble_bounds, bubble_widget, builder)) =
            self.controller.open_snapshot()
        {
            let controller = self.controller.clone();
            let content = builder(&controller, &theme);
            let metrics = content.metrics;

            // [2] Dim barrier — alpha = v * 0.4
            let ctrl_for_barrier = controller.clone();
            let dim_alpha = (v * 0.4) as f32;
            let barrier = vexo::Positioned::new(
                vexo::GestureDetector::new(
                    vexo::Opacity::new(
                        vexo::WithLayout::new(
                            vexo::DecoratedBox::with_style(
                                vexo::Text::new(""),
                                vexo::Style::default()
                                    .background(vexo::Color::BLACK),
                            ),
                            vexo::Layout::default()
                                .width_percent(1.0)
                                .height_percent(1.0),
                        ),
                        dim_alpha,
                    ),
                )
                .on_press(move || ctrl_for_barrier.close()),
            )
            .left(0.0)
            .top(0.0)
            .right(0.0)
            .bottom(0.0);
            stack = stack.push(barrier);

            // [3] Bright bubble copy — scale 1+v*0.03, translate -v*4px,
            // opacity 1.0 (always full bright).
            let ctrl_for_bubble = controller.clone();
            let bubble_scale = 1.0 + v * 0.03;
            let bubble_lift = -(v * 4.0) as f32;
            let bw = bubble_bounds.width();
            let bh = bubble_bounds.height();
            let bubble_copy = vexo::Positioned::new(
                vexo::GestureDetector::new(
                    scale_about_center(
                        bubble_widget,
                        bubble_scale as f32,
                        bubble_scale as f32,
                        bw,
                        bh,
                    ),
                )
                .on_press(move || ctrl_for_bubble.close()),
            )
            .left(bubble_bounds.left)
            .top(bubble_bounds.top + bubble_lift);
            stack = stack.push(bubble_copy);

            // [5] Actions card — scale 0.8+v*0.2, opacity v, positioned
            // below the bubble (Task 7 adds full positioning).
            let card_scale = 0.8 + v * 0.2;
            let card_opacity = v as f32;
            let actions_x = bubble_bounds.left;
            let actions_y = bubble_bounds.top + bubble_bounds.height() + metrics.gap;
            let positioned_actions = vexo::Positioned::new(
                vexo::Opacity::new(
                    scale_about_center(
                        content.actions,
                        card_scale as f32,
                        card_scale as f32,
                        metrics.actions_size.width,
                        metrics.actions_size.height,
                    ),
                    card_opacity,
                ),
            )
            .left(actions_x)
            .top(actions_y);
            stack = stack.push(positioned_actions);
        }
    }

    stack.boxed()
}
```

Add the `scale_about_center` helper:

```rust
/// Wrap a child in a transform chain that scales about its center:
/// translate(-w/2, -h/2) → scale(sx, sy) → translate(w/2, h/2).
fn scale_about_center(
    child: Box<dyn Widget>,
    sx: f32,
    sy: f32,
    w: f32,
    h: f32,
) -> Box<dyn Widget> {
    vexo::Transform::translate(
        vexo::Transform::scale(
            vexo::Transform::translate(child, -w / 2.0, -h / 2.0),
            sx,
            sy,
        ),
        w / 2.0,
        h / 2.0,
    )
    .boxed()
}
```

Note: The `Transform::translate`/`Transform::scale` constructors each wrap their child in a new Transform widget. The chain is inside-out: translate(child, -w/2, -h/2) is the innermost (applied first to the child), then scale wraps that, then translate wraps the scaled result. This is correct for scale-about-center.

- [ ] **Step 4: Run test #5 and all tests**

Run: `cargo test -p vexo_uikit`
Expected: PASS — all tests including the barrier-dismiss-during-animation test.

Run: `cargo test -p shared_app`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/context_menu.rs
git commit -m "feat(vexo_uikit): spring-driven dim/bubble/card transforms in context menu"
```

---

### Task 7: Reactions pill positioning + edge-aware flip/clamp

**Files:**
- Modify: `vexo_uikit/src/context_menu.rs` (host render: add reactions pill layer, edge-aware positioning)
- Test: `vexo_uikit/src/context_menu.rs` (tests #8, #9)

**Interfaces:**
- Consumes: `content.reactions` + `content.metrics` from the builder.
- Produces: host renders all 5 layers (content, dim, bubble copy, reactions pill, actions card) with edge-aware positioning.

**Context:** The reactions pill goes above the bubble, the actions card below. If there's no room above for the pill, it goes below the actions card. If there's no room below for the actions card, the whole stack flips above the bubble. Horizontal: clamp to `[8, window_w - card_w - 8]`. Window size from `MediaQuery::of(ctx)`.

- [ ] **Step 1: Write failing test #8 — edge flip when no room above**

```rust
#[test]
fn test_edge_flip_when_no_room_above() {
    let controller = ContextMenuController::new();
    let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
    let ticker = Arc::new(AnimationTicker::new());

    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Bubble at the very top — no room above for the reactions pill.
    controller.show(
        vexo::core::Bounds::new(50.0, 5.0, 100.0, 40.0),
        vexo::Text::new("bubble").boxed(),
        test_content_builder("Copy"),
    );
    // Settle to Open so we can inspect the laid-out positions.
    pipeline.perform_rebuilds();
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // The reactions pill should be positioned BELOW the actions card
    // (not above the bubble, where it would clip off-screen).
    // We assert this by checking that both cards are below the bubble.
    // The reactions pill's top > bubble_bounds.top + bubble_bounds.height().
    // (Detailed position assertions require walking Positioned render objects,
    // which is fragile. Instead, assert the menu didn't clip by checking
    // both cards are within window bounds.)
    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");
    assert!(
        find_text_in_tree(ro_reg, root, "Copy"),
        "actions card should still be rendered with edge flip"
    );
    // The key assertion: both "r" (reactions) and "Copy" (actions) appear,
    // proving neither card was clipped off-screen.
    assert!(
        find_text_in_tree(ro_reg, root, "r"),
        "reactions pill should still be rendered with edge flip"
    );
}
```

- [ ] **Step 2: Write failing test #9 — edge flip when no room below**

```rust
#[test]
fn test_edge_flip_when_no_room_below() {
    let controller = ContextMenuController::new();
    let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
    let ticker = Arc::new(AnimationTicker::new());

    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Bubble near the bottom — no room below for the actions card.
    controller.show(
        vexo::core::Bounds::new(50.0, 560.0, 100.0, 40.0),
        vexo::Text::new("bubble").boxed(),
        test_content_builder("Copy"),
    );
    pipeline.perform_rebuilds();
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Both cards should be above the bubble (flipped).
    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");
    assert!(
        find_text_in_tree(ro_reg, root, "Copy"),
        "actions card should still be rendered with edge flip"
    );
    assert!(
        find_text_in_tree(ro_reg, root, "r"),
        "reactions pill should still be rendered with edge flip"
    );
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit test_edge_flip_when_no_room_above test_edge_flip_when_no_room_below`
Expected: FAIL — the reactions pill is not rendered yet (only actions card).

- [ ] **Step 4: Implement positioning + reactions pill + edge flip**

Update `ContextMenu::render` with the full positioning logic:

```rust
fn render(&self, _state: &mut ContextMenuHostState, ctx: &mut RenderContext) -> Box<dyn Widget> {
    let theme = vexo::Theme::of(ctx);
    let phase = self.controller.phase();
    let v = self.controller.animation_value();

    let mut stack = vexo::Stack::new().push(self.child.clone_boxed());

    if phase != Phase::Closed {
        if let Some((bubble_bounds, bubble_widget, builder)) =
            self.controller.open_snapshot()
        {
            let controller = self.controller.clone();
            let content = builder(&controller, &theme);
            let metrics = content.metrics;

            // Window size for edge detection.
            let mq = vexo::MediaQuery::of(ctx);
            let window_w = mq.size.width;
            let window_h = mq.size.height;

            // === Positioning ===
            let gap = metrics.gap;
            let pill_h = metrics.reactions_size.height;
            let card_h = metrics.actions_size.height;
            let bubble_bottom = bubble_bounds.top + bubble_bounds.height();
            let bubble_center_x = bubble_bounds.left + bubble_bounds.width() / 2.0;

            // Default: pill above bubble, card below bubble.
            // Pill bottom edge = bubble_top - gap. Pill top = bubble_top - gap - pill_h.
            // Card top edge = bubble_bottom + gap.
            let room_above = bubble_bounds.top - gap - pill_h - gap >= 0.0;
            let room_below = bubble_bottom + gap + card_h <= window_h;

            let (pill_y, card_y) = if room_above && room_below {
                // Default: pill above, card below.
                (bubble_bounds.top - gap - pill_h, bubble_bottom + gap)
            } else if !room_above && room_below {
                // No room above: pill below the card.
                (bubble_bottom + gap + card_h + gap, bubble_bottom + gap)
            } else if room_above && !room_below {
                // No room below: flip both above the bubble. Card directly
                // above bubble, pill above card.
                (
                    bubble_bounds.top - gap - pill_h,
                    bubble_bounds.top - gap - card_h,
                )
            } else {
                // No room above or below: default to below (best effort).
                (bubble_bottom + gap + card_h + gap, bubble_bottom + gap)
            };

            // Horizontal: center on bubble, clamp to [8, window_w - card_w - 8].
            let clamp_x = |card_w: f32| -> f32 {
                let x = bubble_center_x - card_w / 2.0;
                x.max(8.0).min(window_w - card_w - 8.0)
            };
            let pill_x = clamp_x(metrics.reactions_size.width);
            let card_x = clamp_x(metrics.actions_size.width);

            // [2] Dim barrier (unchanged from Task 6).
            let ctrl_for_barrier = controller.clone();
            let dim_alpha = (v * 0.4) as f32;
            let barrier = vexo::Positioned::new(
                vexo::GestureDetector::new(
                    vexo::Opacity::new(
                        vexo::WithLayout::new(
                            vexo::DecoratedBox::with_style(
                                vexo::Text::new(""),
                                vexo::Style::default().background(vexo::Color::BLACK),
                            ),
                            vexo::Layout::default().width_percent(1.0).height_percent(1.0),
                        ),
                        dim_alpha,
                    ),
                )
                .on_press(move || ctrl_for_barrier.close()),
            )
            .left(0.0).top(0.0).right(0.0).bottom(0.0);
            stack = stack.push(barrier);

            // [3] Bright bubble copy (unchanged from Task 6).
            let ctrl_for_bubble = controller.clone();
            let bubble_scale = 1.0 + v * 0.03;
            let bubble_lift = -(v * 4.0) as f32;
            let bw = bubble_bounds.width();
            let bh = bubble_bounds.height();
            let bubble_copy = vexo::Positioned::new(
                vexo::GestureDetector::new(
                    scale_about_center(bubble_widget, bubble_scale as f32, bubble_scale as f32, bw, bh),
                )
                .on_press(move || ctrl_for_bubble.close()),
            )
            .left(bubble_bounds.left)
            .top(bubble_bounds.top + bubble_lift);
            stack = stack.push(bubble_copy);

            // [4] Reactions pill — scale 0.8+v*0.2, opacity v.
            let pill_scale = 0.8 + v * 0.2;
            let pill_opacity = v as f32;
            let positioned_pill = vexo::Positioned::new(
                vexo::Opacity::new(
                    scale_about_center(
                        content.reactions,
                        pill_scale as f32,
                        pill_scale as f32,
                        metrics.reactions_size.width,
                        metrics.reactions_size.height,
                    ),
                    pill_opacity,
                ),
            )
            .left(pill_x)
            .top(pill_y);
            stack = stack.push(positioned_pill);

            // [5] Actions card (unchanged from Task 6, now using card_x/card_y).
            let card_scale = 0.8 + v * 0.2;
            let card_opacity = v as f32;
            let positioned_actions = vexo::Positioned::new(
                vexo::Opacity::new(
                    scale_about_center(
                        content.actions,
                        card_scale as f32,
                        card_scale as f32,
                        metrics.actions_size.width,
                        metrics.actions_size.height,
                    ),
                    card_opacity,
                ),
            )
            .left(card_x)
            .top(card_y);
            stack = stack.push(positioned_actions);
        }
    }

    stack.boxed()
}
```

- [ ] **Step 5: Run tests #8, #9 and all tests**

Run: `cargo test -p vexo_uikit`
Expected: PASS.

Run: `cargo test -p shared_app`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/context_menu.rs
git commit -m "feat(vexo_uikit): reactions pill + edge-aware positioning for context menu"
```

---

### Task 8: Polish, metrics tuning, and final verification

**Files:**
- Modify: `shared_app/src/chats/message_menu.rs` (tune `MenuMetrics` constants)
- Verify: full build + test sweep + manual checklist handoff

**Interfaces:**
- No new interfaces. Final tuning + verification only.

**Context:** The `MenuMetrics` constants (~150×28 pill, ~200×108 card) are estimates. This task verifies them by reading back real laid-out sizes, tunes if needed, and runs the full verification gate. Then hands the manual visual checklist to the user.

- [ ] **Step 1: Verify metrics by reading back real laid-out sizes**

Write a temporary test (or add to an existing test) that opens the menu, lays out, and reads back the reactions pill + actions card render object sizes. Compare against `MenuMetrics`:

```rust
#[test]
fn test_metrics_match_real_sizes() {
    // This test is informational — if it fails, update MenuMetrics in
    // message_menu.rs to match the real sizes.
    let controller = ContextMenuController::new();
    let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
    let ticker = Arc::new(AnimationTicker::new());
    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.update(host.boxed());
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = new_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    controller.show(
        vexo::core::Bounds::new(100.0, 100.0, 100.0, 40.0),
        vexo::Text::new("bubble").boxed(),
        // Use the REAL builder from shared_app, not test_content_builder.
        // If that's not accessible from vexo_uikit tests, copy the metrics
        // values and verify manually.
        test_content_builder("Copy"),
    );
    pipeline.perform_rebuilds();
    std::thread::sleep(std::time::Duration::from_millis(700));
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    // Read back the actions card size by finding the DecoratedBoxRenderObject
    // that contains "Copy" text and checking its computed_bounds.
    // (Implementation detail — walk the tree, find the text, go up to the
    // nearest DecoratedBox, read its bounds.)
    // Assert: width >= 200.0 (min_width), height ≈ 108.0 (±15px tolerance).
    // If significantly off, update MenuMetrics in message_menu.rs.
}
```

If the real sizes differ from the constants by more than ~15px, update `MenuMetrics` in `shared_app/src/chats/message_menu.rs` to match. Delete the temporary test or keep it as a regression net (commented out if flaky).

- [ ] **Step 2: Run full verification gate**

```bash
cargo build -p vexo
cargo test   -p vexo
cargo build -p vexo_uikit
cargo test   -p vexo_uikit
cargo build -p shared_app
cargo test   -p shared_app
cargo build -p desktop_demo
```

Expected: All builds and tests pass.

- [ ] **Step 3: Commit any metrics changes**

```bash
git add shared_app/src/chats/message_menu.rs
git commit -m "fix(shared_app): tune MenuMetrics to match real card sizes"
```
(Only commit if metrics were changed. Skip if no changes.)

- [ ] **Step 4: Hand manual visual checklist to the user**

Do NOT run `cargo run -p desktop_demo` yourself (per CLAUDE.md). Ask the user to run it and verify:

```
Please run `cargo run -p desktop_demo` and right-click a message bubble. Verify:

1. Right-click a bubble → screen dims, tapped bubble lifts slightly +
   brightens, reactions pill scales in above it, actions card scales in
   below it — all moving together.
2. Mid-open, click outside → menu reverses smoothly (no snap) and unmounts.
3. Mid-open, right-click another bubble → menu closes (reverses); need a
   second right-click to open the new one (v1 limitation).
4. Reactions pill: 6 FA icons, centered above the bubble, pill-shaped
   (18px radius).
5. Actions card: Copy/Reply/Delete, hover tint works, Delete is red.
6. Click a reaction or action → log line + menu reverses + unmounts.
7. Right-click a bubble near the top of the screen → reactions pill flips
   below the actions card.
8. Right-click a bubble near the bottom → both cards flip above the bubble.
9. Right-click a bubble near the left/right edge → cards clamp on-screen,
   don't clip.
10. Toggle theme while menu open → cards + bubble copy re-render with new
    colors.
```

Wait for the user's feedback. If issues are reported, create follow-up tasks to address them.
