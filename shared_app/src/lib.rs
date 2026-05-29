use vexo::{reactive::StatefulMutable, Application, Widget, Text, Column, Row, DecoratedContainer, GestureDetector, TextEdit, Style, State as RetainState, StatefulWidget, BuildContext};
uniffi::setup_scaffolding!();

/// Helper to create a tappable button-like widget using GestureDetector.
/// This replaces the old Button widget with the Flutter-style composition:
/// GestureDetector(DecoratedContainer(Text))
fn tap_button(label: &str, on_press: impl FnMut() + 'static) -> GestureDetector {
    GestureDetector::new(Box::new(
        DecoratedContainer::new(Box::new(Text::new(label)))
            .style(
                Style::new()
                    .background(vexo::Color::rgb(0.9, 0.9, 0.9))
                    .border(vexo::Color::rgb(0.6, 0.6, 0.6), 1.0)
                    .corner_radius(8.0)
                    .padding(24.0),
            ),
    ))
    .on_press(on_press)
}

// --- Retain Mode Counter StatefulWidget ---

/// Counter widget configuration for retain mode.
/// This demonstrates StatefulWidget with persistent mutable state.
#[derive(Clone)]
struct RetainCounter {
    label: String,
}

/// State for the RetainCounter that persists across rebuilds.
/// Uses StatefulMutable for reactive count that triggers rebuilds on change.
struct RetainCounterState {
    count: StatefulMutable<u32>,
}

impl RetainState for RetainCounterState {
    fn set_dirty_callback(&mut self, cb: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.count.set_dirty_callback(cb);
    }
}

impl Default for RetainCounterState {
    fn default() -> Self {
        Self {
            count: StatefulMutable::new(0),
        }
    }
}

impl StatefulWidget for RetainCounter {
    type State = RetainCounterState;

    fn build(
        &self,
        state: &mut Self::State,
        _ctx: &mut BuildContext,
    ) -> Box<dyn Widget> {
        let count = state.count.get();

        // Clone StatefulMutable for each callback so they can update the count
        // and trigger a rebuild of this element.
        let dec_count = state.count.clone();
        let inc_count = state.count.clone();
        let reset_count = state.count.clone();

        Box::new(
            Column::new()
                .push(Text::new(&self.label))
                .push(Text::new(format!("Count: {}", count)))
                .push(
                    Row::new()
                        .push(tap_button("-", move || {
                            let cur = dec_count.get();
                            if cur > 0 {
                                dec_count.set(cur - 1);
                            }
                        }))
                        .push(tap_button("+", move || {
                            let cur = inc_count.get();
                            inc_count.set(cur + 1);
                        }))
                        .push(tap_button("Reset", move || {
                            reset_count.set(0);
                        })),
                ),
        )
    }
}

// --- HoverableCard StatefulWidget ---

/// A card that changes border on hover. Owns its own hover state
/// so only this element rebuilds on mouse enter/exit.
#[derive(Clone)]
struct HoverableCard {
    title: String,
    editors: Vec<vexo::TextEditingController>,
}

struct HoverableCardState {
    hovered: StatefulMutable<bool>,
}

impl RetainState for HoverableCardState {
    fn set_dirty_callback(&mut self, cb: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.hovered.set_dirty_callback(cb);
    }
}

impl Default for HoverableCardState {
    fn default() -> Self {
        Self {
            hovered: StatefulMutable::new(false),
        }
    }
}

impl HoverableCard {
    fn new(title: &str, editors: Vec<vexo::TextEditingController>) -> Self {
        Self {
            title: title.to_string(),
            editors,
        }
    }
}

impl StatefulWidget for HoverableCard {
    type State = HoverableCardState;

