use crossbeam::channel::Receiver;
use vexo::{Application, Button, Column, FrameworkState, Rectangle, Row, Text, Widget};
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
            .push(Box::new(Rectangle {
                width: 150.0,
                height: 150.0,
                color: [0.0, 0.1, 0.0],
            }))
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

#[derive(uniffi::Object)]
pub struct MobileApp {}

static mut GLOBAL_FS: Option<FrameworkState<State>> = None;

#[uniffi::export]
impl MobileApp {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {}
    }

    pub fn init_renderer(&self, view_ptr_as_u64: u64, width: u32, height: u32, scale_factor: f32) {
        let view_ptr = view_ptr_as_u64 as *mut std::ffi::c_void;
        let fut = FrameworkState::new_with_ios(view_ptr, width as f32, height as f32, scale_factor);
        let fs = pollster::block_on(fut).unwrap();
        unsafe {
            GLOBAL_FS = Some(fs);
        }
    }

    pub fn render(&self) {
        unsafe {
            // https://doc.rust-lang.org/edition-guide/rust-2024/static-mut-references.html
            let rp = &mut *&raw mut GLOBAL_FS;
            if let Some(val) = rp {
                let _ = val.render();
            } else {
                print!("Global fs not init");
            }
        }
    }

    pub fn resize(&self, width: f32, height: f32) {
        unsafe {
            // https://doc.rust-lang.org/edition-guide/rust-2024/static-mut-references.html
            let rp = &mut *&raw mut GLOBAL_FS;
            if let Some(val) = rp {
                let _ = val.resize_by_logical_point(width, height);
            } else {
                print!("Global fs not init");
            }
        }
    }

    pub fn on_tap(&self, x: f32, y: f32) {
        unsafe {
            // https://doc.rust-lang.org/edition-guide/rust-2024/static-mut-references.html
            let rp = &mut *&raw mut GLOBAL_FS;
            if let Some(val) = rp {
                let _ = val.handle_tap(x, y);
            } else {
                print!("Global fs not init");
            }
        }
    }
}
