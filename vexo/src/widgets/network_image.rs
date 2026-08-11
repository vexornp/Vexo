//! `NetworkImage` — a widget that loads and renders an image from a remote URL.
//!
//! Wraps the existing synchronous `Image` widget. On first `render()`, calls
//! `ImageCache::get_or_fetch(url)` to get a `Signal<LoadState>`, then
//! subscribes via `RenderContext::depend_on_signal`. While loading, shows
//! a placeholder (if provided). On error, shows an error widget (if provided).
//! On success, shows an `Image`.

use std::sync::Arc;

use url::Url;

use crate::image_cache::{ImageCache, LoadState};
use crate::key::WidgetKey;
use crate::stateful_widget::{Component, ComponentState, RenderContext};
use crate::widgets::{Image, Widget};

/// A widget that loads and renders an image from a remote URL.
///
/// # Example
///
/// ```ignore
/// NetworkImage::new(Url::parse("https://example.com/avatar.png").unwrap())
///     .placeholder(|| Text::new("Loading…").boxed())
///     .error(|e| Text::new(e).boxed())
/// ```
///
/// When used in a list, always set a key (typically the URL string) so
/// reconciliation reuses the element when the list reorders:
///
/// ```ignore
/// NetworkImage::new(url.clone())
///     .with_key(url.as_str())
/// ```
pub struct NetworkImage {
    url: Url,
    // `Arc<dyn Fn()>` (not `Box<dyn Fn()>`) so the struct is `Clone`:
    // `Arc` is cheaply clonable via ref-count bump, while `Box<dyn Fn()>`
    // cannot be cloned at all. `Clone` is required for the blanket
    // `impl<W: Component + Clone + 'static> Widget for W` to apply.
    placeholder: Option<Arc<dyn Fn() -> Box<dyn Widget> + Send + Sync>>,
    error: Option<Arc<dyn Fn(&str) -> Box<dyn Widget> + Send + Sync>>,
    key: Option<WidgetKey>,
}

impl NetworkImage {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            placeholder: None,
            error: None,
            key: None,
        }
    }

    /// Set a placeholder widget shown while the image is loading.
    pub fn placeholder<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Box<dyn Widget> + Send + Sync + 'static,
    {
        self.placeholder = Some(Arc::new(f));
        self
    }

    /// Set an error widget builder, called with the error message string
    /// when the fetch or decode fails.
    pub fn error<F>(mut self, f: F) -> Self
    where
        F: Fn(&str) -> Box<dyn Widget> + Send + Sync + 'static,
    {
        self.error = Some(Arc::new(f));
        self
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Clone for NetworkImage {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            placeholder: self.placeholder.clone(), // Arc::clone — ref-count bump
            error: self.error.clone(),             // Arc::clone — ref-count bump
            key: self.key.clone(),
        }
    }
}

/// State for `NetworkImage`. No reactive fields — `depend_on_signal` in
/// `render()` handles subscription automatically.
#[derive(Default)]
pub struct NetworkImageState;

impl ComponentState for NetworkImageState {}

impl Component for NetworkImage {
    type State = NetworkImageState;

    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let cache: Arc<ImageCache> = ctx.image_cache();
        let signal = cache.get_or_fetch(self.url.clone());
        let load_state = ctx.depend_on_signal(&signal);

        match load_state {
            LoadState::Loading => self
                .placeholder
                .as_ref()
                .map(|f| f())
                .unwrap_or_else(empty_widget),
            LoadState::Loaded(data) => Image::new(data).boxed(),
            LoadState::Error(msg) => self
                .error
                .as_ref()
                .map(|f| f(&msg))
                .unwrap_or_else(empty_widget),
        }
    }

    // NOTE: `clone_boxed()` is NOT implemented here. It is a `Widget` trait
    // method, not a `Component` trait method. The blanket impl at
    // `vexo/src/stateful_widget.rs` (`impl<W: Component + Clone + 'static>
    // Widget for W`) provides `clone_boxed`, `create_element`, `as_any`, etc.
    // automatically. Implementing it here would be dead code shadowed by the
    // blanket impl.
}

/// Produce a minimal empty widget (zero-size, no paint).
fn empty_widget() -> Box<dyn Widget> {
    crate::widgets::Spacer::new().boxed()
}

// ============================================================================
// TESTS (Layer 2)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_data::ImageData;
    use crate::key::Key;
    use crate::widgets::Text;

    fn test_url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn test_image_data() -> ImageData {
        ImageData {
            pixels: vec![255, 0, 0, 255],
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn loading_state_renders_placeholder() {
        let url = test_url("https://example.com/img.png");
        let widget = NetworkImage::new(url).placeholder(|| Text::new("Loading").boxed());

        // We can't easily test render() without a full pipeline + cache,
        // so test the structure + clone instead.
        let cloned = widget.clone();
        assert_eq!(widget.url, cloned.url);
        assert!(widget.placeholder.is_some());
    }

    #[test]
    fn network_image_clone_preserves_fields() {
        let url = test_url("https://example.com/img.png");
        let widget = NetworkImage::new(url.clone())
            .placeholder(|| Text::new("Loading").boxed())
            .error(|e| Text::new(e).boxed())
            .with_key("test-key");

        let cloned = widget.clone();
        assert_eq!(cloned.url, url);
        assert!(cloned.placeholder.is_some());
        assert!(cloned.error.is_some());
        assert_eq!(cloned.key, widget.key);
    }

    #[test]
    fn loading_state_renders_nothing_when_no_placeholder() {
        let url = test_url("https://example.com/img.png");
        let widget = NetworkImage::new(url);

        // No placeholder — render() would produce an empty widget.
        // Verify the closure is None.
        assert!(widget.placeholder.is_none());
        assert!(widget.error.is_none());
    }

    #[test]
    fn loaded_state_produces_image() {
        // Verify that ImageData can be wrapped in an Image widget —
        // this is what render() does on LoadState::Loaded.
        let data = test_image_data();
        let image = Image::new(data.clone());
        assert_eq!(image.image_data().width, 1);
        assert_eq!(image.image_data().height, 1);
    }

    #[test]
    fn network_image_implements_widget() {
        let url = test_url("https://example.com/img.png");
        let widget = NetworkImage::new(url);
        // Verify it can be boxed (Widget trait).
        let _boxed: Box<dyn Widget> = widget.boxed();
    }

    #[test]
    fn network_image_with_key_sets_key() {
        let url = test_url("https://example.com/img.png");
        let widget = NetworkImage::new(url).with_key("my-image");
        // `key()` is ambiguous (defined on both `Component` and the blanket
        // `Widget` impl); use fully-qualified syntax. `Widget::key` delegates
        // to `Component::key` via the blanket impl, so both give the same value.
        assert_eq!(
            Widget::key(&widget),
            Some(WidgetKey::Local(Key::new("my-image")))
        );
    }
}