    fn build(&self, state: &mut Self::State, _ctx: &mut BuildContext) -> Box<dyn Widget> {
        let hovered = state.hovered.clone();
        let border_color = if hovered.get() {
            vexo::Color::rgb(0.2, 0.4, 1.0)
        } else {
            vexo::Color::rgb(0.5, 0.5, 0.8)
        };
        let border_width = if hovered.get() { 2.5 } else { 1.0 };

        let mut column = Column::new().push(Text::new(&self.title));
        for editor in &self.editors {
            column = column.push(vexo::Focus::new(TextEdit::new(editor.clone())));
        }

        Box::new(
            vexo::MouseRegion::new(Box::new(
                DecoratedContainer::new(Box::new(column))
                    .style(
                        Style::new()
                            .background(vexo::Color::rgb(0.95, 0.95, 1.0))
                            .border(border_color, border_width)
                            .corner_radius(8.0)
                            .padding(8.0)
                    ),
            ))
            .cursor(vexo::input::MouseCursor::System(vexo::SystemCursorKind::Pointer))
            .on_enter({
                let h = hovered.clone();
                move || h.set(true)
            })
            .on_exit({
                let h = hovered.clone();
                move || h.set(false)
            })
        )
    }
}

// --- The User's Code ---
pub struct State {
    text_editor_controller: Option<vexo::TextEditingController>,
    editor_a1: Option<vexo::TextEditingController>,
    editor_a2: Option<vexo::TextEditingController>,
    editor_b1: Option<vexo::TextEditingController>,
    editor_b2: Option<vexo::TextEditingController>,
}

impl Application for State {
    type State = Self;

    fn new() -> Self::State {
        Self {
            text_editor_controller: None,
            editor_a1: None,
            editor_a2: None,
            editor_b1: None,
            editor_b2: None,
        }
    }

    fn view(state: &mut Self::State, font_system: &mut glyphon::FontSystem) -> Box<dyn Widget> {
        // Lazily initialize TextEdit controllers
        if state.text_editor_controller.is_none() {
            state.text_editor_controller = Some(
                vexo::TextEditingController::new("Type here...", font_system)
            );
        }
        if state.editor_a1.is_none() {
            state.editor_a1 = Some(
                vexo::TextEditingController::new("Field A1", font_system)
            );
        }
        if state.editor_a2.is_none() {
            state.editor_a2 = Some(
                vexo::TextEditingController::new("Field A2", font_system)
            );
        }
        if state.editor_b1.is_none() {
            state.editor_b1 = Some(
                vexo::TextEditingController::new("Field B1", font_system)
            );
        }
        if state.editor_b2.is_none() {
            state.editor_b2 = Some(
                vexo::TextEditingController::new("Field B2", font_system)
            );
        }

        let controller = state.text_editor_controller.as_ref().unwrap();
        let a1 = state.editor_a1.as_ref().unwrap();
        let a2 = state.editor_a2.as_ref().unwrap();
        let b1 = state.editor_b1.as_ref().unwrap();
        let b2 = state.editor_b2.as_ref().unwrap();

        Box::new(
            Column::new()
                // Title
                .push(Text::new("Focus Demo"))
                .push(Text::new("Click a field to focus it. Click outside to unfocus."))
                // Group A: Focus fields in a hoverable card
                .push(HoverableCard::new(
                    "Group A",
                    vec![a1.clone(), a2.clone()],
                ))
                // Group B: Focus fields in a decorated container
                .push(DecoratedContainer::new(Box::new(
                    Column::new()
                        .push(Text::new("Group B"))
                        .push(vexo::Focus::new(TextEdit::new(b1.clone())))
                        .push(vexo::Focus::new(TextEdit::new(b2.clone())))
                ))
                .style(
                    Style::new()
                        .background(vexo::Color::rgb(1.0, 0.95, 0.95))
                        .border(vexo::Color::rgb(0.8, 0.5, 0.5), 1.0)
                        .corner_radius(8.0)
                        .padding(8.0)
                ))
                // Standalone field
                .push(vexo::Focus::new(TextEdit::new(controller.clone())))
        )
    }
}

#[derive(uniffi::Object)]
pub struct MobileApp {}

#[uniffi::export]
impl MobileApp {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {}
    }

    pub fn start_app(&self) {
        let rt = vexo::run_desktop_demo::<State>();
        match rt {
            Ok(_) => println!("App exited normally"),
            Err(e) => println!("App exited with error: {:?}", e),
        }
    }
}