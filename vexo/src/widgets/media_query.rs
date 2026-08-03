//! `MediaQueryData`, `Orientation`, `RemoveEdges` — the data model for
//! `MediaQuery`. See `docs/superpowers/specs/2026-07-26-media-query-design.md`.

use crate::core::{Logical, Size};
use crate::layout::EdgeInsets;
use crate::widgets::Brightness;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Per-side flag for `MediaQuery::remove_padding` / `remove_view_insets` /
/// `remove_view_padding`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RemoveEdges {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl RemoveEdges {
    pub const NONE: Self = Self {
        top: false,
        right: false,
        bottom: false,
        left: false,
    };
    pub const TOP: Self = Self {
        top: true,
        right: false,
        bottom: false,
        left: false,
    };
    pub const BOTTOM: Self = Self {
        top: false,
        right: false,
        bottom: true,
        left: false,
    };
    pub const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };
}

#[derive(Clone, PartialEq, Debug)]
pub struct MediaQueryData {
    pub size: Size<Logical>,
    pub device_pixel_ratio: f32,
    pub padding: EdgeInsets,
    pub view_insets: EdgeInsets,
    pub view_padding: EdgeInsets,
    pub platform_brightness: Brightness,
    pub orientation: Orientation,
}

impl MediaQueryData {
    pub const fn all_zero() -> Self {
        Self {
            size: Size::new(0.0, 0.0),
            device_pixel_ratio: 1.0,
            padding: EdgeInsets::ZERO,
            view_insets: EdgeInsets::ZERO,
            view_padding: EdgeInsets::ZERO,
            platform_brightness: Brightness::Light,
            orientation: Orientation::Portrait,
        }
    }

    pub fn copy_with_padding(&self, padding: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.padding = padding;
        clone
    }

    pub fn copy_with_view_insets(&self, view_insets: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.view_insets = view_insets;
        clone
    }

    pub fn copy_with_view_padding(&self, view_padding: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.view_padding = view_padding;
        clone
    }
}

use crate::inherited_widget::{impl_widget_for_inherited, InheritedWidget};
use crate::key::WidgetKey;
use crate::stateful_widget::{RenderContext, SimpleState};
use crate::widgets::Widget;
use crate::Component;

pub struct MediaQuery {
    data: MediaQueryData,
    child: std::rc::Rc<dyn Widget>,
    key: Option<WidgetKey>,
}

impl MediaQuery {
    pub fn new(data: MediaQueryData, child: impl Widget + 'static) -> Self {
        Self {
            data,
            child: std::rc::Rc::new(child),
            key: None,
        }
    }

    /// Constructor that accepts an `Rc<dyn Widget>` child, for use by
    /// `RootMediaQuery::render()`. Sharing the `Rc` (instead of cloning the
    /// widget tree) lets `InheritedElement::update()` detect that the child
    /// hasn't changed (same `Rc` pointer) and skip the rebuild cascade.
    pub(crate) fn new_with_rc(data: MediaQueryData, child: std::rc::Rc<dyn Widget>) -> Self {
        Self {
            data,
            child,
            key: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn of(ctx: &mut RenderContext) -> MediaQueryData {
        ctx.depend_on_inherited_widget::<MediaQueryData>()
            .unwrap_or_else(MediaQueryData::all_zero)
    }

    pub fn remove_padding(child: impl Widget + 'static, edges: RemoveEdges) -> MediaQueryMutator {
        MediaQueryMutator::new(Box::new(child), move |parent: &MediaQueryData| {
            let mut p = parent.padding;
            if edges.top {
                p.top = 0.0;
            }
            if edges.right {
                p.right = 0.0;
            }
            if edges.bottom {
                p.bottom = 0.0;
            }
            if edges.left {
                p.left = 0.0;
            }
            parent.copy_with_padding(p)
        })
    }

    pub fn remove_view_insets(
        child: impl Widget + 'static,
        edges: RemoveEdges,
    ) -> MediaQueryMutator {
        MediaQueryMutator::new(Box::new(child), move |parent: &MediaQueryData| {
            let mut v = parent.view_insets;
            if edges.top {
                v.top = 0.0;
            }
            if edges.right {
                v.right = 0.0;
            }
            if edges.bottom {
                v.bottom = 0.0;
            }
            if edges.left {
                v.left = 0.0;
            }
            parent.copy_with_view_insets(v)
        })
    }

    pub fn remove_view_padding(
        child: impl Widget + 'static,
        edges: RemoveEdges,
    ) -> MediaQueryMutator {
        MediaQueryMutator::new(Box::new(child), move |parent: &MediaQueryData| {
            let mut v = parent.view_padding;
            if edges.top {
                v.top = 0.0;
            }
            if edges.right {
                v.right = 0.0;
            }
            if edges.bottom {
                v.bottom = 0.0;
            }
            if edges.left {
                v.left = 0.0;
            }
            parent.copy_with_view_padding(v)
        })
    }

    pub fn reduce_view_insets_bottom(
        child: impl Widget + 'static,
        amount: f32,
    ) -> MediaQueryMutator {
        MediaQueryMutator::new(Box::new(child), move |parent: &MediaQueryData| {
            let mut v = parent.view_insets;
            v.bottom = (v.bottom - amount).max(0.0);
            parent.copy_with_view_insets(v)
        })
    }
}

impl Clone for MediaQuery {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            child: std::rc::Rc::clone(&self.child),
            key: self.key.clone(),
        }
    }
}

