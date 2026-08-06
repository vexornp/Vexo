//! Context menu widget trio: `MenuBuilder`, `ContextMenuController`, `ContextMenu` host.
//!
//! Mirrors the `ScrollController` pattern: the screen owns a controller,
//! wraps its root in `ContextMenu::new(child, controller)`, and wraps each
//! right-clickable element in `context_menu_trigger(child, controller, builder)`.
//!
//! The menu's visual content is fully caller-supplied via `MenuBuilder`. The
//! builder runs at render time (inside `ContextMenu::render`), so it always
//! reads the current theme. Each trigger captures its own builder, so different
//! bubbles can render different menu styles.
//!
//! Task 2 state: the API is reshaped to its final form (`show(bubble_bounds,
//! bubble_widget, builder)`, `phase()`, `animation_value()`,
//! `MenuContent { reactions, actions, metrics }`) but open/close is still
//! instant — no spring animation yet (Task 5 adds it). Task 4 adds the dim
//! barrier (full-screen 0.4 alpha black, tappable to dismiss) and the bright
//! bubble copy (Positioned at bubble_bounds, full opacity, tappable to
//! dismiss). The actions card is now positioned below the bubble. The
//! reactions pill and spring animation come in later tasks.

use std::any::Any;
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use vexo::animation::AnimationTicker;
use vexo::core::{Bounds, Logical, Size};
use vexo::{Component, ComponentState, LifecycleContext, RenderContext, Widget};

// ============================================================================
// MenuContent + MenuMetrics + MenuBuilder
// ============================================================================

/// The two cards produced by a menu builder.
///
/// `reactions` is the top pill (emoji/reaction strip); `actions` is the lower
/// card (Copy / Reply / Delete rows). The host positions them relative to
/// `bubble_bounds` using `metrics` for spacing. Task 2 renders only `actions`;
/// Task 3 adds the reactions pill and proper styling.
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

/// Caller-supplied factory that produces the menu's `MenuContent`.
///
/// Wraps `Rc<dyn Fn(&ContextMenuController, &ThemeData) -> MenuContent>`.
/// `Rc<dyn Fn>` (not `FnMut`): the builder is cloned into the controller's
/// cell and re-invoked on every rebuild; `Rc` keeps clones cheap and matches
/// the single-threaded pattern used elsewhere in `vexo_uikit` (no `Send +
/// Sync` bounds that `Arc` would impose).
///
/// The builder runs inside `ContextMenu::render`, so it always sees the live
/// `ThemeData` — theme toggles re-render the menu correctly. It receives
/// `&ContextMenuController` so its item rows can call `controller.close()` on
/// tap.
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

/// Lifecycle phase of the context menu. Task 2 only uses `Closed` and `Open`
/// (instant transitions). Task 5 adds `Opening`/`Closing` for the spring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Opening,
    Open,
    Closing,
}

/// Snapshot of what `show()` was called with. Held in the controller's shared
/// cell while the menu is open; cleared on `close()`.
struct OpenState {
    bubble_bounds: Bounds<Logical>,
    bubble_widget: Box<dyn Widget>,
    builder: MenuBuilder,
}

/// Shared (across controller clones) mutable state.
struct Shared {
    phase: Phase,
    open: Option<OpenState>,
    animation_value: f64,
    /// Stored for Task 5's spring; not yet driven by `on_tick`.
    ticker: Option<Arc<AnimationTicker>>,
    /// Wired by the host's `on_mount`/`on_update`. Invoked by `show()`/`close()`
    /// so the host rebuilds and re-reads `phase()`/`open_snapshot()`. This
    /// replaces the old `Signal<Option<Point>>` + `signal_value` path — the
    /// builder is `!Send + !Sync` (`Rc<dyn Fn>`), so it can't travel through a
    /// `Signal`, and the controller now owns the open state directly.
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// Controller for a context menu — owns open/close state, the current builder,
/// the animation phase, and hooks for the dirty callback + animation ticker.
///
/// Created by the screen's caller (alongside `ScrollController::new()`), held
/// as a field, `.clone()`d into triggers and the host. The `Rc<RefCell<Shared>>`
/// shares underlying state across clones, so widget-struct recreation on
/// rebuild doesn't lose menu state.
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

