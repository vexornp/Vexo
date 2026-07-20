use std::rc::Rc;

use vexo::{DecoratedBox, Image, Layout, Style, Widget, WithLayout};

pub(crate) fn avatar(bytes: &Rc<[u8]>, diameter: f32) -> Box<dyn Widget> {
    DecoratedBox::with_style(
        WithLayout::new(
            Image::from_bytes(bytes)
                .expect("avatar bytes are valid PNG")
                .with_corner_radius(diameter / 2.0),
            Layout::default().width(diameter).height(diameter),
        ),
        Style::default().clip(),
    )
    .boxed()
}
