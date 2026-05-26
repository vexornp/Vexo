//! Render commands for the Vexo UI framework.
//!
//! This module provides immutable render commands that widgets produce during
//! the paint phase. These commands are then consumed by the rendering backend.
//!
//! # Design Goals
//!
//! - Decouple widgets from the renderer
//! - Enable testing of render output
//! - Support different rendering strategies (batching, culling, etc.)
//! - Allow for render command recording and replay

use crate::core::{Bounds, Color, Point, Stroke};
use crate::core::Logical;

// ============================================================================
// RENDER COMMAND
// ============================================================================

/// Immutable render command produced by widgets during painting.
///
/// These commands describe "what to render" without specifying "how to render".
/// The rendering backend interprets these commands and produces GPU output.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderCommand {
    /// Draw a filled rectangle with optional stroke and corner radius.
    Rect {
        /// The rectangle bounds in logical coordinates.
        bounds: Bounds<Logical>,
        /// The fill color.
        fill: Color,
        /// Optional stroke (border).
        stroke: Option<Stroke>,
        /// Corner radius for rounded rectangles (0.0 = sharp corners).
        corner_radius: f32,
    },

    /// Draw text at a position.
    Text {
        /// The text content to render.
        content: String,
        /// Position in logical coordinates.
        position: Point<Logical>,
        /// Font size in logical points.
        font_size: f32,
        /// Text color.
        color: Color,
        /// Maximum width for text wrapping (optional).
        max_width: Option<f32>,
    },

    /// Draw a text cursor (caret) at a position.
    Caret {
        /// Top-left position of the cursor bar in logical coordinates.
        position: Point<Logical>,
        /// Height of the cursor bar (line height).
        height: f32,
        /// Cursor color.
        color: Color,
    },

    
    /// Push a clipping region onto the stack.
    /// All subsequent commands are clipped to this region.
    PushClip {
        /// The clipping bounds in logical coordinates.
        bounds: Bounds<Logical>,
    },

    /// Pop the most recent clipping region from the stack.
    PopClip,

    /// Push a transform offset onto the stack.
    /// All subsequent commands are offset by this amount.
    PushOffset {
        /// The offset in logical coordinates.
        offset: Point<Logical>,
    },

    /// Pop the most recent transform offset from the stack.
    PopOffset,

    /// Push a corner radius context onto the stack.
    /// Used by modifiers to apply corner radius to child widgets.
    PushCornerRadius {
        /// The corner radius value.
        radius: f32,
    },

    /// Pop the most recent corner radius context from the stack.
    PopCornerRadius,
}

// ============================================================================
// RENDER COMMAND UTILITIES
// ============================================================================

impl RenderCommand {
    /// Create a simple filled rectangle.
    pub fn rect(bounds: Bounds<Logical>, fill: Color) -> Self {
        Self::Rect {
            bounds,
            fill,
            stroke: None,
            corner_radius: 0.0,
        }
    }

    /// Create a rectangle with a border.
    pub fn rect_with_border(
        bounds: Bounds<Logical>,
        fill: Color,
        border_color: Color,
        border_width: f32,
    ) -> Self {
        Self::Rect {
            bounds,
            fill,
            stroke: Some(Stroke::new(border_color, border_width)),
            corner_radius: 0.0,
        }
    }

    /// Create a rounded rectangle.
    pub fn rounded_rect(bounds: Bounds<Logical>, fill: Color, corner_radius: f32) -> Self {
        Self::Rect {
            bounds,
            fill,
            stroke: None,
            corner_radius,
        }
    }

    /// Create a text command.
    pub fn text(content: impl Into<String>, position: Point<Logical>, font_size: f32, color: Color) -> Self {
        Self::Text {
            content: content.into(),
            position,
            font_size,
            color,
            max_width: None,
        }
    }

    /// Create a caret (cursor) command.
    pub fn caret(position: Point<Logical>, height: f32, color: Color) -> Self {
        Self::Caret {
            position,
            height,
            color,
        }
    }
}

// ============================================================================
// RENDER COMMAND LIST
// ============================================================================

/// A list of render commands that can be collected during painting.
#[derive(Debug, Clone, Default)]
pub struct RenderCommandList {
    commands: Vec<RenderCommand>,
}

impl RenderCommandList {
    /// Create an empty command list.
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    /// Create a command list with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            commands: Vec::with_capacity(capacity),
        }
    }

    /// Add a command to the list.
    pub fn push(&mut self, command: RenderCommand) {
        self.commands.push(command);
    }

    /// Add multiple commands to the list.
    pub fn extend(&mut self, commands: impl IntoIterator<Item = RenderCommand>) {
        self.commands.extend(commands);
    }

    /// Get the number of commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Get an iterator over the commands.
    pub fn iter(&self) -> impl Iterator<Item = &RenderCommand> {
        self.commands.iter()
    }

    /// Convert to a vector of commands.
    pub fn into_vec(self) -> Vec<RenderCommand> {
        self.commands
    }

    /// Clear the command list.
    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

impl std::ops::Deref for RenderCommandList {
    type Target = [RenderCommand];

    fn deref(&self) -> &Self::Target {
        &self.commands
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_command() {
        let bounds = Bounds::from_xywh(10.0, 20.0, 100.0, 50.0);
        let cmd = RenderCommand::rect(bounds, Color::RED);

        match cmd {
            RenderCommand::Rect {
                bounds: b,
                fill,
                stroke,
                corner_radius,
            } => {
                assert_eq!(b.left, 10.0);
                assert_eq!(b.width(), 100.0);
                assert_eq!(fill, Color::RED);
                assert!(stroke.is_none());
                assert_eq!(corner_radius, 0.0);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_rect_with_border() {
        let bounds = Bounds::from_xywh(0.0, 0.0, 50.0, 50.0);
        let cmd = RenderCommand::rect_with_border(bounds, Color::WHITE, Color::BLACK, 2.0);

        match cmd {
            RenderCommand::Rect { stroke: Some(s), .. } => {
                assert_eq!(s.color, Color::BLACK);
                assert_eq!(s.width, 2.0);
            }
            _ => panic!("Expected Rect with stroke"),
        }
    }

    #[test]
    fn test_text_command() {
        let pos = Point::new(10.0, 20.0);
        let cmd = RenderCommand::text("Hello", pos, 16.0, Color::BLACK);

        match cmd {
            RenderCommand::Text {
                content,
                position,
                font_size,
                color,
                max_width,
            } => {
                assert_eq!(content, "Hello");
                assert_eq!(position.x, 10.0);
                assert_eq!(font_size, 16.0);
                assert_eq!(color, Color::BLACK);
                assert!(max_width.is_none());
            }
            _ => panic!("Expected Text command"),
        }
    }

    #[test]
    fn test_command_list() {
        let mut list = RenderCommandList::new();
        assert!(list.is_empty());

        list.push(RenderCommand::rect(
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Color::RED,
        ));
        list.push(RenderCommand::text("Test", Point::new(0.0, 0.0), 12.0, Color::BLACK));

        assert_eq!(list.len(), 2);
        assert!(!list.is_empty());

        list.clear();
        assert!(list.is_empty());
    }

    #[test]
    fn test_stroke_default() {
        let s = Stroke::default();
        assert_eq!(s.color, Color::BLACK);
        assert_eq!(s.width, 1.0);
    }
}
