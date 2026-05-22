use vexo::{reactive::StatefulMutable, retain, widgets::Widget, Application, WidgetExt};
uniffi::setup_scaffolding!();

/// Helper to create a tappable button-like widget using GestureDetector.
/// This replaces the old Button widget with the Flutter-style composition:
/// GestureDetector(DecoratedContainer(Text))
fn tap_button(label: &str, on_press: impl FnMut() + 'static) -> retain::GestureDetector {
    retain::GestureDetector::new(Box::new(
        retain::DecoratedContainer::new(Box::new(retain::Text::new(label)))
            .style(
                retain::Style::new()
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

impl retain::State for RetainCounterState {
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

impl retain::StatefulWidget for RetainCounter {
    type State = RetainCounterState;

    fn build(
        &self,
        state: &mut Self::State,
        _ctx: &mut retain::BuildContext,
    ) -> Box<dyn retain::Widget> {
        let count = state.count.get();

        // Clone StatefulMutable for each callback so they can update the count
        // and trigger a rebuild of this element.
        let dec_count = state.count.clone();
        let inc_count = state.count.clone();
        let reset_count = state.count.clone();

        Box::new(
            retain::Column::new()
                .push(retain::Text::new(&self.label))
                .push(retain::Text::new(format!("Count: {}", count)))
                .push(
                    retain::Row::new()
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

// --- The User's Code ---
#[derive(Debug, Clone)]
pub enum Message {
    None,
    Clicked,
    CounterOutput(CounterOutput),
    ToggleRetainMode,
}

// --- Counter Component ---

#[derive(Clone, Debug)]
pub enum CounterMessage {
    Increment,
    Decrement,
    Reset,
}

#[derive(Clone, Debug)]
pub enum CounterOutput {
    CountReached(u32),
}

#[derive(Default, Clone)]
pub struct CounterState {
    count: u32,
}

pub struct CounterComponent;

impl vexo::component::Component for CounterComponent {
    type Message = CounterMessage;
    type Output = CounterOutput;
    type State = CounterState;

    fn update(state: &mut Self::State, message: Self::Message) {
        match message {
            CounterMessage::Increment => state.count += 1,
            CounterMessage::Decrement => {
                if state.count > 0 {
                    state.count -= 1;
                }
            }
            CounterMessage::Reset => state.count = 0,
        }
    }

    fn view(
        state: &Self::State,
        _ctx: &mut vexo::component::ComponentContext<'_, Self::Message>,
    ) -> Box<dyn vexo::widgets::Widget<Self::Message>> {
        let count_text = format!("Count: {}", state.count);

        vexo::column![
            vexo::text!(count_text).font_size(24.0),
            vexo::row![
                vexo::button!(vexo::text!("-"), CounterMessage::Decrement)
                    .width(40.0)
                    .height(40.0),
                vexo::button!(vexo::text!("+"), CounterMessage::Increment)
                    .width(40.0)
                    .height(40.0),
                vexo::button!(vexo::text!("Reset"), CounterMessage::Reset).height(40.0),
            ]
            .gap(8.0),
        ]
        .align(vexo::layout::AlignItems::Center)
        .padding(16.0)
        .background(vexo::Color::rgb(0.95, 0.95, 0.95))
        .border(vexo::Color::rgb(0.8, 0.8, 0.8), 1.0)
        .corner_radius(8.0)
        .boxed()
    }

    fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output> {
        match message {
            CounterMessage::Increment if state.count == 10 => Some(CounterOutput::CountReached(10)),
            _ => None,
        }
    }
}

pub struct State {
    click_count: u32,
    milestones: u32,
    text_editor_controller: Option<vexo::retain::TextEditingController>,
    editor_a1: Option<vexo::retain::TextEditingController>,
    editor_a2: Option<vexo::retain::TextEditingController>,
    editor_b1: Option<vexo::retain::TextEditingController>,
    editor_b2: Option<vexo::retain::TextEditingController>,
}

impl Application for State {
    type Message = Message;
    type State = Self;

    fn new() -> Self::State {
        Self {
            click_count: 0,
            milestones: 0,
            text_editor_controller: None,
            editor_a1: None,
            editor_a2: None,
            editor_b1: None,
            editor_b2: None,
        }
    }

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

    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>> {
        let _text_content = format!("You clicked {} times!", state.click_count);
        let milestone_text = format!("Milestones reached: {}", state.milestones);

        vexo::column![
            // Hint for retain mode toggle
            vexo::text!("Press R to toggle retain mode")
                .font_size(14.0)
                .padding(4.0),
            // Title
            vexo::text!("ScrollView Demo").font_size(28.0),
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
            vexo::component!(
                CounterComponent,
                "counter",
                |output| Message::CounterOutput(output)
            ),
            // Milestone display
            vexo::text!(milestone_text).font_size(18.0).padding(10.0),
        ]
        .align(vexo::layout::AlignItems::Center)
        .fill()
        .background(vexo::Color::WHITE)
        .boxed()
    }

    fn retain_view(state: &mut Self::State, font_system: &mut glyphon::FontSystem) -> Option<Box<dyn retain::Widget>> {
        // Lazily initialize TextEdit controllers
        if state.text_editor_controller.is_none() {
            state.text_editor_controller = Some(
                vexo::retain::TextEditingController::new("Type here...", font_system)
            );
        }
        if state.editor_a1.is_none() {
            state.editor_a1 = Some(
                vexo::retain::TextEditingController::new("Field A1", font_system)
            );
        }
        if state.editor_a2.is_none() {
            state.editor_a2 = Some(
                vexo::retain::TextEditingController::new("Field A2", font_system)
            );
        }
        if state.editor_b1.is_none() {
            state.editor_b1 = Some(
                vexo::retain::TextEditingController::new("Field B1", font_system)
            );
        }
        if state.editor_b2.is_none() {
            state.editor_b2 = Some(
                vexo::retain::TextEditingController::new("Field B2", font_system)
            );
        }

        let controller = state.text_editor_controller.as_ref().unwrap();
        let a1 = state.editor_a1.as_ref().unwrap();
        let a2 = state.editor_a2.as_ref().unwrap();
        let b1 = state.editor_b1.as_ref().unwrap();
        let b2 = state.editor_b2.as_ref().unwrap();

        Some(Box::new(
            retain::Column::new()
                // Title
                .push(retain::Text::new("Focus Demo"))
                .push(retain::Text::new("Click a field to focus it. Click outside to unfocus."))
                // Group A: Focus fields in a decorated container
                .push(retain::DecoratedContainer::new(Box::new(
                    retain::Column::new()
                        .push(retain::Text::new("Group A"))
                        .push(vexo::retain::Focus::new(retain::TextEdit::new(a1.clone())))
                        .push(vexo::retain::Focus::new(retain::TextEdit::new(a2.clone())))
                ))
                .style(
                    retain::Style::new()
                        .background(vexo::Color::rgb(0.95, 0.95, 1.0))
                        .border(vexo::Color::rgb(0.5, 0.5, 0.8), 1.0)
                        .corner_radius(8.0)
                        .padding(8.0)
                ))
                // Group B: Focus fields in a decorated container
                .push(retain::DecoratedContainer::new(Box::new(
                    retain::Column::new()
                        .push(retain::Text::new("Group B"))
                        .push(vexo::retain::Focus::new(retain::TextEdit::new(b1.clone())))
                        .push(vexo::retain::Focus::new(retain::TextEdit::new(b2.clone())))
                ))
                .style(
                    retain::Style::new()
                        .background(vexo::Color::rgb(1.0, 0.95, 0.95))
                        .border(vexo::Color::rgb(0.8, 0.5, 0.5), 1.0)
                        .corner_radius(8.0)
                        .padding(8.0)
                ))
                // Standalone field
                .push(vexo::retain::Focus::new(retain::TextEdit::new(controller.clone())))
        ))
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
