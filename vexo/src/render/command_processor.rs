//! Process RenderCommands into UiBatcher calls.
//!
//! This module provides the bridge between the new `RenderCommand` output from
//! `Paint::paint()` and the existing `UiBatcher` renderer.

use crate::core::{Color, Logical, Point, Rect};
use crate::render::RenderCommand;
use crate::renderer::UiBatcher;

/// Process a list of render commands into UiBatcher calls.
///
/// This function iterates through the commands and translates each one
/// into the appropriate UiBatcher method call. It handles offset stacking
/// internally for `PushOffset`/`PopOffset` commands.
///
/// # Arguments
///
/// * `commands` - The render commands to process
/// * `batcher` - The batcher to submit commands to
/// * `initial_offset` - An initial offset to apply to all coordinates
pub fn process_commands(
    commands: &[RenderCommand],
    batcher: &mut UiBatcher,
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
                let pos = [
                    bounds.origin.x + current_offset.x,
                    bounds.origin.y + current_offset.y,
                ];
                let size = [bounds.size.width, bounds.size.height];
                let border_color = stroke
                    .as_ref()
                    .map(|s| s.color)
                    .unwrap_or(Color::TRANSPARENT);
                let border_width = stroke.as_ref().map(|s| s.width).unwrap_or(0.0);
                batcher.add_rect(pos, size, *fill, border_color, border_width, *corner_radius);
            }
            RenderCommand::Text {
                content,
                position,
                font_size,
                color,
                ..
            } => {
                let pos = Point::new(
                    position.x + current_offset.x,
                    position.y + current_offset.y,
                );
                batcher.add_text(content.clone(), pos, *font_size, *color);
            }
            RenderCommand::Editor { id, bounds, .. } => {
                let rect = Rect::new(
                    Point::new(
                        bounds.origin.x + current_offset.x,
                        bounds.origin.y + current_offset.y,
                    ),
                    bounds.size,
                );
                batcher.add_editor_request(id, rect);
            }
            RenderCommand::PushClip { bounds } => {
                let adjusted_bounds = Rect::new(
                    Point::new(
                        bounds.origin.x + current_offset.x,
                        bounds.origin.y + current_offset.y,
                    ),
                    bounds.size,
                );
                batcher.push_clip(adjusted_bounds);
            }
            RenderCommand::PopClip => {
                batcher.pop_clip();
            }
            RenderCommand::PushCornerRadius { radius } => {
                batcher.push_corner_radius(*radius);
            }
            RenderCommand::PopCornerRadius => {
                batcher.pop_corner_radius();
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

    #[test]
    fn test_process_rect_command() {
        let mut batcher = UiBatcher::new();
        let commands = vec![RenderCommand::rect(
            Rect::from_xywh(10.0, 20.0, 100.0, 50.0),
            Color::RED,
        )];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert_eq!(batcher.quad_instances.len(), 1);
        let quad = &batcher.quad_instances[0];
        assert_eq!(quad.position, [10.0, 20.0]);
        assert_eq!(quad.size, [100.0, 50.0]);
        assert_eq!(quad.color, Color::RED.to_array());
    }

    #[test]
    fn test_process_rect_with_offset() {
        let mut batcher = UiBatcher::new();
        let commands = vec![RenderCommand::rect(
            Rect::from_xywh(0.0, 0.0, 100.0, 50.0),
            Color::RED,
        )];

        process_commands(&commands, &mut batcher, Point::new(50.0, 25.0));

        assert_eq!(batcher.quad_instances.len(), 1);
        let quad = &batcher.quad_instances[0];
        assert_eq!(quad.position, [50.0, 25.0]);
    }

    #[test]
    fn test_process_rect_with_border() {
        let mut batcher = UiBatcher::new();
        let commands = vec![RenderCommand::rect_with_border(
            Rect::from_xywh(0.0, 0.0, 100.0, 50.0),
            Color::WHITE,
            Color::BLACK,
            2.0,
        )];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert_eq!(batcher.quad_instances.len(), 1);
        let quad = &batcher.quad_instances[0];
        assert_eq!(quad.border_color, Color::BLACK.to_array());
        assert_eq!(quad.border_width, 2.0);
    }

    #[test]
    fn test_process_text_command() {
        let mut batcher = UiBatcher::new();
        let commands = vec![RenderCommand::text(
            "Hello",
            Point::new(10.0, 20.0),
            16.0,
            Color::BLACK,
        )];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert_eq!(batcher.text_requests.len(), 1);
        let text = &batcher.text_requests[0];
        assert_eq!(text.content, "Hello");
        assert_eq!(text.position.x, 10.0);
        assert_eq!(text.position.y, 20.0);
        assert_eq!(text.size, 16.0);
    }

    #[test]
    fn test_process_text_with_offset() {
        let mut batcher = UiBatcher::new();
        let commands = vec![RenderCommand::text(
            "Hello",
            Point::new(10.0, 20.0),
            16.0,
            Color::BLACK,
        )];

        process_commands(&commands, &mut batcher, Point::new(5.0, 10.0));

        assert_eq!(batcher.text_requests.len(), 1);
        let text = &batcher.text_requests[0];
        assert_eq!(text.position.x, 15.0);
        assert_eq!(text.position.y, 30.0);
    }

    #[test]
    fn test_process_editor_command() {
        let mut batcher = UiBatcher::new();
        let commands = vec![RenderCommand::editor(
            "editor-1",
            Rect::from_xywh(10.0, 20.0, 200.0, 30.0),
        )];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert_eq!(batcher.editor_requests.len(), 1);
        let editor = &batcher.editor_requests[0];
        assert_eq!(editor.id, "editor-1");
        assert_eq!(editor.bounds.origin.x, 10.0);
        assert_eq!(editor.bounds.origin.y, 20.0);
    }

    #[test]
    fn test_process_editor_with_offset() {
        let mut batcher = UiBatcher::new();
        let commands = vec![RenderCommand::editor(
            "editor-1",
            Rect::from_xywh(0.0, 0.0, 200.0, 30.0),
        )];

        process_commands(&commands, &mut batcher, Point::new(100.0, 50.0));

        let editor = &batcher.editor_requests[0];
        assert_eq!(editor.bounds.origin.x, 100.0);
        assert_eq!(editor.bounds.origin.y, 50.0);
    }

    #[test]
    fn test_process_corner_radius_commands() {
        let mut batcher = UiBatcher::new();
        let commands = vec![
            RenderCommand::PushCornerRadius { radius: 10.0 },
            RenderCommand::rect(Rect::from_xywh(0.0, 0.0, 100.0, 100.0), Color::RED),
            RenderCommand::PopCornerRadius,
        ];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert_eq!(batcher.quad_instances.len(), 1);
        // Corner radius should be applied from context since command radius is 0.0
        assert_eq!(batcher.quad_instances[0].corner_radius, 10.0);
    }

    #[test]
    fn test_process_clip_commands() {
        let mut batcher = UiBatcher::new();
        let clip_bounds = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let commands = vec![
            RenderCommand::PushClip { bounds: clip_bounds },
            RenderCommand::rect(Rect::from_xywh(10.0, 10.0, 50.0, 50.0), Color::RED),
            RenderCommand::PopClip,
        ];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        // Clip should be popped, so current_clip should be None
        assert!(batcher.current_clip().is_none());
    }

    #[test]
    fn test_process_clip_with_offset() {
        let mut batcher = UiBatcher::new();
        let clip_bounds = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let commands = vec![RenderCommand::PushClip { bounds: clip_bounds }];

        process_commands(&commands, &mut batcher, Point::new(50.0, 25.0));

        // Clip bounds should be adjusted by offset
        let current_clip = batcher.current_clip();
        assert!(current_clip.is_some());
        let clip = current_clip.unwrap();
        assert_eq!(clip.origin.x, 50.0);
        assert_eq!(clip.origin.y, 25.0);
    }

    #[test]
    fn test_process_offset_commands() {
        let mut batcher = UiBatcher::new();
        let commands = vec![
            RenderCommand::PushOffset {
                offset: Point::new(100.0, 50.0),
            },
            RenderCommand::rect(Rect::from_xywh(0.0, 0.0, 50.0, 50.0), Color::RED),
            RenderCommand::PopOffset,
            RenderCommand::rect(Rect::from_xywh(0.0, 0.0, 25.0, 25.0), Color::BLUE),
        ];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert_eq!(batcher.quad_instances.len(), 2);
        // First rect should be at (100.0, 50.0) due to push offset
        assert_eq!(batcher.quad_instances[0].position, [100.0, 50.0]);
        // Second rect should be at (0.0, 0.0) after pop offset
        assert_eq!(batcher.quad_instances[1].position, [0.0, 0.0]);
    }

    #[test]
    fn test_process_nested_offsets() {
        let mut batcher = UiBatcher::new();
        let commands = vec![
            RenderCommand::PushOffset {
                offset: Point::new(10.0, 10.0),
            },
            RenderCommand::rect(Rect::from_xywh(0.0, 0.0, 50.0, 50.0), Color::RED),
            RenderCommand::PushOffset {
                offset: Point::new(20.0, 20.0),
            },
            RenderCommand::rect(Rect::from_xywh(0.0, 0.0, 50.0, 50.0), Color::GREEN),
            RenderCommand::PopOffset,
            RenderCommand::rect(Rect::from_xywh(0.0, 0.0, 50.0, 50.0), Color::BLUE),
            RenderCommand::PopOffset,
        ];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert_eq!(batcher.quad_instances.len(), 3);
        // First rect: offset (10.0, 10.0)
        assert_eq!(batcher.quad_instances[0].position, [10.0, 10.0]);
        // Second rect: offset (10.0 + 20.0, 10.0 + 20.0) = (30.0, 30.0)
        assert_eq!(batcher.quad_instances[1].position, [30.0, 30.0]);
        // Third rect: back to offset (10.0, 10.0)
        assert_eq!(batcher.quad_instances[2].position, [10.0, 10.0]);
    }

    #[test]
    fn test_process_multiple_commands() {
        let mut batcher = UiBatcher::new();
        let commands = vec![
            RenderCommand::rect(Rect::from_xywh(0.0, 0.0, 100.0, 50.0), Color::RED),
            RenderCommand::text("Hello", Point::new(10.0, 10.0), 16.0, Color::BLACK),
            RenderCommand::rect(Rect::from_xywh(0.0, 60.0, 100.0, 50.0), Color::BLUE),
        ];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert_eq!(batcher.quad_instances.len(), 2);
        assert_eq!(batcher.text_requests.len(), 1);
    }

    #[test]
    fn test_process_empty_commands() {
        let mut batcher = UiBatcher::new();
        let commands: Vec<RenderCommand> = vec![];

        process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

        assert!(batcher.quad_instances.is_empty());
        assert!(batcher.text_requests.is_empty());
        assert!(batcher.editor_requests.is_empty());
    }
}
