use vexo::{
    button, column, color_widget, row, text, text_edit, widgets::Widget, AlignItems, Application,
    Color, WidgetExt,
};
uniffi::setup_scaffolding!();

// --- The User's Code ---
#[derive(Debug, Clone, Copy)]
pub enum Message {
    None,
    Clicked,
}

pub struct State {
    click_count: u32,
}

impl Application for State {
    type Message = Message;
    type State = Self;

    fn new() -> Self::State {
        Self { click_count: 0 }
    }

    fn update(state: &mut Self::State, message: Self::Message) {
        match message {
            Message::Clicked => {
                state.click_count += 1;
            }
            Message::None => {}
        }
    }

    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>> {
        let text_content = format!("You clicked {} times!", state.click_count);

        column![
            align: AlignItems::Center,
            color_widget!(Color::rgb(0.0, 0.1, 0.0))
                .frame(400.0, 150.0)
                .boxed(),
            text_edit!("editor_id_input", "Type here...")
                .frame(100.0, 50.0)
                .boxed(),
            // Example with modifiers
            text!("Modified Text", font_size: 24.0)
                .padding(10.0)
                .background(Color::RED)
                .border(Color::GREEN, 2.0)
                .boxed(),
            button!(text!(text_content, font_size: 24.0), Message::Clicked)
                .padding(10.0)
                .background(Color::rgb(0.1, 0.4, 0.1))
                .border(Color::BLACK, 1.0)
                .boxed(),
            color_widget!(Color::BLUE)
                .frame(150.0, 50.0)
                .boxed(),
            color_widget!(Color::CYAN)
                .frame(110.0, 30.0)
                .boxed(),
            row![
                color_widget!(Color::RED).frame(60.0, 70.0).boxed(),
                color_widget!(Color::YELLOW).frame(90.0, 40.0).boxed(),
            ],
        ]
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