    /// Open the menu anchored to `bubble_bounds`, carrying a clone of the
    /// bubble widget (Task 6 renders a lifted copy). Instant: sets phase to
    /// `Open` and `animation_value` to `1.0`. Task 5 replaces this with a
    /// forward spring that ramps `animation_value` 0→1 over a few frames.
    pub fn show(
        &self,
        bubble_bounds: Bounds<Logical>,
        bubble_widget: Box<dyn Widget>,
        builder: MenuBuilder,
    ) {
        let dirty = {
            let mut s = self.shared.borrow_mut();
            s.open = Some(OpenState {
                bubble_bounds,
                bubble_widget,
                builder,
            });
            s.phase = Phase::Open;
            s.animation_value = 1.0;
            s.dirty_callback.clone()
        };
        // Notify the host so it rebuilds and re-reads phase()/open_snapshot().
        // Matches the framework pattern (AnimationController, TextEditingController,
        // ScrollController all invoke the dirty callback on state change).
        if let Some(cb) = dirty {
            cb();
        }
    }

    /// Close the menu. Instant: clears open state, sets phase to `Closed` and
    /// `animation_value` to `0.0`. Task 5 replaces this with a reverse spring
    /// that ramps `animation_value` 1→0, then clears `open` on completion.
    pub fn close(&self) {
        let dirty = {
            let mut s = self.shared.borrow_mut();
            s.open = None;
            s.phase = Phase::Closed;
            s.animation_value = 0.0;
            s.dirty_callback.clone()
        };
        if let Some(cb) = dirty {
            cb();
        }
    }

    pub fn phase(&self) -> Phase {
        self.shared.borrow().phase
    }

    /// Placeholder: `1.0` when `Open`, `0.0` when `Closed`. Task 5 drives this
    /// from the spring simulation.
    pub fn animation_value(&self) -> f64 {
        self.shared.borrow().animation_value
    }

    /// Store the animation ticker for Task 5's spring. No-op beyond storing
    /// (the ticker isn't registered for per-frame ticks yet).
    pub fn set_animation_ticker(&self, t: Arc<AnimationTicker>) {
        self.shared.borrow_mut().ticker = Some(t);
    }

    /// Store the host's dirty callback. Invoked by `show()`/`close()` to
    /// trigger a host rebuild.
    pub fn set_dirty_callback(&self, cb: Arc<dyn Fn() + Send + Sync>) {
        self.shared.borrow_mut().dirty_callback = Some(cb);
    }

    /// Snapshot the current open state (clones bounds, clones the bubble
    /// widget, clones the builder). Returns `None` when closed.
    /// Called by the host during `render()` only when `phase() != Closed`.
    /// The bubble widget isn't used by the host in Task 2 (Task 6 renders a
    /// lifted copy during the open animation), but it's returned here so the
    /// snapshot signature is stable across tasks.
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

// ============================================================================
// ContextMenu host (Component) + ContextMenuHostState
// ============================================================================

/// A host widget that renders a context menu overlay on top of its child.
///
/// Wrap the screen's root content in `ContextMenu::new(content, controller)`.
/// When `controller.show(bubble_bounds, bubble_widget, builder)` is called
/// (e.g. by a `context_menu_trigger` on right-click), the host rebuilds and
/// shows a floating menu at the bubble position — the menu's content is
/// whatever the caller's `builder` returns — with a full-size dismiss barrier
/// behind it.
///
/// The host must be OUTSIDE any `ScrollView` (which clips with `overflow:
/// Hidden`). Place it at the screen root so the menu floats above all content.
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

impl Clone for ContextMenu {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
            child: self.child.clone_boxed(),
        }
    }
}

/// Host state for `ContextMenu`. Wires the controller's dirty callback in
/// `on_mount`/`on_update` so `show()`/`close()` (called from event handlers or
/// programmatically) trigger a host rebuild. Task 5 also wires the animation
/// ticker here for the spring `on_tick`.
#[derive(Default)]
pub struct ContextMenuHostState;

impl ComponentState for ContextMenuHostState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(menu) = ctx.widget().downcast_ref::<ContextMenu>() {
            menu.controller.set_dirty_callback(ctx.dirty_callback());
        }
    }
    fn on_update(&mut self, _old: &dyn Any, ctx: &mut LifecycleContext) {
        // Re-wire on every parent-cascade update. The controller is shared via
        // Rc<RefCell>, so identity comparison (Rc::ptr_eq) isn't meaningful
        // here — the widget struct is recreated each rebuild but the shared
        // cell persists. Just re-store the current dirty callback.
        if let Some(menu) = ctx.widget().downcast_ref::<ContextMenu>() {
            menu.controller.set_dirty_callback(ctx.dirty_callback());
        }
    }
}

