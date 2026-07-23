//! Contacts list screen.

use vexo::{
    children, Component, DecoratedBox, Layout, MultiChild, RenderContext, ScrollView, SimpleState,
    Style, Text, Theme, Widget, WithLayout,
};

use crate::data::Contact;
use crate::widgets::avatar::avatar;

pub(crate) fn build_contacts_screen(contacts: Vec<Contact>) -> Box<dyn Widget> {
    ContactsScreen { contacts }.boxed()
}

/// Contacts screen component. Reads the theme via `Theme::of(ctx)` so it
/// re-themes when the ancestor `Theme` swaps light/dark.
#[derive(Clone)]
struct ContactsScreen {
    contacts: Vec<Contact>,
}

impl Component for ContactsScreen {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let mut list = MultiChild::empty(Layout::column());
        for c in &self.contacts {
            list = list.push(build_contact_row(c, &theme));
        }
        // Paint a themed background behind the list so the pane isn't left
        // showing the window's white clear in dark mode.
        DecoratedBox::with_style(
            WithLayout::new(ScrollView::new(list.boxed()), Layout::flex_fill()),
            Style::default().background(theme.background),
        )
        .boxed()
    }
}

fn build_contact_row(c: &Contact, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let avatar = avatar(&c.avatar_bytes, 40.0);

    let name = Text::new(c.name.as_str())
        .with_font_size(16.0)
        .with_color(theme.on_background);
    let status = Text::new(c.status.as_str())
        .with_font_size(13.0)
        .with_color(theme.on_surface_variant);

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
