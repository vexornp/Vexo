use std::any::Any;

use vexo::{
    AlignItems, AlignSelf, ClipRRect, Color, Component, ComponentState, DecoratedBox, Image,
    ImageData, JustifyContent, Layout, LifecycleContext, NetworkImage, Positioned, RenderContext,
    Spacer, Stack, Style, Text, Theme, ThemeData, Widget, WithLayout,
};

use crate::data::AvatarSource;

// ---------------------------------------------------------------------------
// Unified Avatar Component
// ---------------------------------------------------------------------------

/// Unified avatar widget: clipped circular image + optional 1px ring +
/// optional unread badge. Owns its PNG decode cache so the image is decoded
/// once and reused across renders.
///
/// Builder API mirrors `Text`/`Image` conventions:
///   `Avatar::new(source, diameter).with_ring(true).with_unread_badge(count)`
#[derive(Clone)]
pub(crate) struct Avatar {
    source: AvatarSource,
    diameter: f32,
    ring: bool,
    unread_badge: Option<u32>,
}

impl Avatar {
    pub(crate) fn new(source: AvatarSource, diameter: f32) -> Self {
        Self {
            source,
            diameter,
            ring: false,
            unread_badge: None,
        }
    }

    pub(crate) fn with_ring(mut self, ring: bool) -> Self {
        self.ring = ring;
        self
    }

    pub(crate) fn with_unread_badge(mut self, count: u32) -> Self {
        self.unread_badge = Some(count);
        self
    }
}

/// Decode cache. `image` is lazily populated on first `render()` for the
/// `Bytes` source path. `Url` sources need no decode — `NetworkImage` +
/// `ImageCache` handle fetch/decode.
pub(crate) struct AvatarState {
    image: Option<ImageData>,
}

impl Default for AvatarState {
    fn default() -> Self {
        Self { image: None }
    }
}

impl ComponentState for AvatarState {
    fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut LifecycleContext) {
        let old = old_widget
            .downcast_ref::<Avatar>()
            .expect("old widget is Avatar");
        let new = ctx
            .widget()
            .downcast_ref::<Avatar>()
            .expect("new widget is Avatar");
        if old.source != new.source {
            self.image = None;
        }
    }
}

impl Component for Avatar {
    type State = AvatarState;

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let diameter = self.diameter;

        let base: Box<dyn Widget> = match &self.source {
            AvatarSource::Bytes(bytes) => {
                let image = state.image.get_or_insert_with(|| {
                    #[cfg(test)]
                    DECODE_COUNT.with(|c| c.set(c.get() + 1));
                    ImageData::from_bytes(bytes).expect("avatar bytes are valid PNG")
                });
                ClipRRect::new(
                    diameter / 2.0,
                    WithLayout::new(
                        Image::new(image.clone()),
                        Layout::default().width(diameter).height(diameter),
                    ),
                )
                .boxed()
            }
            AvatarSource::Url(url) => {
                let key = url.as_str().to_string();
                ClipRRect::new(
                    diameter / 2.0,
                    WithLayout::new(
                        NetworkImage::new(url.clone()).with_key(key),
                        Layout::default().width(diameter).height(diameter),
                    ),
                )
                .boxed()
            }
        };

        let mut stack = Stack::new()
            .with_layout(
                Layout::stack()
                    .width(diameter)
                    .height(diameter)
                    .flex_shrink(0.0),
            )
            .push(base);

        if self.ring {
            stack = stack.push(border_ring(diameter, theme.outline));
        }

        if let Some(count) = self.unread_badge {
            if count > 0 {
                let badge = Positioned::new(unread_badge(count, &theme))
                    .top(-4.0)
                    .right(-4.0)
                    .boxed();
                stack = stack.push(badge);
            }
        }

        stack.boxed()
    }
}

/// 1px circular border ring sized to `diameter`, painted in `color`.
fn border_ring(diameter: f32, color: Color) -> Box<dyn Widget> {
    Positioned::new(DecoratedBox::with_style(
        WithLayout::new(
            Spacer::new(),
            Layout::default().width(diameter).height(diameter),
        ),
        Style::default()
            .border(color, 1.0)
            .corner_radius(diameter / 2.0),
    ))
    .top(0.0)
    .left(0.0)
    .width(diameter)
    .height(diameter)
    .boxed()
}

/// Unread-count badge: red circle with white number. Moved here from
/// `conversation_list.rs` so the `Avatar` widget owns badge rendering.
fn unread_badge(count: u32, theme: &ThemeData) -> Box<dyn Widget> {
    DecoratedBox::with_style(
        WithLayout::new(
            Text::new(count.to_string())
                .with_font_size(11.0)
                .with_color(theme.on_error),
            Layout::default()
                .width(20.0)
                .height(20.0)
                .justify(JustifyContent::Center)
                .align(AlignItems::Center)
                .align_self(AlignSelf::Start)
                .flex_shrink(0.0),
        ),
        Style::default().background(theme.error).corner_radius(10.0),
    )
    .boxed()
}

#[cfg(test)]
thread_local! {
    static DECODE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    use crate::data::{make_avatar_png, AvatarSource};

    #[test]
    fn avatar_renders_bytes_without_panic() {
        let bytes = make_avatar_png(255, 0, 0);
        let view = Avatar::new(AvatarSource::Bytes(bytes), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 0,
            "expected at least one element for bytes avatar"
        );
    }

    #[test]
    fn avatar_renders_url_without_panic() {
        let url = url::Url::parse("https://example.com/avatar.png").unwrap();
        let view = Avatar::new(AvatarSource::Url(url), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 0,
            "expected at least one element for url avatar"
        );
    }

    #[test]
    fn avatar_with_badge_and_ring_has_more_elements_than_bare() {
        let bytes = make_avatar_png(255, 0, 0);
        let source = AvatarSource::Bytes(bytes);

        let bare = Avatar::new(source.clone(), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(bare);
        let bare_count = pipeline.element_registry().len();

        let full = Avatar::new(source, 40.0)
            .with_ring(true)
            .with_unread_badge(5)
            .boxed();
        let mut pipeline2 = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline2);
        pipeline2.update(full);
        let full_count = pipeline2.element_registry().len();

        assert!(
            full_count > bare_count,
            "avatar with ring + badge ({}) should have more elements than bare ({})",
            full_count,
            bare_count
        );
    }

    #[test]
    fn avatar_caches_decode() {
        DECODE_COUNT.with(|c| c.set(0));
        let bytes = make_avatar_png(255, 0, 0);
        let source = AvatarSource::Bytes(bytes);

        let view = Avatar::new(source.clone(), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert_eq!(
            DECODE_COUNT.with(|c| c.get()),
            1,
            "first render should decode exactly once"
        );

        let view2 = Avatar::new(source, 40.0).boxed();
        pipeline.update(view2);
        assert_eq!(
            DECODE_COUNT.with(|c| c.get()),
            1,
            "second render should hit cache, not re-decode"
        );
    }
}
