use vexo::{
    column, input::MouseCursor, reactive::StatefulMutable, row, run_desktop_demo, Application,
    BuildContext, Color, DecoratedContainer, Focus, GestureDetector, MouseRegion,
    State as RetainState, StatefulWidget, Style, SystemCursorKind, Text, TextEdit,
    TextEditingController, Transform, Widget,
};
uniffi::setup_scaffolding!();

/// Helper to create a tappable button-like widget using GestureDetector.
fn tap_button(label: &str, on_press: impl FnMut() + 'static) -> GestureDetector {
    GestureDetector::new(
        DecoratedContainer::new(Text::new(label)).style(
            Style::new()
                .background(Color::rgb(0.9, 0.9, 0.9))
                .border(Color::rgb(0.6, 0.6, 0.6), 1.0)
                .corner_radius(8.0)
                .padding(24.0),
        ),
    )
    .on_press(on_press)
}

// --- Retain Mode Counter StatefulWidget ---

#[derive(Clone)]
struct RetainCounter {
    label: String,
}

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

    fn build(&self, state: &mut Self::State, _ctx: &mut BuildContext) -> Box<dyn Widget> {
        let count = state.count.get();
        let dec_count = state.count.clone();
        let inc_count = state.count.clone();
        let reset_count = state.count.clone();

        Box::new(column![
            Text::new(&self.label),
            Text::new(format!("Count: {}", count)),
            row![
                tap_button("-", move || {
                    let cur = dec_count.get();
                    if cur > 0 {
                        dec_count.set(cur - 1);
                    }
                }),
                tap_button("+", move || {
                    let cur = inc_count.get();
                    inc_count.set(cur + 1);
                }),
                tap_button("Reset", move || {
                    reset_count.set(0);
                })
            ]
        ])
    }
}

// --- HoverableCard StatefulWidget ---

#[derive(Clone)]
struct HoverableCard {
    title: String,
    editors: Vec<TextEditingController>,
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
    fn new(title: &str, editors: Vec<TextEditingController>) -> Self {
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
            Color::rgb(0.2, 0.4, 1.0)
        } else {
            Color::rgb(0.5, 0.5, 0.8)
        };
        let border_width = if hovered.get() { 2.5 } else { 1.0 };

        let mut column = column![Text::new(&self.title)];
        for editor in &self.editors {
            column = column.push(Focus::new(TextEdit::new(editor.clone())));
        }

        Box::new(
            MouseRegion::new(
                DecoratedContainer::new(column).style(
                    Style::new()
                        .background(Color::rgb(0.95, 0.95, 1.0))
                        .border(border_color, border_width)
                        .corner_radius(8.0)
                        .padding(8.0),
                ),
            )
            .cursor(MouseCursor::System(SystemCursorKind::Pointer))
            .on_enter({
                let h = hovered.clone();
                move || h.set(true)
            })
            .on_exit({
                let h = hovered.clone();
                move || h.set(false)
            }),
        )
    }
}

// --- The User's Code ---
pub struct State {
    text_editor_controller: Option<TextEditingController>,
    editor_a1: Option<TextEditingController>,
    editor_a2: Option<TextEditingController>,
    editor_b1: Option<TextEditingController>,
    editor_b2: Option<TextEditingController>,
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
        if state.text_editor_controller.is_none() {
            state.text_editor_controller =
                Some(TextEditingController::new("Type here...", font_system));
        }
        if state.editor_a1.is_none() {
            state.editor_a1 = Some(TextEditingController::new("Field A1", font_system));
        }
        if state.editor_a2.is_none() {
            state.editor_a2 = Some(TextEditingController::new("Field A2", font_system));
        }
        if state.editor_b1.is_none() {
            state.editor_b1 = Some(TextEditingController::new("Field B1", font_system));
        }
        if state.editor_b2.is_none() {
            state.editor_b2 = Some(TextEditingController::new("Field B2", font_system));
        }

        let controller = state.text_editor_controller.as_ref().unwrap();
        let a1 = state.editor_a1.as_ref().unwrap();
        let a2 = state.editor_a2.as_ref().unwrap();
        let b1 = state.editor_b1.as_ref().unwrap();
        let b2 = state.editor_b2.as_ref().unwrap();

        Box::new(column![
            Text::new("Focus Demo"),
            Text::new("Click a field to focus it. Click outside to unfocus."),
            HoverableCard::new("Group A", vec![a1.clone(), a2.clone()]),
            DecoratedContainer::new(column![
                Text::new("Group B"),
                Focus::new(TextEdit::new(b1.clone())),
                Focus::new(TextEdit::new(b2.clone()))
            ])
            .style(
                Style::new()
                    .background(Color::rgb(1.0, 0.95, 0.95))
                    .border(Color::rgb(0.8, 0.5, 0.5), 1.0)
                    .corner_radius(8.0)
                    .padding(8.0)
            ),
            Focus::new(TextEdit::new(controller.clone())),
            // Transformed elements
            Text::new("Transforms:"),
            row![
                Transform::rotate(
                    DecoratedContainer::new(Text::new("Rotated 10°")).style(
                        Style::new()
                            .background(Color::rgb(0.85, 1.0, 0.85))
                            .padding(8.0)
                    ),
                    10.0_f32.to_radians(),
                ),
                // Scaled card (1.5x)
                Transform::scale(
                    DecoratedContainer::new(Text::new("1.5x")).style(
                        Style::new()
                            .background(Color::rgb(1.0, 0.9, 0.85))
                            .padding(8.0)
                    ),
                    1.5,
                    1.5,
                ),
                // Translated card
                Transform::translate(
                    DecoratedContainer::new(Text::new("Shifted")).style(
                        Style::new()
                            .background(Color::rgb(0.85, 0.9, 1.0))
                            .padding(8.0)
                    ),
                    100.0,
                    100.0,
                ),
            ]
        ])
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
        let rt = run_desktop_demo::<State>();
        match rt {
            Ok(_) => println!("App exited normally"),
            Err(e) => println!("App exited with error: {:?}", e),
        }
    }
}
