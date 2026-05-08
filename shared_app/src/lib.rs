use vexo::{retain, widgets::Widget, Application, WidgetExt};
uniffi::setup_scaffolding!();

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
        ctx: &mut vexo::component::ComponentContext<'_, Self::Message>,
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
}

impl Application for State {
    type Message = Message;
    type State = Self;

    fn new() -> Self::State {
        Self {
            click_count: 0,
            milestones: 0,
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
        let text_content = format!("You clicked {} times!", state.click_count);
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

    fn retain_view(_state: &Self::State) -> Option<Box<dyn retain::Widget>> {
        // Retain mode widgets use callbacks for event handling.
        // The callbacks are set via .on_press() and will be invoked
        // when the button is clicked.
        //
        // Note: Callbacks currently don't trigger state updates because
        // retain_view receives an immutable state reference. Future work
        // will integrate Mutable<T> for reactive state.

        Some(Box::new(
            retain::Column::new()
                // Header
                .push(retain::Text::new("Retain Mode Widget Demo"))
                // Button controls in a Row
                .push(
                    retain::Row::new()
                        .push(retain::Button::new("Increment (+)")
                            .on_press(|| {
                                println!("Increment button clicked");
                            }))
                        .push(retain::Button::new("Decrement (-)")
                            .on_press(|| {
                                println!("Decrement button clicked");
                            }))
                        .push(retain::Button::new("Reset")
                            .on_press(|| {
                                println!("Reset button clicked");
                            }))
                )
                // Counter display (placeholder text)
                .push(retain::Text::new("Count: 0"))
                // Container demo: Row with two Columns
                .push(retain::Text::new("--- Container Layout ---"))
                .push(
                    retain::Row::new()
                        .push(
                            retain::Column::new()
                                .push(retain::Text::new("Left Column"))
                                .push(retain::Button::new("Button L")
                                    .on_press(|| {
                                        println!("Left button clicked");
                                    }))
                        )
                        .push(
                            retain::Column::new()
                                .push(retain::Text::new("Right Column"))
                                .push(retain::Button::new("Button R")
                                    .on_press(|| {
                                        println!("Right button clicked");
                                    }))
                        )
                )
                // DecoratedContainer demo - single element for multiple decorations
                .push(retain::Text::new("--- DecoratedContainer Demo ---"))
                .push(
                    // This creates 1 element + 1 render object instead of 3 + 3
                    retain::DecoratedContainer::new(
                        Box::new(retain::Text::new("Styled with DecoratedContainer!"))
                    )
                    .style(
                        retain::Style::new()
                            .background(vexo::Color::rgb(0.2, 0.6, 0.9))
                            .border(vexo::Color::rgb(0.1, 0.3, 0.5), 2.0)
                            .corner_radius(12.0)
                    )
                )
                .push(
                    // Another example with different styling
                    retain::DecoratedContainer::new(
                        Box::new(retain::Column::new()
                            .push(retain::Text::new("Multi-line"))
                            .push(retain::Text::new("decorated content"))
                        )
                    )
                    .style(
                        retain::Style::new()
                            .background(vexo::Color::rgb(0.95, 0.95, 0.95))
                            .border(vexo::Color::rgb(0.7, 0.7, 0.7), 1.0)
                            .corner_radius(6.0)
                    )
                )
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
