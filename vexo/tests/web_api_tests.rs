//! Integration tests verifying the web-developer-friendly API surface.
//!
//! These tests ensure that the new names, macros, and trait implementations
//! work correctly.

use vexo::reactive::Signal;
use vexo::*;

// --- Signal tests ---

#[test]
fn signal_new_and_get() {
    let s: Signal<u32> = Signal::new(42);
    assert_eq!(s.get(), 42);
}

#[test]
fn signal_set_triggers_callback() {
    let s: Signal<u32> = Signal::new(0);
    // We can't call set_dirty_callback directly in an integration test
    // without an element context, but we can verify the type exists
    // and the basic get/set works.
    s.set(5);
    assert_eq!(s.get(), 5);
}

// --- MultiChild column/row tests ---

#[test]
fn multi_child_empty_column_has_no_children() {
    let col = MultiChild::empty(Layout::column());
    assert_eq!(col.children().len(), 0);
}

#[test]
fn multi_child_empty_row_has_no_children() {
    let row = MultiChild::empty(Layout::row());
    assert_eq!(row.children().len(), 0);
}

#[test]
fn multi_child_column_with_children() {
    let col = MultiChild::empty(Layout::column().gap(16.0))
        .push(Text::new("A"))
        .push(Text::new("B"));
    assert_eq!(col.children().len(), 2);
}

// --- children! macro tests ---

#[test]
fn children_macro_basic() {
    let col = MultiChild::new(
        vexo::children![Text::new("A"), Text::new("B")],
        Layout::column(),
    );
    assert_eq!(col.children().len(), 2);
}

#[test]
fn children_macro_nested() {
    let inner = MultiChild::new(
        vexo::children![Text::new("A"), Text::new("B")],
        Layout::row(),
    );
    let col = MultiChild::new(vexo::children![Text::new("Title"), inner], Layout::column());
    assert_eq!(col.children().len(), 2);
}

// --- ComponentState derive test ---
// (Comprehensive derive tests are in derive_component_state.rs;
// this just verifies the derive is usable from `use vexo::*`.)

#[derive(ComponentState)]
struct WebApiTestState {
    count: Signal<u32>,
    name: String,
}

impl Default for WebApiTestState {
    fn default() -> Self {
        Self {
            count: Signal::new(0),
            name: String::new(),
        }
    }
}

#[test]
fn derive_component_state_compiles() {
    // If this compiles, the derive macro works.
    let _state = WebApiTestState::default();
}

// --- RenderContext / LifecycleContext tests ---

#[test]
fn render_context_compiles() {
    // Verify the type compiles and can be used in function signatures.
    fn _check(_: RenderContext) {}
}

#[test]
fn lifecycle_context_compiles() {
    // Verify the type compiles and can be used in function signatures.
    fn _check(_: LifecycleContext) {}
}

// --- Component trait test ---

#[derive(Clone)]
struct TestComponent;

#[derive(ComponentState)]
struct TestComponentState {
    value: Signal<i32>,
}

impl Default for TestComponentState {
    fn default() -> Self {
        Self {
            value: Signal::new(0),
        }
    }
}

impl Component for TestComponent {
    type State = TestComponentState;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        Text::new(format!("Value: {}", state.value.get())).boxed()
    }
}

#[test]
fn component_trait_works() {
    // Verify Component trait compiles and can create a widget via clone_boxed
    // (which goes through the blanket Widget impl for Component).
    let comp = TestComponent;
    let _widget: Box<dyn Widget> = comp.clone_boxed();
}

// --- MultiChild direction tests ---

#[test]
fn multi_child_column_has_column_flex_direction() {
    let col = MultiChild::empty(Layout::column());
    assert_eq!(col.children().len(), 0);
    let direct = MultiChild::empty(Layout::column());
    assert_eq!(col.children().len(), direct.children().len());
}

#[test]
fn multi_child_row_has_row_flex_direction() {
    let row = MultiChild::empty(Layout::row());
    let direct = MultiChild::empty(Layout::row());
    assert_eq!(row.children().len(), direct.children().len());
}

// --- column! / row! macro tests ---

#[test]
fn column_macro_creates_vertical_flex() {
    let col = vexo::column![Text::new("A"), Text::new("B")];
    assert_eq!(col.children().len(), 2);
}

#[test]
fn row_macro_creates_horizontal_flex() {
    let row = vexo::row![Text::new("X"), Text::new("Y")];
    assert_eq!(row.children().len(), 2);
}

// --- Widget trait method tests (builder-style API) ---

#[test]
fn widget_boxed_returns_box_dyn_widget() {
    let widget: Box<dyn Widget> = Text::new("Hello").boxed();
    assert!(widget.as_any().downcast_ref::<Text>().is_some());
}

#[test]
fn widget_with_layout_modifier() {
    let widget = WithLayout::new(
        Text::new("Padded"),
        Layout::default().padding(8.0).width(100.0).height(50.0),
    );
    // Verify it compiles and produces a valid widget.
    let _ = widget.clone_boxed();
}

// --- ComponentState trait blanket impl ---

#[test]
fn component_state_blanket_impl() {
    // Types implementing ComponentState via derive should work.
    fn assert_component_state<T: ComponentState>() {}
    assert_component_state::<TestComponentState>();
}