impl Component for ContextMenu {
    type State = ContextMenuHostState;

    fn render(
        &self,
        _state: &mut ContextMenuHostState,
        ctx: &mut RenderContext,
    ) -> Box<dyn Widget> {
        let theme = vexo::Theme::of(ctx);
        let phase = self.controller.phase();

        let mut stack = vexo::Stack::new().push(self.child.clone_boxed());

        if phase != Phase::Closed {
            if let Some((bubble_bounds, bubble_widget, builder)) = self.controller.open_snapshot() {
                let controller = self.controller.clone();

                // [2] Dim barrier — full-screen, fixed 0.4 alpha (Task 6
                // animates this with the spring value). Structure (per the
                // spec's render tree + the task's implementation note):
                // Positioned(0,0,0,0) → GestureDetector.on_press(→ close) →
                // WithLayout(width_percent=1.0, height_percent=1.0) →
                // Opacity(0.4) → DecoratedBox(BLACK) → Text("").
                //
                // The WithLayout makes the inner subtree fill the Stack's
                // content box, so the GestureDetector's computed_bounds cover
                // the full screen and any tap inside the window hits the
                // barrier (→ close) unless a higher overlay (bubble copy,
                // actions card) intercepts it first. The empty Text is the
                // DecoratedBox's required child — it has zero intrinsic size,
                // so it doesn't affect layout.
                let ctrl_for_barrier = controller.clone();
                let barrier = vexo::Positioned::new(
                    vexo::GestureDetector::new(vexo::WithLayout::new(
                        vexo::Opacity::new(
                            vexo::DecoratedBox::with_style(
                                vexo::Text::new(""),
                                vexo::Style::default().background(vexo::Color::BLACK),
                            ),
                            0.4,
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
                // opacity, tappable to dismiss (matches iMessage: tapping the
                // lifted bubble closes the menu). No transform yet (Task 6
                // adds the scale+lift spring). The bubble_widget is the same
                // widget the caller passed to `show()`; rendering it twice
                // (once in-content under the dim, once here as the bright
                // focal point) is the dual-render spike validated by test #7.
                let ctrl_for_bubble = controller.clone();
                let bubble_copy = vexo::Positioned::new(
                    vexo::GestureDetector::new(bubble_widget)
                        .on_press(move || ctrl_for_bubble.close()),
                )
                .left(bubble_bounds.left)
                .top(bubble_bounds.top);
                stack = stack.push(bubble_copy);

                // [5] Actions card — Positioned below the bubble
                // (top + height + 8px gap). Task 7 adds the reactions pill
                // above the bubble and proper edge-aware positioning.
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

// ============================================================================
// context_menu_trigger — sugar for wrapping a child with right-click detection
// ============================================================================

/// Wrap `child` with a right-click handler that opens the context menu
/// anchored to the child's global bounds, rendering content from `builder`.
///
/// The child widget is cloned and passed to `controller.show()` as the
/// `bubble_widget` (Task 6 renders a lifted copy of it during the open
/// animation). Equivalent to:
/// ```ignore
/// child.on_secondary_press(move |_pos, bounds| {
///     controller.show(bounds, child.clone_boxed(), builder);
/// })
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use vexo::core::ScaleSource;
    use vexo::input::{ButtonState, InputEvent, Modifiers, PointerButton};
    use vexo::layout::TaffyLayoutEngine;
    use vexo::platform::stub_clipboard::StubClipboard;
    use vexo::platform::Clipboard;
    use vexo::render_objects::TextRenderObject;
    use vexo::resource::new_font_system;
    use vexo::RenderObjectKey;
    use vexo::RenderObjectRegistry;
    use vexo::Text;
    use vexo::ThreeTreePipeline;

    fn test_clipboard() -> Arc<dyn Clipboard> {
        Arc::new(StubClipboard)
    }

    /// New-API builder: returns `MenuContent` with placeholder reactions and
    /// a `Text` actions card carrying `label`. Ignores controller + theme.
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

    fn find_text_in_tree(reg: &RenderObjectRegistry, key: RenderObjectKey, needle: &str) -> bool {
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

    #[test]
    fn test_controller_clone_shares_state() {
        let controller = ContextMenuController::new();
        let cloned = controller.clone();

        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(50.0, 60.0, 150.0, 100.0);
        cloned.show(bounds, bubble_widget, test_content_builder("A"));

        // The original sees the same state (shared via Rc<RefCell>).
        assert_eq!(controller.phase(), Phase::Open);
        assert!(controller.open_snapshot().is_some());
    }

    #[test]
    fn test_host_closed_has_only_content() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // When closed, the render tree should NOT contain the menu content.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            !find_text_in_tree(ro_reg, root, "Copy"),
            "menu content 'Copy' should not be rendered when menu is closed"
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
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Open the menu anchored to a bubble at (100, 200) with a builder that
        // renders "Copy" in the actions card.
        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(100.0, 200.0, 300.0, 250.0);
        controller.show(bounds, bubble_widget, test_content_builder("Copy"));
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // The menu content should now appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "menu content 'Copy' should be rendered when menu is open"
        );
    }

    #[test]
    fn test_item_tap_fires_on_select_and_closes() {
        let selected = Rc::new(std::cell::Cell::new(false));
        let selected_clone = selected.clone();

        // A builder that renders a single tappable row in `actions`. on_tap
        // flips the cell and closes the menu — mirrors a real menu item.
        let builder = MenuBuilder::new(move |ctrl, _theme| {
            let ctrl = ctrl.clone();
            let selected = selected_clone.clone();
            let row = vexo::GestureDetector::new(vexo::WithLayout::new(
                vexo::Text::new("Copy"),
                vexo::Layout::default().padding(8.0).width(160.0),
            ))
            .on_tap(move || {
                selected.set(true);
                ctrl.close();
            });
            MenuContent {
                reactions: vexo::Text::new("r").boxed(),
                actions: row.boxed(),
                metrics: MenuMetrics {
                    reactions_size: vexo::core::Size::new(150.0, 28.0),
                    actions_size: vexo::core::Size::new(200.0, 108.0),
                    gap: 8.0,
                },
            }
        });

        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // The actions card is Positioned below the bubble at
        // (bubble_bounds.left, bubble_bounds.top + bubble_bounds.height() + 8)
        // = (10, 10 + 40 + 8) = (10, 58). The item row has 8px padding, so
        // clicking at (15, 70) lands inside the row's padding area.
        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 200.0, 40.0);
        controller.show(bounds, bubble_widget, builder);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let primary_press = InputEvent::PointerButton {
            position: vexo::core::Point::new(15.0, 70.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let primary_release = InputEvent::PointerButton {
            position: vexo::core::Point::new(15.0, 70.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            vexo::core::Point::new(15.0, 70.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        pipeline.handle_event(
            vexo::core::Point::new(15.0, 70.0),
            &primary_release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(selected.get(), "on_tap should have fired");
        // After the tap, the menu should close (controller.close() called by
        // the item's on_tap closure). The dirty callback triggers a rebuild;
        // perform_rebuilds processes it.
        pipeline.perform_rebuilds();
        assert_eq!(
            controller.phase(),
            Phase::Closed,
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
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 200.0, 40.0);
        controller.show(bounds, bubble_widget, test_content_builder("Copy"));
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Click far away from the menu — should hit the barrier and close.
        let primary_press = InputEvent::PointerButton {
            position: vexo::core::Point::new(350.0, 550.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            vexo::core::Point::new(350.0, 550.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        pipeline.perform_rebuilds();
        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "menu should be closed after clicking outside (barrier dismiss)"
        );
    }

    #[test]
    fn test_builder_reads_current_theme() {
        // A builder that encodes theme.surface.r into the rendered text label
        // (placed in the actions card). The builder runs inside
        // `ContextMenu::render`, so it must re-run with the *current* theme
        // whenever the `Theme` InheritedWidget changes — this is the whole
        // justification for running the builder at render time instead of
        // pre-building the menu widget.
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        // Two distinct themes so the assertion can tell them apart. We compute
        // the expected labels from the themes themselves (rather than hardcoding
        // float strings) so the test stays robust to color-preset tweaks.
        let light_theme = vexo::ThemeData::light();
        let dark_theme = vexo::ThemeData::dark();
        let light_label = format!("surface-r-{}", light_theme.surface.r);
        let dark_label = format!("surface-r-{}", dark_theme.surface.r);
        assert_ne!(
            light_label, dark_label,
            "light and dark surface.r must differ for this test to be meaningful"
        );

        // Wrap the host in Theme(light) so the builder reads the light theme
        // via Theme::of(ctx) during render.
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(vexo::Theme::new(light_theme.clone(), host.clone()).boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Open the menu. The builder runs in render() and must read the light
        // theme's surface.r.
        let builder = MenuBuilder::new(|_ctrl, theme| {
            let r = theme.surface.r;
            MenuContent {
                reactions: vexo::Text::new("r").boxed(),
                actions: vexo::Text::new(format!("surface-r-{}", r)).boxed(),
                metrics: MenuMetrics {
                    reactions_size: vexo::core::Size::new(150.0, 28.0),
                    actions_size: vexo::core::Size::new(200.0, 108.0),
                    gap: 8.0,
                },
            }
        });
        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 200.0, 40.0);
        controller.show(bounds, bubble_widget, builder);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, &light_label),
            "builder should have rendered the light theme's surface.r ({:?})",
            light_label
        );

        // Toggle: re-wrap the host in Theme(dark). The InheritedWidget change
        // invalidates the ContextMenu element (a Theme::of dependent), forcing
        // render() — and thus the builder — to re-run with the dark theme.
        // The controller state (open + builder) is shared via Rc<RefCell>, so
        // the menu stays open across the toggle.
        pipeline.update(vexo::Theme::new(dark_theme.clone(), host.clone()).boxed());
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, &dark_label),
            "builder should have re-run with the dark theme's surface.r ({:?}) after the toggle",
            dark_label
        );
        assert!(
            !find_text_in_tree(ro_reg, root, &light_label),
            "light theme's label must be gone after toggling to dark — the builder re-ran"
        );
    }

    /// Walk the render tree and collect the `computed_bounds` sizes of every
    /// `TextRenderObject` whose content contains `needle`. Used by the
    /// dual-render spike test (#7) to assert the in-content and bright-copy
    /// Text render objects have identical laid-out sizes.
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

    #[test]
    fn test_bright_bubble_copy_rendered_on_top() {
        let controller = ContextMenuController::new();
        let bubble_text = "BUBBLE_CONTENT marker";
        let bubble_widget = vexo::Text::new(bubble_text).boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0);

        let host = ContextMenu::new(vexo::Text::new("background content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open the menu with the bubble widget. The host should render a
        // bright copy of `bubble_widget` on top of the dim barrier.
        controller.show(bounds, bubble_widget, test_content_builder("Actions"));
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // The bubble widget's text should appear in the render tree (as the
        // bright copy on top of the dim). It does NOT appear in the host's
        // background content ("background content") nor in the actions card
        // ("Actions"), so a presence check is sufficient.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, bubble_text),
            "bright bubble copy should be rendered when menu is open"
        );
    }

    #[test]
    fn test_bubble_copy_size_matches_original() {
        let controller = ContextMenuController::new();
        // A bubble widget with a known intrinsic size (pinned via WithLayout).
        let bubble_widget = vexo::WithLayout::new(
            vexo::Text::new("X"),
            vexo::Layout::default().width(80.0).height(30.0),
        )
        .boxed();
        // Bounds use (left, top, width, height) — `Bounds::from_xywh` produces
        // valid (left, top, right, bottom) from x/y/w/h. The brief's literal
        // `Bounds::new(50.0, 50.0, 80.0, 30.0)` would be (left=50, top=50,
        // right=80, bottom=30) → negative width/height; using from_xywh keeps
        // the intent (bubble at (50,50) sized 80x30) without malformed edges.
        let bounds = vexo::core::Bounds::from_xywh(50.0, 50.0, 80.0, 30.0);

        // Wrap the bubble widget in the content tree too, so it renders twice
        // (once in-content, once as the bright copy on top of the dim).
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

        controller.show(
            bounds,
            bubble_widget.clone_boxed(),
            test_content_builder("A"),
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Find all TextRenderObjects with content "X" in the tree. There
        // should be two (one in-content, one as the bright copy). Assert
        // their computed_bounds sizes match — this is the dual-render spike
        // gate: if sizes diverge, the dual-render assumption is wrong and the
        // spec's cutout-frame fallback must be used.
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
}
