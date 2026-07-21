use std::rc::Rc;

use vexo::{ClipRRect, Image, Layout, Widget, WithLayout};

pub(crate) fn avatar(bytes: &Rc<[u8]>, diameter: f32) -> Box<dyn Widget> {
    ClipRRect::new(
        diameter / 2.0,
        WithLayout::new(
            Image::from_bytes(bytes).expect("avatar bytes are valid PNG"),
            Layout::default().width(diameter).height(diameter),
        ),
    )
    .boxed()
}
