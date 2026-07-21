//! Contacts list screen.

use vexo::{children, Color, Layout, MultiChild, ScrollView, Text, Widget, WithLayout};

use crate::data::Contact;
use crate::widgets::avatar::avatar;

pub(crate) fn build_contacts_screen(contacts: Vec<Contact>) -> Box<dyn Widget> {
    let mut list = MultiChild::empty(Layout::column());
    for c in &contacts {
        list = list.push(build_contact_row(c));
    }
    WithLayout::new(ScrollView::new(list.boxed()), Layout::flex_fill()).boxed()
}

fn build_contact_row(c: &Contact) -> Box<dyn Widget> {
    let avatar = avatar(&c.avatar_bytes, 40.0);

    let name = Text::new(c.name.as_str())
        .with_font_size(16.0)
        .with_color(Color::BLACK);
    let status = Text::new(c.status.as_str())
        .with_font_size(13.0)
        .with_color(Color::rgb(0.5, 0.5, 0.5));

    WithLayout::new(
        MultiChild::new(
            children![
                avatar,
                MultiChild::new(
                    children![name, status],
                    Layout::column().gap(2.0).flex_grow(1.0),
                ),
            ],
            Layout::row().gap(12.0),
        ),
        Layout::default().padding(12.0),
    )
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_contacts_screen_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = build_contacts_screen(state.contacts.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 8 contacts"
        );
    }
}
