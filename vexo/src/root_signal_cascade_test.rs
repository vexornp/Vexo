//! Regression test for state-driven root rebuild cascading through the
//! InheritedWidget chain to a descendant `Component` that reads a `Signal`
//! clone.
//!
//! This mirrors the desktop Chats app structure:
//!   `RootComponent` (State owns `selected: Signal`)
//!     → `RootMediaQuery` (Component)
//!       → `MediaQuery` (InheritedWidget, Rc child)
//!         → `Theme` (InheritedWidget, Box child)
//!           → `Shell` (Component, like DesktopShell)
//!             → `Reader` (Component, like DesktopChatsPage — reads the
//!               signal clone in `render()`)
//!
//! The state-driven path (`rebuild_from_state`, triggered by `Signal::set`)
//! is what actually runs in the app when a conversation is tapped. Existing
//! InheritedWidget tests only exercise the full-`reconcile` (tree-replacement)
//! path, which does NOT cover this cascade. A `Signal::set` on the app state
//! must re-run `Application::view()` and the resulting new widget tree must
//! propagate through every `InheritedElement::update()` (which short-circuits
//! on equal child pointers) down to the `Reader`.

use std::cell::RefCell;
use std::sync::Arc;

use crate::animation::AnimationTicker;
use crate::layout::TaffyLayoutEngine;
use crate::reactive::Signal;
use crate::stateful_widget::RenderContext;
use crate::widgets::{Shared, Text, Theme, ThemeData};
use crate::{
    Application, Component, ComponentState, RootComponent, SimpleState, ThreeTreePipeline, Widget,
};

thread_local! {
    /// A wired clone of the root state's `selected` signal, published by
    /// `RootState::set_dirty_callback` during root mount. The test retrieves
    /// it after `pipeline.update(...)` so it can drive `set_from` with the
    /// dirty callback actually wired (mirroring how app-level signals fire).
    static PUBLISHED: RefCell<Option<Signal<Option<u32>>>> = const { RefCell::new(None) };
}

fn take_published() -> Signal<Option<u32>> {
    PUBLISHED.with(|p| {
        p.borrow_mut()
            .take()
            .expect("root mounted → published clone")
    })
}

struct RootState {
    selected: Signal<Option<u32>>,
}

impl Default for RootState {
    fn default() -> Self {
        Self {
            selected: Signal::new(None),
        }
    }
}

impl ComponentState for RootState {
    fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.selected.set_dirty_callback(callback);
        // Publish a clone AFTER wiring so the test's `set_from` fires the
        // root dirty callback. (A clone taken before wiring would share the
        // Mutable but carry a `None` on_change and thus never fire.)
        PUBLISHED.with(|p| *p.borrow_mut() = Some(self.selected.clone()));
    }
}

struct TestApp;

impl Application for TestApp {
    type State = RootState;

    fn new() -> Self::State {
        RootState::default()
    }

    fn view(state: &mut Self::State) -> Box<dyn Widget> {
        // Same shape as `ImState::view`: Theme(theme, Shell{selected}).
        Theme::new(
            ThemeData::light(),
            Shell {
                selected: state.selected.clone(),
            },
        )
        .boxed()
    }

    fn register_fonts(_font_system: &mut glyphon::FontSystem) {}
}

/// Intermediate `Component` (DesktopShell analog): rebuilds on parent cascade
/// and re-creates the `Reader` widget with a fresh signal clone each render.
#[derive(Clone)]
struct Shell {
    selected: Signal<Option<u32>>,
}

impl Component for Shell {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        Reader {
            selected: self.selected.clone(),
        }
        .boxed()
    }
}

/// Leaf `Component` (ChatScreen analog): reads the signal clone in
/// `render()` and wraps the output in `Shared` (Rc pointer comparison) —
/// mirroring the fixed `KeyboardAvoider`. `Shared` skips the cascade when
/// the child Rc is unchanged (keyboard frames) but reconciles when the Rc
/// changes (conversation switch).
#[derive(Clone)]
struct Reader {
    selected: Signal<Option<u32>>,
}

impl Component for Reader {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let text = format!("v={:?}", self.selected.get_cloned());
        Shared::new(std::rc::Rc::new(Text::new(text)) as std::rc::Rc<dyn Widget>).boxed()
    }
}

fn rendered_text(pipeline: &mut ThreeTreePipeline) -> String {
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = crate::resource::new_font_system();
    pipeline.layout(
        crate::core::Size::new(400.0, 300.0),
        &mut engine,
        &mut font_system,
    );
    let commands = pipeline.paint();
    commands
        .iter()
        .filter_map(|cmd| match cmd {
            crate::render::RenderCommand::Text { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn root_signal_change_cascades_through_inherited_chain_to_reader() {
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Initial mount — mirrors the first-frame `pipeline.update(RootComponent)`.
    pipeline.update(RootComponent::<TestApp>::default().boxed());
    let published = take_published();

    assert_eq!(
        rendered_text(&mut pipeline),
        "v=None",
        "initial render should reflect the default signal value"
    );

    // First selection: None → Some(1). Fires the root dirty callback.
    published.set_from(&Some(1));
    pipeline.perform_rebuilds();
    assert_eq!(
        rendered_text(&mut pipeline),
        "v=Some(1)",
        "first Signal::set must cascade through the InheritedWidget chain to the Reader"
    );

    // Switch to a different conversation: Some(1) → Some(2). This is the
    // reported bug — the chat screen doesn't update when clicking a different
    // conversation. The cascade must reach the Reader again.
    published.set_from(&Some(2));
    pipeline.perform_rebuilds();
    assert_eq!(
        rendered_text(&mut pipeline),
        "v=Some(2)",
        "second Signal::set must also cascade — switching conversations must update the Reader"
    );
}
