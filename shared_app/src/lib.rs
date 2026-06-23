use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use vexo::reactive::Signal;
use vexo::{
    run_desktop_demo, AnimationController, Application, Column, ComponentState, RenderContext,
    Color, ColorTween, Focus, Image, ImageData, LifecycleContext, StatefulWidget, ScrollView,
    Text, Tween, Widget,
};
uniffi::setup_scaffolding!();

/// Creates a 200x150 gradient JPEG as ImageData for demo purposes.
fn create_test_image_data() -> ImageData {
    use image::{ImageFormat, RgbImage};

    let width = 200u32;
    let height = 150u32;
    let mut img = RgbImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let r = (x as f32 / width as f32 * 255.0) as u8;
            let g = (y as f32 / height as f32 * 255.0) as u8;
            let b = 128;
            img.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }

    let mut jpeg_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut jpeg_bytes), ImageFormat::Jpeg)
        .expect("Failed to encode JPEG");

    ImageData::from_bytes(&jpeg_bytes).expect("Failed to create ImageData from JPEG bytes")
}

// --- FocusableScrollList: A StatefulWidget that changes border on focus ---

#[derive(Clone)]
struct FocusableScrollList;

#[derive(ComponentState)]
struct FocusableScrollListState {
    is_focused: Signal<bool>,
}

impl Default for FocusableScrollListState {
    fn default() -> Self {
        Self {
            is_focused: Signal::new(false),
        }
    }
}

impl StatefulWidget for FocusableScrollList {
    type State = FocusableScrollListState;

    fn build(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_focused = state.is_focused.get();
        let border_color = if is_focused {
            Color::rgb(0.2, 0.4, 0.8)
        } else {
            Color::rgb(0.6, 0.6, 0.6)
        };
        let border_width = if is_focused { 2.0 } else { 1.0 };

        let is_focused_clone = state.is_focused.clone();
        let content = build_scroll_content();

        Focus::new(
            ScrollView::new(content)
                .width(200.0)
                .height(300.0)
                .border(border_color, border_width),
        )
        .on_focus_change(move |focused| {
            is_focused_clone.set(focused);
        })
        .boxed()
    }
}

fn build_scroll_content() -> Box<dyn Widget> {
    let mut column = Column::new().gap(0.0);
    for i in 0..20 {
        let label = format!("Item {}", i + 1);
        column = column.push(Text::new(&label).padding(16.0).background(if i % 2 == 0 {
            Color::rgb(0.95, 0.95, 0.95)
        } else {
            Color::WHITE
        }));
    }
    column.boxed()
}

// --- AnimatedButton: A StatefulWidget whose background color animates on press ---

#[derive(Clone)]
struct AnimatedButton;

struct AnimatedButtonState {
    anim: Rc<RefCell<AnimationController>>,
    color_tween: ColorTween,
}

impl Default for AnimatedButtonState {
    fn default() -> Self {
        Self {
            anim: Rc::new(RefCell::new(AnimationController::new(Duration::from_millis(300)))),
            color_tween: ColorTween::new(Color::rgb(0.2, 0.4, 0.8), Color::rgb(0.8, 0.2, 0.2)),
        }
    }
}

impl vexo::State for AnimatedButtonState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        self.anim
            .borrow_mut()
            .set_ticker(ctx.animation_ticker().clone());
    }

    fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.anim.borrow_mut().set_dirty_callback(callback);
    }

    fn on_tick(&mut self, now: std::time::Instant) {
        self.anim.borrow_mut().advance(now);
    }
}

impl StatefulWidget for AnimatedButton {
    type State = AnimatedButtonState;

    fn build(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let t = state.anim.borrow().value();
        let bg = state.color_tween.lerp(t);

        let anim = state.anim.clone();
        Column::new()
            .background(bg)
            .corner_radius(8.0)
            .padding(8.0)
            .push(Text::new("Tap to animate"))
            .on_press(move || {
                let mut ctrl = anim.borrow_mut();
                if ctrl.value() < 0.5 {
                    ctrl.forward();
                } else {
                    ctrl.reverse();
                }
            })
    }
}

// --- The User's Code ---
pub struct State;

impl Application for State {
    type State = Self;

    fn new() -> Self::State {
        Self
    }

    fn view(_state: &mut Self::State, _font_system: &mut glyphon::FontSystem) -> Box<dyn Widget> {
        let test_image = create_test_image_data();

        Column::new()
            .gap(16.0)
            .push(Text::new("Image Demo").padding(8.0))
            .push(Image::new(test_image).width(200.0).border(Color::BLUE, 3.0))
            .push(FocusableScrollList.boxed())
            .push(AnimatedButton.boxed())
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
        let rt = run_desktop_demo::<State>();
        match rt {
            Ok(_) => println!("App exited normally"),
            Err(e) => println!("App exited with error: {:?}", e),
        }
    }
}
