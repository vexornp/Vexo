//! Integration test: EdgePanDetector fires on_start/on_update/on_end when an
//! edge-pan gesture wins the arena, and registers no recognizer when disabled.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use vexo::animation::AnimationTicker;
use vexo::core::{Point, ScaleSource, Size};
use vexo::input::{ButtonState, InputEvent, PointerButton};
use vexo::layout::TaffyLayoutEngine;
use vexo::ThreeTreePipeline;
use vexo::{Color, DecoratedBox, Style, Text};
use vexo::{EdgePanDetector, Widget};

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = vexo::resource::file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

#[test]
fn edge_pan_detector_fires_start_update_end_when_enabled() {
    let started = Rc::new(Cell::new(false));
    let last_delta = Rc::new(Cell::new(0.0_f32));
    let ended = Rc::new(Cell::new(false));
    let end_delta = Rc::new(Cell::new(0.0_f32));
    let s = started.clone();
    let u = last_delta.clone();
    let e = ended.clone();
    let ed = end_delta.clone();

    let widget: Box<dyn Widget> = Box::new(
        EdgePanDetector::new(
            DecoratedBox::with_style(
                Text::new("Swipe me"),
                Style::default().background(Color::WHITE),
            ),
            true,
        )
        .on_start(move || s.set(true))
        .on_update(move |dx| u.set(dx))
        .on_end(move |dx| {
            e.set(true);
            ed.set(dx);
        }),
    );

    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(widget);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    let clipboard: Arc<dyn vexo::platform::Clipboard> =
        Arc::new(vexo::platform::stub_clipboard::StubClipboard);

    // Press within the 20pt edge zone.
    let press = InputEvent::PointerButton {
        position: Point::new(10.0, 100.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        Point::new(10.0, 100.0),
        &press,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );

    // Move rightward past slop — triggers Down+Move on the winning recognizer.
    let mv = InputEvent::PointerMoved {
        position: Point::new(80.0, 102.0),
    };
    pipeline.handle_event(
        Point::new(80.0, 102.0),
        &mv,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );
    pipeline.perform_rebuilds();

    assert!(started.get(), "on_start must fire when recognizer wins");
    assert!(
        last_delta.get() > 0.0,
        "on_update must fire with positive delta_x, got {}",
        last_delta.get()
    );

    // Release — triggers on_end.
    let release = InputEvent::PointerButton {
        position: Point::new(80.0, 102.0),
        button: PointerButton::Primary,
        state: ButtonState::Released,
    };
    pipeline.handle_event(
        Point::new(80.0, 102.0),
        &release,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );
    pipeline.perform_rebuilds();

    assert!(ended.get(), "on_end must fire on release");
    assert!(end_delta.get() > 0.0);
}

#[test]
fn edge_pan_detector_disabled_does_not_fire() {
    let started = Rc::new(Cell::new(false));
    let s = started.clone();

    let widget: Box<dyn Widget> = Box::new(
        EdgePanDetector::new(
            DecoratedBox::with_style(
                Text::new("No swipe"),
                Style::default().background(Color::WHITE),
            ),
            false,
        )
        .on_start(move || s.set(true)),
    );

    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(widget);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    let clipboard: Arc<dyn vexo::platform::Clipboard> =
        Arc::new(vexo::platform::stub_clipboard::StubClipboard);

    let press = InputEvent::PointerButton {
        position: Point::new(10.0, 100.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        Point::new(10.0, 100.0),
        &press,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );
    let mv = InputEvent::PointerMoved {
        position: Point::new(80.0, 100.0),
    };
    pipeline.handle_event(
        Point::new(80.0, 100.0),
        &mv,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );
    pipeline.perform_rebuilds();

    assert!(!started.get(), "on_start must NOT fire when disabled");
}
