use vexo::{
    button, color_widget, text, text_edit, widgets::{Widget, Column, Row}, Application,
    Color, WidgetExt, layout::AlignItems,
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

        // Main column fills available space with white background
        Column::new()
            .align(AlignItems::Center)
            .gap(10.0)
            .padding(20.0)
            .push(
                // Blue rectangle with fixed size
                Column::new()
                    .width(200.0)
                    .height(100.0)
                    .push(color_widget!(Color::BLUE))
                    .boxed()
            )
            .push(
                // Text edit with fixed size
                Column::new()
                    .width(100.0)
                    .height(50.0)
                    .push(text_edit!("editor_id_input", "Type here..."))
                    .boxed()
            )
            .push(
                // Text with padding and decorative styling
                Column::new()
                    .padding(10.0)
                    .push(
                        text!("Modified Text", font_size: 24.0)
                            .background(Color::RED)
                            .border(Color::GREEN, 2.0)
                            .corner_radius(8.0)
                            .boxed()
                    )
                    .boxed()
            )
            .push(
                // Button with padding and decorative styling
                Column::new()
                    .padding(10.0)
                    .push(
                        button!(text!(text_content, font_size: 24.0), Message::Clicked)
                            .background(Color::rgb(0.1, 0.4, 0.1))
                            .border(Color::BLACK, 1.0)
                            .corner_radius(8.0)
                            .boxed()
                    )
                    .boxed()
            )
            .push(
                // Blue rectangle with fixed size
                Column::new()
                    .width(150.0)
                    .height(50.0)
                    .push(color_widget!(Color::BLUE))
                    .boxed()
            )
            .push(
                // Cyan rectangle with fixed size
                Column::new()
                    .width(110.0)
                    .height(30.0)
                    .push(color_widget!(Color::CYAN))
                    .boxed()
            )
            .push(
                // Row with two colored rectangles
                Row::new()
                    .push(
                        Column::new()
                            .width(60.0)
                            .height(70.0)
                            .push(color_widget!(Color::RED))
                            .boxed()
                    )
                    .push(
                        Column::new()
                            .width(90.0)
                            .height(40.0)
                            .push(color_widget!(Color::YELLOW))
                            .boxed()
                    )
                    .boxed()
            )
            .fill()
            .background(Color::WHITE)
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
