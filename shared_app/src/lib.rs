use std::os::macos::raw::stat;

use vexo::{Application, Button, Column, FrameworkState, Rectangle, Row, Text, TextEdit, Widget};
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
        let text_edit = Box::new(vexo::TextEdit::new(24.0).size((100.0, 50.0)));

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
