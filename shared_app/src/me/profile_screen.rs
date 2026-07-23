//! Profile screen — the root of the Me tab.

use vexo::{
    children, Component, DecoratedBox, Layout, MultiChild, RenderContext, SimpleState, Style, Text,
    Theme, Widget, WithLayout,
};
use vexo_fontawesome::{Icon, Icons};

use crate::data::Profile;
use crate::widgets::avatar::avatar;
use crate::widgets::theme_toggle::ThemeToggle;

pub(crate) fn build_profile_screen(
    profile: &Profile,
    is_dark: vexo::Signal<bool>,
) -> Box<dyn Widget> {
    ProfileScreen {
        profile: profile.clone(),
        is_dark,
    }
    .boxed()
}

/// Profile screen component. Reads the theme via `Theme::of(ctx)` so it
/// re-themes when the ancestor `Theme` swaps light/dark.
#[derive(Clone)]
struct ProfileScreen {
    profile: Profile,
    is_dark: vexo::Signal<bool>,
}

impl Component for ProfileScreen {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let avatar = avatar(&self.profile.avatar_bytes, 80.0);

        let name = Text::new(self.profile.name.as_str())
            .with_font_size(22.0)
            .with_color(theme.on_background);
        let email = Text::new(self.profile.email.as_str())
            .with_font_size(14.0)
            .with_color(theme.on_surface_variant);

        let header = WithLayout::new(
            MultiChild::new(children![avatar, name, email], Layout::column().gap(4.0)),
            Layout::default().padding(24.0),
        );

        // Dark Mode toggle row (first), then the static settings rows.
        let mut settings_list = MultiChild::empty(Layout::column());
        settings_list = settings_list.push(build_toggle_row(self.is_dark.clone(), &theme));

        let static_settings = ["Settings", "Notifications", "About"];
        for label in static_settings {
            settings_list = settings_list.push(build_settings_row(label, &theme));
        }

        // Paint a themed background so the screen isn't left showing the
        // window's white clear in dark mode. The column fills the available
        // height (matching ChatScreen's root) so the background covers the
        // whole pane — without this the short content leaves a white gap at
        // the bottom below the settings rows.
        DecoratedBox::with_style(
            MultiChild::new(
                children![header, settings_list],
                Layout::column()
                    .flex_grow(1.0)
                    .flex_basis(0.0)
                    .min_height(0.0),
            ),
            Style::default().background(theme.background),
        )
        .boxed()
    }
}

/// The Dark Mode toggle row: icon + label on the left, ThemeToggle on the right.
fn build_toggle_row(is_dark: vexo::Signal<bool>, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let dark = is_dark.get();
    let icon = if dark { Icons::Sun } else { Icons::Moon };
    WithLayout::new(
        MultiChild::new(
            children![
                WithLayout::new(
                    MultiChild::new(
                        children![
                            Icon::new(icon)
                                .with_size(16.0)
                                .with_color(theme.on_surface_variant),
                            Text::new("Dark Mode")
                                .with_font_size(16.0)
                                .with_color(theme.on_background),
                        ],
                        Layout::row().gap(8.0).flex_grow(1.0),
                    ),
                    Layout::default().flex_grow(1.0),
                ),
                ThemeToggle::new(is_dark),
            ],
            Layout::row().gap(8.0).align(vexo::AlignItems::Center),
        ),
        Layout::default().padding(16.0),
    )
    .boxed()
}

fn build_settings_row(label: &str, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    WithLayout::new(
        MultiChild::new(
            children![
                WithLayout::new(
                    Text::new(label)
                        .with_font_size(16.0)
                        .with_color(theme.on_background),
                    Layout::default().flex_grow(1.0),
                ),
                Text::new("›")
                    .with_font_size(20.0)
                    .with_color(theme.on_surface_variant),
            ],
            Layout::row().gap(8.0),
        ),
        Layout::default().padding(16.0),
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
    fn test_profile_screen_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = build_profile_screen(&state.profile, state.is_dark.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 2,
            "expected multiple elements for profile header + settings rows"
        );
    }
}
