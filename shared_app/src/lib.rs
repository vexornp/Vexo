use vexo::{Application, Color, Column, ComponentState, Text, Widget};

uniffi::setup_scaffolding!();

#[derive(ComponentState, Default)]
pub struct State {
    _placeholder: (),
}

impl Application for State {
    type State = Self;

    fn new() -> Self::State {
        Self::State { _placeholder: () }
    }

    fn view(_state: &mut Self::State) -> Box<dyn Widget> {
        let mut column = Column::new().gap(0.0);
        for i in 0..20 {
            let label = format!("Row {}", i + 1);
            column = column.push(Text::new(&label).padding(16.0).background(if i % 2 == 0 {
                Color::rgb(0.95, 0.95, 0.95)
            } else {
                Color::WHITE
            }));
        }
        column.boxed()
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