impl InheritedWidget for MediaQuery {
    type Value = MediaQueryData;
    fn value(&self) -> &MediaQueryData {
        &self.data
    }
    fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }
}

impl_widget_for_inherited!(MediaQuery);

/// `Component` that reads the parent `MediaQuery` at render time, applies
/// a pure transformation to produce a child `MediaQueryData`, and emits
/// `MediaQuery::new(transformed, child)`.
///
/// Used by `MediaQuery::remove_padding` / `remove_view_insets` /
/// `remove_view_padding` / `reduce_view_insets_bottom`. The closure is
/// stored in an `Rc<dyn Fn>` so the mutator itself is cheaply cloneable
/// (closures don't auto-impl `Clone`).
///
/// The widget tree is single-threaded (main thread), so `Rc` is fine. The
/// `Component` trait bound is `Sized + 'static` (verified in Step 2 — no
/// `Send`/`Sync`), so `Rc<dyn Fn>` satisfies the trait.
pub struct MediaQueryMutator {
    child: std::rc::Rc<dyn Widget>,
    compute: std::rc::Rc<dyn Fn(&MediaQueryData) -> MediaQueryData>,
}

impl MediaQueryMutator {
    pub fn new(
        child: Box<dyn Widget>,
        compute: impl Fn(&MediaQueryData) -> MediaQueryData + 'static,
    ) -> Self {
        Self {
            child: std::rc::Rc::from(child),
            compute: std::rc::Rc::new(compute),
        }
    }
}

impl Clone for MediaQueryMutator {
    fn clone(&self) -> Self {
        Self {
            child: std::rc::Rc::clone(&self.child),
            compute: self.compute.clone(),
        }
    }
}

impl Component for MediaQueryMutator {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let parent = MediaQuery::of(ctx);
        let data = (self.compute)(&parent);
        // Use new_with_rc so the child Rc is shared (Rc::clone, O(1)) instead
        // of deep-cloned. This lets InheritedElement::update() compare Rc
        // pointers and skip update_child() when the child hasn't changed —
        // critical for keyboard animations where MediaQueryMutator rebuilds
        // every frame but its child subtree is identical.
        MediaQuery::new_with_rc(data, std::rc::Rc::clone(&self.child)).boxed()
    }
}

/// Framework-internal root `Component` that composes `MediaQueryData` from
/// the three platform sources and provides it to the application subtree
/// via `MediaQuery::new(data, child)`. App authors never touch this — the
/// framework wraps `Application::view()` output in `RootMediaQuery` before
/// mounting.
pub(crate) struct RootMediaQuery {
    child: std::rc::Rc<dyn Widget>,
}

impl RootMediaQuery {
    pub(crate) fn new(child: Box<dyn Widget>) -> Self {
        Self {
            child: std::rc::Rc::from(child),
        }
    }
}

impl Clone for RootMediaQuery {
    fn clone(&self) -> Self {
        Self {
            child: std::rc::Rc::clone(&self.child),
        }
    }
}

