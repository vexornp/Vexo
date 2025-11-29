use vexo::run_desktop_demo;
use vexo::{Application, Button, Column, Rectangle, Row, Text, Widget};

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

        let row = Row::new()
            .push(Box::new(Rectangle {
                width: 60.0,
                height: 70.0,
                color: [1.0, 0.0, 0.0],
            }))
            .push(Box::new(Rectangle {
                width: 90.0,
                height: 40.0,
                color: [1.0, 1.0, 0.0],
            }));

        let clm = Column::new()
            .push(Box::new(
                Button::new(
                    Box::new(Text::new(text_content).size(24.0)),
                    Message::Clicked,
                )
                .color([0.1, 0.4, 0.1]),
            ))
            .push(Box::new(Rectangle {
                width: 150.0,
                height: 50.0,
                color: [0.0, 0.0, 1.0],
            }))
            .push(Box::new(Rectangle {
                width: 50.0,
                height: 150.0,
                color: [0.0, 1.0, 1.0], // Cyan
            }))
            .push(Box::new(row));
        Box::new(clm)
    }
}

fn main() -> anyhow::Result<()> {
    run_desktop_demo::<State>()
}
