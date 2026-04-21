use vexo::{
    button, color_widget, column, grid,
    layout::{AlignItems, TrackSizing},
    row, text, text_edit,
    widgets::Widget,
    Application, Color, WidgetExt,
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
        column![
            // Text edit with fixed size - now using CSS-like layout directly
            text_edit!("editor_id_input")
                .content("Type here...")
                .width(100.0)
                .height(50.0)
                .boxed(),
            // Text with padding and decorative styling
            column![text!("Modified Text", font_size: 24.0)
                .background(Color::RED)
                .border(Color::GREEN, 2.0)
                .corner_radius(8.0)
                .boxed()]
            .padding(10.0)
            .boxed(),
            // Button with padding and decorative styling
            column![
                button!(text!(text_content, font_size: 24.0).boxed(), Message::Clicked)
                    .background(Color::rgb(0.1, 0.4, 0.1))
                    .border(Color::BLACK, 1.0)
                    .corner_radius(8.0)
                    .boxed()
            ]
            .padding(10.0)
            .background(Color::BLUE)
            .boxed(),
            // Cyan rectangle with fixed size - now using CSS-like layout directly
            color_widget!(Color::CYAN)
                .width(110.0)
                .height(30.0)
                .boxed(),
            // Row with two colored rectangles - now using CSS-like layout directly
            row![
                color_widget!(Color::RED)
                    .width(60.0)
                    .height(70.0)
                    .boxed(),
                color_widget!(Color::YELLOW)
                    .width(90.0)
                    .height(40.0)
                    .boxed(),
            ]
            .boxed(),
            // Grid demonstration: 2x3 grid with different sized cells
            column![
                text!("Grid Demo (2x3):", font_size: 18.0).boxed(),
                grid![
                    columns: vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)],
                    rows: vec![
                        TrackSizing::Px(40.0),
                        TrackSizing::Px(40.0),
                        TrackSizing::Px(40.0),
                    ],
                    text!("Cell 1,1").background(Color::RED).boxed(),
                    text!("Cell 1,2 (2x wide)").background(Color::GREEN).boxed(),
                    text!("Cell 2,1").background(Color::BLUE).boxed(),
                    text!("Cell 2,2").background(Color::YELLOW).boxed(),
                    text!("Cell 3,1").background(Color::MAGENTA).boxed(),
                    text!("Cell 3,2").background(Color::CYAN).boxed(),
                ]
                .border(Color::BLACK, 2.0)
                .boxed(),
            ]
            .boxed(),
        ]
        .align(AlignItems::Center)
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
