//! Process RenderCommands into FrameBuilder calls.
//!
//! This module provides the bridge between the new `RenderCommand` output from
//! `Paint::paint()` and the existing `FrameBuilder` renderer.

use crate::core::{Bounds, Logical, Point};
use crate::render::RenderCommand;
use crate::frame_builder::FrameBuilder;

/// Process a list of render commands into FrameBuilder calls.
///
/// This function iterates through the commands and translates each one
/// into the appropriate FrameBuilder method call. It handles offset stacking
/// internally for `PushOffset`/`PopOffset` commands.
///
/// # Arguments
///
/// * `commands` - The render commands to process
/// * `frame_builder` - The frame_builder to submit commands to
/// * `initial_offset` - An initial offset to apply to all coordinates
pub fn process_commands(
    commands: &[RenderCommand],
    frame_builder: &mut FrameBuilder,
    initial_offset: Point<Logical>,
) {
    // Stack to track nested offsets
    let mut offset_stack: Vec<Point<Logical>> = Vec::new();
    let mut current_offset = initial_offset;

    for cmd in commands {
        match cmd {
            RenderCommand::Rect {
                bounds,
                fill,
                stroke,
                corner_radius,
            } => {
                let adjusted_bounds = Bounds::new(
                    bounds.left + current_offset.x,
                    bounds.top + current_offset.y,
                    bounds.right + current_offset.x,
                    bounds.bottom + current_offset.y,
                );
                frame_builder.add_rect(adjusted_bounds, *fill, *stroke, *corner_radius);
            }
            RenderCommand::Text {
                content,
                position,
                font_size,
                color,
                max_width,
            } => {
                let pos = Point::new(
                    position.x + current_offset.x,
                    position.y + current_offset.y,
                );
                frame_builder.add_text(content, pos, *font_size, *color, *max_width);
            }
            RenderCommand::Caret {
                position,
                height,
                color,
            } => {
                let pos: Point<Logical> = Point::new(
                    position.x + current_offset.x,
                    position.y + current_offset.y,
                );
                let bounds = Bounds::from_xywh(pos.x, pos.y, 2.0, *height);
                frame_builder.add_rect(bounds, *color, None, 0.0);
            }
            RenderCommand::PushClip { bounds } => {
                let adjusted_bounds = Bounds::new(
                    bounds.left + current_offset.x,
                    bounds.top + current_offset.y,
                    bounds.right + current_offset.x,
                    bounds.bottom + current_offset.y,
                );
                frame_builder.push_clip(adjusted_bounds);
            }
            RenderCommand::PopClip => {
                frame_builder.pop_clip();
            }
            RenderCommand::PushCornerRadius { radius } => {
                frame_builder.push_corner_radius(*radius);
            }
            RenderCommand::PopCornerRadius => {
                frame_builder.pop_corner_radius();
            }
            RenderCommand::PushOffset { offset: off } => {
                // Save current offset before modifying
                offset_stack.push(current_offset);
                current_offset = Point::new(
                    current_offset.x + off.x,
                    current_offset.y + off.y,
                );
            }
            RenderCommand::PopOffset => {
                // Restore previous offset
                if let Some(prev_offset) = offset_stack.pop() {
                    current_offset = prev_offset;
                }
            }
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Color;

    #[test]
    fn test_process_rect_command() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![RenderCommand::rect(
            Bounds::from_xywh(10.0, 20.0, 100.0, 50.0),
            Color::RED,
        )];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        assert_eq!(frame_builder.quad_count(), 1);
        let quad = &frame_builder.quad_instances()[0];
        assert_eq!(quad.position, [10.0, 20.0]);
        assert_eq!(quad.size, [100.0, 50.0]);
        assert_eq!(quad.color, Color::RED.to_array());
    }

    #[test]
    fn test_process_rect_with_offset() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![RenderCommand::rect(
            Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
            Color::RED,
        )];

        process_commands(&commands, &mut frame_builder, Point::new(50.0, 25.0));

        assert_eq!(frame_builder.quad_count(), 1);
        let quad = &frame_builder.quad_instances()[0];
        assert_eq!(quad.position, [50.0, 25.0]);
    }

    #[test]
    fn test_process_rect_with_border() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![RenderCommand::rect_with_border(
            Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
            Color::WHITE,
            Color::BLACK,
            2.0,
        )];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        assert_eq!(frame_builder.quad_count(), 1);
        let quad = &frame_builder.quad_instances()[0];
        assert_eq!(quad.border_color, Color::BLACK.to_array());
        assert_eq!(quad.border_width, 2.0);
    }

    #[test]
    fn test_process_text_command() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![RenderCommand::text(
            "Hello",
            Point::new(10.0, 20.0),
            16.0,
            Color::BLACK,
        )];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        assert_eq!(frame_builder.text_count(), 1);
        let text = &frame_builder.text_requests()[0];
        assert_eq!(text.content, "Hello");
        assert_eq!(text.position.x, 10.0);
        assert_eq!(text.position.y, 20.0);
        assert_eq!(text.size, 16.0);
    }

    #[test]
    fn test_process_text_with_offset() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![RenderCommand::text(
            "Hello",
            Point::new(10.0, 20.0),
            16.0,
            Color::BLACK,
        )];

        process_commands(&commands, &mut frame_builder, Point::new(5.0, 10.0));

        assert_eq!(frame_builder.text_count(), 1);
        let text = &frame_builder.text_requests()[0];
        assert_eq!(text.position.x, 15.0);
        assert_eq!(text.position.y, 30.0);
    }

    #[test]
    fn test_process_corner_radius_commands() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![
            RenderCommand::PushCornerRadius { radius: 10.0 },
            RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 100.0, 100.0), Color::RED),
            RenderCommand::PopCornerRadius,
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        assert_eq!(frame_builder.quad_count(), 1);
        assert_eq!(frame_builder.quad_instances()[0].corner_radius, 10.0);
    }

    #[test]
    fn test_process_clip_commands() {
        let mut frame_builder = FrameBuilder::new();
        let clip_bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 100.0);
        let commands = vec![
            RenderCommand::PushClip { bounds: clip_bounds },
            RenderCommand::rect(Bounds::from_xywh(10.0, 10.0, 50.0, 50.0), Color::RED),
            RenderCommand::PopClip,
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        // Clip should be popped, so current_clip should be None
        assert!(frame_builder.current_clip().is_none());
    }

    #[test]
    fn test_process_clip_with_offset() {
        let mut frame_builder = FrameBuilder::new();
        let clip_bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 100.0);
        let commands = vec![RenderCommand::PushClip { bounds: clip_bounds }];

        process_commands(&commands, &mut frame_builder, Point::new(50.0, 25.0));

        // Clip bounds should be adjusted by offset
        let current_clip = frame_builder.current_clip();
        assert!(current_clip.is_some());
        let clip = current_clip.unwrap();
        assert_eq!(clip.left, 50.0);
        assert_eq!(clip.top, 25.0);
    }

    #[test]
    fn test_process_offset_commands() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![
            RenderCommand::PushOffset {
                offset: Point::new(100.0, 50.0),
            },
            RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 50.0, 50.0), Color::RED),
            RenderCommand::PopOffset,
            RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 25.0, 25.0), Color::BLUE),
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        assert_eq!(frame_builder.quad_count(), 2);
        assert_eq!(frame_builder.quad_instances()[0].position, [100.0, 50.0]);
        assert_eq!(frame_builder.quad_instances()[1].position, [0.0, 0.0]);
    }

    #[test]
    fn test_process_nested_offsets() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![
            RenderCommand::PushOffset {
                offset: Point::new(10.0, 10.0),
            },
            RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 50.0, 50.0), Color::RED),
            RenderCommand::PushOffset {
                offset: Point::new(20.0, 20.0),
            },
            RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 50.0, 50.0), Color::GREEN),
            RenderCommand::PopOffset,
            RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 50.0, 50.0), Color::BLUE),
            RenderCommand::PopOffset,
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        assert_eq!(frame_builder.quad_count(), 3);
        assert_eq!(frame_builder.quad_instances()[0].position, [10.0, 10.0]);
        assert_eq!(frame_builder.quad_instances()[1].position, [30.0, 30.0]);
        assert_eq!(frame_builder.quad_instances()[2].position, [10.0, 10.0]);
    }

    #[test]
    fn test_process_multiple_commands() {
        let mut frame_builder = FrameBuilder::new();
        let commands = vec![
            RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0), Color::RED),
            RenderCommand::text("Hello", Point::new(10.0, 10.0), 16.0, Color::BLACK),
            RenderCommand::rect(Bounds::from_xywh(0.0, 60.0, 100.0, 50.0), Color::BLUE),
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        assert_eq!(frame_builder.quad_count(), 2);
        assert_eq!(frame_builder.text_count(), 1);
    }

    #[test]
    fn test_process_empty_commands() {
        let mut frame_builder = FrameBuilder::new();
        let commands: Vec<RenderCommand> = vec![];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        assert!(frame_builder.quad_instances().is_empty());
        assert!(frame_builder.text_requests().is_empty());
    }

    #[test]
    fn test_process_caret_command() {
        let mut frame_builder = FrameBuilder::new();
        let cursor_color = Color::rgb(0.3, 0.67, 0.97);
        let commands = vec![RenderCommand::caret(
            Point::new(50.0, 10.0),
            20.0,
            cursor_color,
        )];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        // Caret should be rendered as a 2px-wide rect
        assert_eq!(frame_builder.quad_count(), 1);
        let quad = &frame_builder.quad_instances()[0];
        assert_eq!(quad.position, [50.0, 10.0]);
        assert_eq!(quad.size, [2.0, 20.0]);
        assert_eq!(quad.color, cursor_color.to_array());
    }

    #[test]
    fn test_process_caret_with_offset() {
        let mut frame_builder = FrameBuilder::new();
        let cursor_color = Color::rgb(0.3, 0.67, 0.97);
        let commands = vec![RenderCommand::caret(
            Point::new(10.0, 5.0),
            20.0,
            cursor_color,
        )];

        process_commands(&commands, &mut frame_builder, Point::new(100.0, 50.0));

        let quad = &frame_builder.quad_instances()[0];
        assert_eq!(quad.position, [110.0, 55.0]);
    }
}