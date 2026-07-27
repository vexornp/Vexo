use vexo::{ClipRRect, Image, ImageData, Layout, Widget, WithLayout};

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
