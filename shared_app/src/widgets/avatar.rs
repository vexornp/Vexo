//! Deduped circular avatar builder.
//!
//! Replaces the 5 inline `Image::from_bytes(...).width(d).height(d)
//! .corner_radius(d/2).clip()` blocks that were copy-pasted across
//! all four screens.

use std::rc::Rc;

use vexo::{Image, Widget};

/// Build a circular avatar widget from PNG bytes.
///
/// `diameter` sets both width and height; corner radius is half the
/// diameter for a perfect circle; `clip()` rounds the visible corners.
///
/// Uses `Image`'s inherent `.width()`, `.height()`, `.corner_radius()`,
/// `.clip()` methods (via `modifier_methods!`) — these set the layout
/// and style fields directly on the `Image` widget without wrapping in
/// `WithLayout`/`DecoratedBox`. `ImageRenderObject` handles the clip
/// and corner radius natively, preserving correct z-order with sibling
/// bars.
pub(crate) fn avatar(bytes: &Rc<[u8]>, diameter: f32) -> Box<dyn Widget> {
    Image::from_bytes(bytes)
        .expect("avatar bytes are valid PNG")
        .width(diameter)
        .height(diameter)
        .corner_radius(diameter / 2.0)
        .clip()
        .boxed()
}
