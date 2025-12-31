use vexo::{
    widgets::{Button, Column, Rectangle, Row, Text, TextEdit, Widget},
    Application, FrameworkState,
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

impl State {
    fn create_rect_box(&self, w: f32, h: f32, color: [f32; 3]) -> Box<dyn Widget<Message>> {
        Box::new(Rectangle::new(w, h, color))
    }
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
        let rect1 = state.create_rect_box(60.0, 70.0, [1.0, 0.0, 0.0]);
        let rect2 = state.create_rect_box(90.0, 40.0, [1.0, 1.0, 0.0]);
        let rect3 = state.create_rect_box(150.0, 150.0, [0.0, 0.1, 0.0]);
        let rect4 = state.create_rect_box(150.0, 50.0, [0.0, 0.0, 1.0]);
        let rect5 = state.create_rect_box(110.0, 30.0, [0.0, 1.0, 1.0]);
        let text_edit =
            Box::new(TextEdit::new("editor_id_input", "Type here...").size((100.0, 50.0)));

        let row = Row::new().push(rect1).push(rect2);

        let clm = Column::new()
            .push(rect3)
            .push(text_edit)
            .push(Box::new(
                Button::new(
                    Box::new(Text::new(text_content).size(24.0)),
                    Message::Clicked,
                )
                .color([0.1, 0.4, 0.1]),
            ))
            .push(rect4)
            .push(rect5)
            .push(Box::new(row));
        Box::new(clm)
    }
}

#[derive(uniffi::Object)]
pub struct MobileApp {}

static mut GLOBAL_FS: Option<FrameworkState<State>> = None;

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
