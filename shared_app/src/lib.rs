use vexo::{Application, Color, Column, ComponentState, Signal, Text, Widget};
use vexo_uikit::{Button, ButtonVariant};

uniffi::setup_scaffolding!();

#[derive(ComponentState, Default)]
pub struct State {
    count: Signal<u32>,
}

impl Application for State {
    type State = Self;

    fn new() -> Self::State {
        Self::State::default()
    }

    fn view(state: &mut Self::State) -> Box<dyn Widget> {
        let pressed = state.count.get();

        let title = Text::new("Button Showcase").with_font_size(32.0);
        let subtitle = Text::new(format!("Pressed: {} times", pressed));

        let count = state.count.clone();

        Column::new()
            .gap(16.0)
            .padding(24.0)
            .background(Color::WHITE)
            .push(title)
            .push(subtitle)
            .push(
                Button::new("Submit")
                    .variant(ButtonVariant::Primary)
                    .on_press(move || {
                        count.set(count.get() + 1);
                    }),
            )
            .push({
                let count = state.count.clone();
                Button::new("Cancel")
                    .variant(ButtonVariant::Secondary)
                    .on_press(move || {
                        count.set(count.get() + 1);
                    })
            })
            .push({
                let count = state.count.clone();
                Button::new("Delete")
                    .variant(ButtonVariant::Destructive)
                    .on_press(move || {
                        count.set(count.get() + 1);
                    })
            })
            .push({
                let count = state.count.clone();
                Button::new("More")
                    .variant(ButtonVariant::Ghost)
                    .on_press(move || {
                        count.set(count.get() + 1);
                    })
            })
            .push(
                Button::new("Submit")
                    .variant(ButtonVariant::Primary)
                    .disabled(true),
            )
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
