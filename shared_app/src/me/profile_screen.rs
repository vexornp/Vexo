//! Profile screen — the root of the Me tab.

use vexo::{Color, Column, Flex, Row, Text, Widget};

use crate::data::Profile;
use crate::widgets::avatar::avatar;

pub(crate) fn build_profile_screen(profile: &Profile) -> Box<dyn Widget> {
    let avatar = avatar(&profile.avatar_bytes, 80.0);

    let name = Text::new(profile.name.as_str())
        .with_font_size(22.0)
        .with_color(Color::BLACK);
    let email = Text::new(profile.email.as_str())
        .with_font_size(14.0)
        .with_color(Color::rgb(0.5, 0.5, 0.5));

    let header = Column::new()
        .gap(4.0)
        .push(avatar)
        .push(name)
        .push(email)
        .boxed()
        .padding(24.0);

    let settings = vec!["Settings", "Notifications", "About"];
    let mut settings_list = Flex::column();
    for label in settings {
        settings_list = settings_list.push(
            Row::new()
                .gap(8.0)
                .push(
                    Text::new(label)
                        .with_font_size(16.0)
                        .with_color(Color::BLACK)
                        .flex_grow(1.0),
                )
                .push(
                    Text::new("›")
                        .with_font_size(20.0)
                        .with_color(Color::rgb(0.6, 0.6, 0.6)),
                )
                .boxed()
                .padding(16.0),
        );
    }

    Column::new()
        .push(header)
        .push(settings_list.boxed())
        .boxed()
}
