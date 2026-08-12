use url::Url;
use vexo::{
    ClipRRect, Color, DecoratedBox, Image, ImageData, Layout, NetworkImage, Positioned, Spacer,
    Style, Widget, WithLayout,
};

pub(crate) fn avatar(image_data: ImageData, diameter: f32) -> Box<dyn Widget> {
    ClipRRect::new(
        diameter / 2.0,
        WithLayout::new(
            Image::new(image_data),
            Layout::default().width(diameter).height(diameter),
        ),
    )
    .boxed()
}

/// Circular avatar backed by a remote URL. Wraps `NetworkImage` in the same
/// `ClipRRect` + sizing as `avatar`, so the slot is layout-stable across
/// `Loading`/`Loaded`/`Error` (always `diameter × diameter`). The
/// `NetworkImage` key is set to the URL string so list reconciliation reuses
/// the element when the list reorders (see `NetworkImage` docs).
///
/// No placeholder/error widget is provided: while loading or on fetch error,
/// the circle is blank (a `Spacer` sized to `diameter`). This is acceptable
/// for the mock — first load is brief, subsequent renders hit `ImageCache`
/// synchronously. Layout never shifts because the `WithLayout` pins the size.
pub(crate) fn network_avatar(url: Url, diameter: f32) -> Box<dyn Widget> {
    let key = url.as_str().to_string();
    ClipRRect::new(
        diameter / 2.0,
        WithLayout::new(
            NetworkImage::new(url).with_key(key),
            Layout::default().width(diameter).height(diameter),
        ),
    )
    .boxed()
}

/// A 1px circular border ring sized to `diameter`, painted in `color`.
///
/// Returns a `Positioned` overlay meant to be pushed onto a `Stack` *above*
/// an avatar built by `avatar` / `network_avatar` (same `diameter`). The
/// overlay is positioned at the Stack's top-left with explicit
/// `width`/`height` equal to `diameter`, so it occupies the same box as the
/// avatar's `ClipRRect`. The `DecoratedBox`'s `corner_radius(diameter / 2.0)`
/// traces the same circle the avatar is clipped to, and the shader paints
/// the 1px border inward from that silhouette — so the ring sits exactly on
/// the clipped image edge.
///
/// Paint order is push order in a `Stack`, so the caller must push this
/// *after* the avatar (and before any badge). Transparent default fill means
/// only the ring paints; the avatar shows through the interior.
///
/// Why a ring at all: a white-background avatar is invisible against a white
/// pane. The ring guarantees the circle reads regardless of image content.
pub(crate) fn avatar_border_ring(diameter: f32, color: Color) -> Box<dyn Widget> {
    Positioned::new(DecoratedBox::with_style(
        WithLayout::new(
            Spacer::new(),
            Layout::default().width(diameter).height(diameter),
        ),
        Style::default()
            .border(color, 1.0)
            .corner_radius(diameter / 2.0),
    ))
    .top(0.0)
    .left(0.0)
    .width(diameter)
    .height(diameter)
    .boxed()
}
