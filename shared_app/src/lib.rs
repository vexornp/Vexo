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

        // Text color showcase: named colors, custom RGB, and alpha.
        let color_section_title = Text::new("Text Color Showcase").with_font_size(24.0);
        let red_text = Text::new("Red (named)").with_color(Color::RED);
        let green_text = Text::new("Green (named)").with_color(Color::GREEN);
        let blue_text = Text::new("Blue (named)").with_color(Color::BLUE);
        let yellow_text = Text::new("Yellow (named)").with_color(Color::YELLOW);
        let magenta_text = Text::new("Magenta (named)").with_color(Color::MAGENTA);
        let cyan_text = Text::new("Cyan (named)").with_color(Color::CYAN);
        let gray_text = Text::new("Gray (named)").with_color(Color::GRAY);
        let custom_orange = Text::new("Custom orange (rgb)").with_color(Color::rgb(1.0, 0.5, 0.0));
        let custom_purple = Text::new("Custom purple (rgb)").with_color(Color::rgb(0.5, 0.0, 0.8));
        let from_hex_teal =
            Text::new("Custom teal (from_hex 0x008080FF)").with_color(Color::from_hex(0x008080FF));
        let from_hex_coral =
            Text::new("Custom coral (from_hex 0xFF7F50FF)").with_color(Color::from_hex(0xFF7F50FF));
        let semi_transparent = Text::new("Semi-transparent red (alpha 0.5)")
            .with_color(Color::new(1.0, 0.0, 0.0, 0.5));

        let count = state.count.clone();

        Column::new()
            .gap(16.0)
            .padding(24.0)
            .background(Color::WHITE)
            .push(title)
            .push(subtitle)
            .push(color_section_title)
            .push(red_text)
            .push(green_text)
            .push(blue_text)
            .push(yellow_text)
            .push(magenta_text)
            .push(cyan_text)
            .push(gray_text)
            .push(custom_orange)
            .push(custom_purple)
            .push(from_hex_teal)
            .push(from_hex_coral)
            .push(semi_transparent)
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
