//! Contacts list screen.

use vexo::{Color, Column, Flex, Row, ScrollView, Text, Widget};

use crate::data::Contact;
use crate::widgets::avatar::avatar;

pub(crate) fn build_contacts_screen(contacts: Vec<Contact>) -> Box<dyn Widget> {
    let mut list = Flex::column();
    for c in &contacts {
        list = list.push(build_contact_row(c));
    }
    ScrollView::new(list.boxed()).flex_fill().boxed()
}

fn build_contact_row(c: &Contact) -> Box<dyn Widget> {
    let avatar = avatar(&c.avatar_bytes, 40.0);

    let name = Text::new(c.name.as_str())
        .with_font_size(16.0)
        .with_color(Color::BLACK);
    let status = Text::new(c.status.as_str())
        .with_font_size(13.0)
        .with_color(Color::rgb(0.5, 0.5, 0.5));

    Row::new()
        .gap(12.0)
        .push(avatar)
        .push(
            Column::new()
                .gap(2.0)
                .push(name)
                .push(status)
                .flex_grow(1.0),
        )
        .boxed()
        .padding(12.0)
}
