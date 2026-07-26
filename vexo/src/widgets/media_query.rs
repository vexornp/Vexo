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
/// `remove_view_padding`. Replaces the deleted `SafeAreaClaimEdges`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RemoveEdges {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl RemoveEdges {
    pub const NONE: Self = Self { top: false, right: false, bottom: false, left: false };
    pub const TOP: Self = Self { top: true, right: false, bottom: false, left: false };
    pub const BOTTOM: Self = Self { top: false, right: false, bottom: true, left: false };
    pub const ALL: Self = Self { top: true, right: true, bottom: true, left: true };
}

#[derive(Clone, PartialEq, Debug)]
pub struct MediaQueryData {
    pub size: Size<Logical>,
    pub device_pixel_ratio: f32,
    pub padding: EdgeInsets,
    pub viewInsets: EdgeInsets,
    pub viewPadding: EdgeInsets,
    pub platform_brightness: Brightness,
    pub orientation: Orientation,
}

impl MediaQueryData {
    pub const fn all_zero() -> Self {
        Self {
            size: Size::new(0.0, 0.0),
            device_pixel_ratio: 1.0,
            padding: EdgeInsets::ZERO,
            viewInsets: EdgeInsets::ZERO,
            viewPadding: EdgeInsets::ZERO,
            platform_brightness: Brightness::Light,
            orientation: Orientation::Portrait,
        }
    }

    pub fn copy_with_padding(&self, padding: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.padding = padding;
        clone
    }

    pub fn copy_with_view_insets(&self, viewInsets: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.viewInsets = viewInsets;
        clone
    }

    pub fn copy_with_view_padding(&self, viewPadding: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.viewPadding = viewPadding;
        clone
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
        assert_eq!(z.viewInsets, EdgeInsets::ZERO);
        assert_eq!(z.viewPadding, EdgeInsets::ZERO);
        assert_eq!(z.platform_brightness, Brightness::Light);
        assert_eq!(z.orientation, Orientation::Portrait);
    }

    #[test]
    fn copy_with_padding_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_padding = EdgeInsets { left: 10.0, right: 20.0, top: 30.0, bottom: 40.0 };
        let updated = z.copy_with_padding(new_padding);
        assert_eq!(updated.padding, new_padding);
        assert_eq!(z.padding, EdgeInsets::ZERO, "original must be unchanged");
    }

    #[test]
    fn copy_with_view_insets_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_vi = EdgeInsets { left: 0.0, right: 0.0, top: 0.0, bottom: 300.0 };
        let updated = z.copy_with_view_insets(new_vi);
        assert_eq!(updated.viewInsets, new_vi);
        assert_eq!(z.viewInsets, EdgeInsets::ZERO, "original must be unchanged");
    }

    #[test]
    fn copy_with_view_padding_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_vp = EdgeInsets { left: 1.0, right: 2.0, top: 3.0, bottom: 4.0 };
        let updated = z.copy_with_view_padding(new_vp);
        assert_eq!(updated.viewPadding, new_vp);
        assert_eq!(z.viewPadding, EdgeInsets::ZERO, "original must be unchanged");
    }

    #[test]
    fn remove_edges_constants() {
        assert_eq!(RemoveEdges::NONE, RemoveEdges { top: false, right: false, bottom: false, left: false });
        assert_eq!(RemoveEdges::TOP, RemoveEdges { top: true, right: false, bottom: false, left: false });
        assert_eq!(RemoveEdges::BOTTOM, RemoveEdges { top: false, right: false, bottom: true, left: false });
        assert_eq!(RemoveEdges::ALL, RemoveEdges { top: true, right: true, bottom: true, left: true });
    }
}
