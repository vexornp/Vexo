//! Profile screen — the root of the Me tab.

use vexo::{children, Color, Layout, MultiChild, Text, Widget, WithLayout};

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

    let header = WithLayout::new(
        MultiChild::new(children![avatar, name, email], Layout::column().gap(4.0)),
        Layout::default().padding(24.0),
    );

    let settings = vec!["Settings", "Notifications", "About"];
    let mut settings_list = MultiChild::empty(Layout::column());
    for label in settings {
        settings_list = settings_list.push(WithLayout::new(
            MultiChild::new(
                children![
                    WithLayout::new(
                        Text::new(label)
                            .with_font_size(16.0)
                            .with_color(Color::BLACK),
                        Layout::default().flex_grow(1.0),
                    ),
                    Text::new("›")
                        .with_font_size(20.0)
                        .with_color(Color::rgb(0.6, 0.6, 0.6)),
                ],
                Layout::row().gap(8.0),
            ),
            Layout::default().padding(16.0),
        ));
    }

    MultiChild::new(children![header, settings_list], Layout::column()).boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_profile_screen_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = build_profile_screen(&state.profile);
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 2,
            "expected multiple elements for profile header + settings rows"
        );
    }
}