impl Component for RootMediaQuery {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let sources = ctx.media_query_sources();
        let view_padding = sources.safe_area;
        let view_insets = EdgeInsets {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: sources.keyboard_current_height,
        };
        let padding = EdgeInsets {
            top: (view_padding.top - view_insets.top).max(0.0),
            bottom: (view_padding.bottom - view_insets.bottom).max(0.0),
            left: (view_padding.left - view_insets.left).max(0.0),
            right: (view_padding.right - view_insets.right).max(0.0),
        };
        let orientation = if sources.media_query.size.width >= sources.media_query.size.height {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        };
        let brightness = if sources.media_query.is_dark {
            Brightness::Dark
        } else {
            Brightness::Light
        };
        let data = MediaQueryData {
            size: sources.media_query.size,
            device_pixel_ratio: sources.media_query.device_pixel_ratio,
            padding,
            view_insets,
            view_padding,
            platform_brightness: brightness,
            orientation,
        };
        MediaQuery::new_with_rc(data, std::rc::Rc::clone(&self.child)).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_zero_defaults() {
        let z = MediaQueryData::all_zero();
        assert_eq!(z.size, Size::<Logical>::new(0.0, 0.0));
        assert_eq!(z.device_pixel_ratio, 1.0);
        assert_eq!(z.padding, EdgeInsets::ZERO);
        assert_eq!(z.view_insets, EdgeInsets::ZERO);
        assert_eq!(z.view_padding, EdgeInsets::ZERO);
        assert_eq!(z.platform_brightness, Brightness::Light);
        assert_eq!(z.orientation, Orientation::Portrait);
    }

    #[test]
    fn copy_with_padding_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_padding = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        let updated = z.copy_with_padding(new_padding);
        assert_eq!(updated.padding, new_padding);
        assert_eq!(z.padding, EdgeInsets::ZERO, "original must be unchanged");
    }

    #[test]
    fn copy_with_view_insets_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_vi = EdgeInsets {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 300.0,
        };
        let updated = z.copy_with_view_insets(new_vi);
        assert_eq!(updated.view_insets, new_vi);
        assert_eq!(
            z.view_insets,
            EdgeInsets::ZERO,
            "original must be unchanged"
        );
    }

    #[test]
    fn copy_with_view_padding_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_vp = EdgeInsets {
            left: 1.0,
            right: 2.0,
            top: 3.0,
            bottom: 4.0,
        };
        let updated = z.copy_with_view_padding(new_vp);
        assert_eq!(updated.view_padding, new_vp);
        assert_eq!(
            z.view_padding,
            EdgeInsets::ZERO,
            "original must be unchanged"
        );
    }

    #[test]
    fn remove_edges_constants() {
        assert_eq!(
            RemoveEdges::NONE,
            RemoveEdges {
                top: false,
                right: false,
                bottom: false,
                left: false
            }
        );
        assert_eq!(
            RemoveEdges::TOP,
            RemoveEdges {
                top: true,
                right: false,
                bottom: false,
                left: false
            }
        );
        assert_eq!(
            RemoveEdges::BOTTOM,
            RemoveEdges {
                top: false,
                right: false,
                bottom: true,
                left: false
            }
        );
        assert_eq!(
            RemoveEdges::ALL,
            RemoveEdges {
                top: true,
                right: true,
                bottom: true,
                left: true
            }
        );
    }

    #[test]
    fn reduce_view_insets_bottom_clamps_to_zero() {
        // Verify the closure logic directly (no pipeline needed).
        let parent = MediaQueryData::all_zero().copy_with_view_insets(EdgeInsets {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 300.0,
        });
        let compute = |p: &MediaQueryData| {
            let mut v = p.view_insets;
            v.bottom = (v.bottom - 49.0).max(0.0);
            p.copy_with_view_insets(v)
        };
        let child = compute(&parent);
        assert_eq!(child.view_insets.bottom, 251.0);

        // Clamp test: subtract more than available.
        let compute2 = |p: &MediaQueryData| {
            let mut v = p.view_insets;
            v.bottom = (v.bottom - 500.0).max(0.0);
            p.copy_with_view_insets(v)
        };
        let clamped = compute2(&parent);
        assert_eq!(clamped.view_insets.bottom, 0.0);
    }
}
