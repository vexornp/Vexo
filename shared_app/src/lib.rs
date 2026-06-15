use vexo::{run_desktop_demo, Application, Color, Flex, ScrollView, Text, Widget};
uniffi::setup_scaffolding!();

/// Helper to create a ScrollView demo with 20 items.
fn scroll_demo() -> Box<dyn Widget> {
    let mut column = Flex::column().gap(0.0);
    for i in 0..20 {
        let label = format!("Item {}", i + 1);
        column = column.push(
            Text::new(&label)
                .padding(16.0)
                .background(if i % 2 == 0 {
                    Color::rgb(0.95, 0.95, 0.95)
                } else {
                    Color::WHITE
                })
        );
    }
    ScrollView::new(column)
        .width(200.0)
        .height(300.0)
        .border(Color::rgb(0.6, 0.6, 0.6), 1.0)
        .boxed()
}

// --- The User's Code ---
pub struct State;

impl Application for State {
    type State = Self;

    fn new() -> Self::State {
        Self
    }

    fn view(_state: &mut Self::State, _font_system: &mut glyphon::FontSystem) -> Box<dyn Widget> {
        scroll_demo()
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
