//! SafeArea widget — insets its child away from the device's unsafe regions.
//!
//! On mobile (iOS) the OS reports per-edge safe-area insets covering the
//! status bar / notch / home indicator. `SafeArea` reads those insets live
//! during render (via [`MediaQuery::of`] → `padding`) and pads its child
//! so content stays within the safe region. On desktop the insets are always
//! zero, so `SafeArea` is a transparent pass-through.
//!
//! This mirrors Flutter's `SafeArea` widget: opt out per side, enforce a
//! `minimum` inset floor, and provide a `MediaQuery` with the consumed
//! edges' `padding` zeroed so descendant `SafeArea`s don't double-consume.
//!
//! # Design notes
//!
//! Insets are resolved at *render* time (Flutter's model), not layout time.
//! [`WindowState`](crate::window::WindowState) writes the live insets into a
//! shared [`SafeAreaSource`](crate::core::SafeAreaSource) each frame; when
//! they change it marks the tree dirty so the root `MediaQuery` re-renders
//! and `SafeArea` (which depends on `MediaQueryData`) rebuilds with the new
//! `padding`.

use crate::key::WidgetKey;
use crate::layout::{AlignItems, EdgeInsets, FlexDirection, Layout};
use crate::stateful_widget::{Component, RenderContext, SimpleState};
use crate::widgets::{MediaQuery, RemoveEdges, Widget, WithLayout};

/// A widget that insets its child by the device's safe-area insets.
///
/// On mobile this keeps content clear of the status bar, notch, and home
/// indicator. On desktop the insets are zero, so this is a transparent
/// pass-through. Per-side opt-out is supported via the builder methods, and a
/// `minimum` floor can be enforced.
///
/// # Example
///
/// ```ignore
/// use vexo::{SafeArea, Text};
///
/// SafeArea::new(Text::new("Hello"))
///
/// SafeArea::new(Text::new("Hello")).bottom(false).left(false).right(false)
/// ```
pub struct SafeArea {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    top: bool,
    right: bool,
    bottom: bool,
    left: bool,
    minimum: EdgeInsets,
}

impl SafeArea {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            top: true,
            right: true,
            bottom: true,
            left: true,
            minimum: EdgeInsets::default(),
        }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn top(mut self, enabled: bool) -> Self {
        self.top = enabled;
        self
    }

    pub fn right(mut self, enabled: bool) -> Self {
        self.right = enabled;
        self
    }

    pub fn bottom(mut self, enabled: bool) -> Self {
        self.bottom = enabled;
        self
    }

    pub fn left(mut self, enabled: bool) -> Self {
        self.left = enabled;
        self
    }

    pub fn minimum(mut self, minimum: EdgeInsets) -> Self {
        self.minimum = minimum;
        self
    }

    fn effective_padding(&self, insets: EdgeInsets) -> (f32, f32, f32, f32) {
        let left = if self.left {
            insets.left.max(self.minimum.left)
        } else {
            0.0
        };
        let right = if self.right {
            insets.right.max(self.minimum.right)
        } else {
            0.0
        };
        let top = if self.top {
            insets.top.max(self.minimum.top)
        } else {
            0.0
        };
        let bottom = if self.bottom {
            insets.bottom.max(self.minimum.bottom)
        } else {
            0.0
        };
        (left, right, top, bottom)
    }
}

impl Clone for SafeArea {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            left: self.left,
            minimum: self.minimum,
        }
    }
}

impl Component for SafeArea {
    type State = SimpleState<()>;

    fn widget_child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let mq = MediaQuery::of(ctx);
        let insets = mq.padding;

        let (left, right, top, bottom) = self.effective_padding(insets);
        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .flex_grow(1.0)
            .min_height(0.0)
            .padding_each(left, right, top, bottom);

        let inner = MediaQuery::remove_padding(
            WithLayout::new(self.child.clone_boxed(), layout),
            RemoveEdges {
                top: self.top,
                right: self.right,
                bottom: self.bottom,
                left: self.left,
            },
        );
        inner.boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn defaults_all_sides_enabled() {
        let w = SafeArea::new(Text::new("Hi"));
        assert!(w.top && w.right && w.bottom && w.left);
        assert_eq!(w.minimum, EdgeInsets::default());
    }

    #[test]
    fn per_side_opt_out() {
        let w = SafeArea::new(Text::new("Hi"))
            .top(false)
            .bottom(false)
            .left(false)
            .right(false);
        assert!(!w.top && !w.right && !w.bottom && !w.left);
    }

    #[test]
    fn minimum_setter() {
        let m = EdgeInsets {
            left: 5.0,
            right: 5.0,
            top: 10.0,
            bottom: 10.0,
        };
        let w = SafeArea::new(Text::new("Hi")).minimum(m);
        assert_eq!(w.minimum, m);
    }

    #[test]
    fn clone_preserves_config() {
        let w = SafeArea::new(Text::new("Hi"))
            .top(false)
            .minimum(EdgeInsets {
                left: 1.0,
                right: 2.0,
                top: 3.0,
                bottom: 4.0,
            });
        let cloned = w.clone();
        assert_eq!(cloned.top, false);
        assert_eq!(cloned.minimum, w.minimum);
        assert!(cloned
            .child
            .as_ref()
            .as_any()
            .downcast_ref::<Text>()
            .is_some());
    }

    #[test]
    fn effective_padding_all_sides() {
        let w = SafeArea::new(Text::new("Hi"));
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        assert_eq!(w.effective_padding(insets), (10.0, 20.0, 30.0, 40.0));
    }

    #[test]
    fn effective_padding_opt_out() {
        let w = SafeArea::new(Text::new("Hi")).top(false).left(false);
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        assert_eq!(w.effective_padding(insets), (0.0, 20.0, 0.0, 40.0));
    }

    #[test]
    fn effective_padding_minimum_floor() {
        let min = EdgeInsets {
            left: 50.0,
            right: 50.0,
            top: 50.0,
            bottom: 50.0,
        };
        let w = SafeArea::new(Text::new("Hi")).minimum(min);
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        assert_eq!(w.effective_padding(insets), (50.0, 50.0, 50.0, 50.0));
    }

    #[test]
    fn effective_padding_no_floor_when_larger() {
        let min = EdgeInsets {
            left: 5.0,
            right: 5.0,
            top: 5.0,
            bottom: 5.0,
        };
        let w = SafeArea::new(Text::new("Hi")).minimum(min);
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        assert_eq!(w.effective_padding(insets), (10.0, 20.0, 30.0, 40.0));
    }
}
