//! Shadow showcase screen — manual smoke test for `BoxShadow` rendering.
//!
//! Displays four cards with distinct shadow configurations so the user
//! can visually verify blur, offset, stacking, and colored glow.

use vexo::{BoxShadow, Color, Column, DecoratedContainer, ScrollView, Text, Widget};

pub(crate) fn build_shadow_showcase_screen() -> Box<dyn Widget> {
    let card_with_shadow = DecoratedContainer::new(
        Text::new("Card with soft shadow")
            .with_font_size(16.0)
            .with_color(Color::BLACK),
    )
    .background(Color::WHITE)
    .corner_radius(12.0)
    .padding(24.0)
    .shadow(
        BoxShadow::new(Color::new(0.0, 0.0, 0.0, 0.15))
            .offset(0.0, 4.0)
            .blur(12.0),
    )
    .boxed();

    let card_with_stacked_shadows = DecoratedContainer::new(
        Text::new("Card with stacked shadows")
            .with_font_size(16.0)
            .with_color(Color::BLACK),
    )
    .background(Color::WHITE)
    .corner_radius(12.0)
    .padding(24.0)
    .shadow(
        BoxShadow::new(Color::new(0.0, 0.0, 0.0, 0.10))
            .offset(0.0, 2.0)
            .blur(4.0),
    )
    .shadow(
        BoxShadow::new(Color::new(0.0, 0.0, 0.0, 0.15))
            .offset(0.0, 8.0)
            .blur(16.0),
    )
    .boxed();

    let elevated_button = DecoratedContainer::new(
        Text::new("Elevated button")
            .with_font_size(16.0)
            .with_color(Color::WHITE),
    )
    .background(Color::new(0.2, 0.6, 1.0, 1.0))
    .corner_radius(8.0)
    .padding(16.0)
    .shadow(
        BoxShadow::new(Color::new(0.0, 0.0, 0.0, 0.3))
            .offset(0.0, 2.0)
            .blur(4.0),
    )
    .boxed();

    let glowing_card = DecoratedContainer::new(
        Text::new("Card with colored glow")
            .with_font_size(16.0)
            .with_color(Color::BLACK),
    )
    .background(Color::WHITE)
    .corner_radius(12.0)
    .padding(24.0)
    .shadow(
        BoxShadow::new(Color::new(0.4, 0.2, 1.0, 0.5))
            .offset(0.0, 0.0)
            .blur(20.0)
            .spread(2.0),
    )
    .boxed();

    let list = Column::new()
        .gap(24.0)
        .push(card_with_shadow)
        .push(card_with_stacked_shadows)
        .push(elevated_button)
        .push(glowing_card)
        .boxed()
        .padding(40.0);

    ScrollView::new(list).flex_fill().boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_shadow_showcase_renders_in_pipeline() {
        let view = build_shadow_showcase_screen();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 4 shadow cards + scroll view"
        );
    }
}
