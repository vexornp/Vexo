use vexo::{column, color_widget, row, text, text_edit, button, widgets::Widget, AlignItems, Application, Color};
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
            color_widget!(400.0, 150.0, Color::rgb(0.0, 0.1, 0.0)),
            text_edit!("editor_id_input", "Type here...", size: (100.0, 50.0)),
            button!(text!(text_content, size: 24.0), Message::Clicked, color: Color::rgb(0.1, 0.4, 0.1)),
            color_widget!(150.0, 50.0, Color::BLUE),
            color_widget!(110.0, 30.0, Color::CYAN),
            row![
                color_widget!(60.0, 70.0, Color::RED),
                color_widget!(90.0, 40.0, Color::YELLOW),
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
