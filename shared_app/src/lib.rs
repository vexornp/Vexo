use vexo::{
    widgets::{MapWidget, Widget},
    Application, WidgetExt,
};
uniffi::setup_scaffolding!();

// --- The User's Code ---
#[derive(Debug, Clone)]
pub enum Message {
    None,
    Clicked,
    CounterOutput(CounterOutput),
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
                vexo::button!(vexo::text!("Reset"), CounterMessage::Reset)
                    .height(40.0),
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
            CounterMessage::Increment if state.count == 10 => {
                Some(CounterOutput::CountReached(10))
            }
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
        }
    }

    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>> {
        let text_content = format!("You clicked {} times!", state.click_count);
        let milestone_text = format!("Milestones reached: {}", state.milestones);

        // Create the counter component wrapped in MapWidget
        let counter_widget = MapWidget::new(
            Box::new(vexo::component::ComponentWidget::<CounterComponent>::new("counter")),
            |output| Message::CounterOutput(output),
        );

        vexo::column![
            // Title
            vexo::text!("Counter Component Demo")
                .font_size(28.0),
            // Counter Component with message mapping
            counter_widget,
            // Milestone display
            vexo::text!(milestone_text)
                .font_size(18.0)
                .padding(10.0),
            // Existing demo widgets
            vexo::text_edit!("editor_id_input")
                .content("Type here...")
                .width(100.0)
                .height(50.0),
            vexo::column![vexo::text!("Modified Text")
                .font_size(24.0)
                .background(vexo::Color::RED)
                .border(vexo::Color::GREEN, 2.0)
                .corner_radius(8.0)]
            .padding(10.0),
            vexo::column![
                vexo::button!(vexo::text!(text_content).font_size(24.0), Message::Clicked)
                    .background(vexo::Color::rgb(0.1, 0.4, 0.1))
                    .border(vexo::Color::BLACK, 1.0)
                    .corner_radius(8.0)
            ]
            .padding(10.0)
            .background(vexo::Color::BLUE),
            vexo::color_widget!(vexo::Color::CYAN).width(110.0).height(30.0),
            vexo::row![
                vexo::color_widget!(vexo::Color::RED).width(60.0).height(70.0),
                vexo::color_widget!(vexo::Color::YELLOW).width(90.0).height(40.0),
            ],
        ]
        .align(vexo::layout::AlignItems::Center)
        .fill()
        .background(vexo::Color::WHITE)
        .boxed()
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
