use url::Url;
use vexo::{ClipRRect, Image, ImageData, Layout, NetworkImage, Widget, WithLayout};

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
