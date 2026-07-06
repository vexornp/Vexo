//! The [`Icon`] widget — a typed FontAwesome icon rendered via vexo's `Text`.
//!
//! [`Icon`] is a thin [`Component`] that delegates to
//! [`vexo::widgets::Text`], setting the font family to FontAwesome and the
//! content to the icon's codepoint. Because it is a `Component`, it integrates
//! with vexo's three-tree architecture (widget → element → render object) the
//! same way any other stateful widget does.

use crate::{Icons, FONT_FAMILY};
use vexo::{Color, Component, SimpleState, Text, Widget};

/// A FontAwesome icon widget.
///
/// Construct with [`Icon::new`], then chain `.with_size()` / `.with_color()`
/// (and any other `Widget` modifier, e.g. `.padding()`, `.on_press()`).
///
/// # Example
///
/// ```ignore
/// use vexo_fontawesome::{Icon, Icons};
///
/// Icon::new(Icons::House)
///     .with_size(24.0)
///     .with_color(vexo::Color::BLACK)
///     .boxed()
/// ```
#[derive(Clone)]
pub struct Icon {
    icon: Icons,
    size: f32,
    color: Color,
}

impl Icon {
    /// Create an icon widget showing `icon` at the default size (24.0) and
    /// color (black).
    pub fn new(icon: Icons) -> Self {
        Self {
            icon,
            size: 24.0,
            color: Color::BLACK,
        }
    }

    /// Set the icon's font size in logical points.
    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Set the icon's color.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Component for Icon {
    // Icons are stateless; reuse the framework's no-op empty state.
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, _ctx: &mut vexo::RenderContext) -> Box<dyn Widget> {
        Text::new(self.icon.codepoint())
            .with_font_family(FONT_FAMILY)
            .with_font_size(self.size)
            .with_color(self.color)
            .boxed()
    }
}
