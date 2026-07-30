//! Integration test: Text widget wires max_lines through to TextRenderObject.

use crate::render_objects::TextRenderObject;
use crate::widgets::{Text, Widget};

#[test]
fn test_text_widget_passes_max_lines_to_render_object() {
    let widget = Text::new("Hello World this is a long text")
        .with_max_lines(2)
        .with_font_size(16.0);

    let ro = widget.create_render_object();
    let any = ro.as_any();
    let text_ro = any
        .downcast_ref::<TextRenderObject>()
        .expect("should be TextRenderObject");
    assert_eq!(text_ro.max_lines(), Some(2));
}

#[test]
fn test_text_widget_without_max_lines_passes_none() {
    let widget = Text::new("Hello World");
    let ro = widget.create_render_object();
    let any = ro.as_any();
    let text_ro = any
        .downcast_ref::<TextRenderObject>()
        .expect("should be TextRenderObject");
    assert!(text_ro.max_lines().is_none());
}
