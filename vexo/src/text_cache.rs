//! Text buffer cache to avoid recreating and shaping text every frame.
//!
//! This module provides caching for shaped text buffers, which is expensive
//! to recreate on every frame. The cache uses a generation-based eviction
//! strategy to remove stale entries.

use std::collections::HashMap;

use glyphon::{cosmic_text, Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

use crate::frame_builder::TextRequest;
use crate::layout::DEFAULT_LINE_HEIGHT_MULTIPLIER;

/// Maximum number of frames a cache entry can remain unused before eviction.
const MAX_STALE_FRAMES: u64 = 100;

/// Cache key for text buffers to avoid recreating/shaping every frame.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextCacheKey {
    content: String,
    font_size_bits: u32,
    color_bits: [u32; 4],
    font_family: Option<String>,
    max_width_bits: Option<u32>,
}

impl TextCacheKey {
    fn from_request(req: &TextRequest) -> Self {
        Self {
            content: req.content.clone(),
            font_size_bits: req.size.to_bits(),
            color_bits: [
                req.color.r.to_bits(),
                req.color.g.to_bits(),
                req.color.b.to_bits(),
                req.color.a.to_bits(),
            ],
            font_family: req.font_family.clone(),
            max_width_bits: req.max_width.map(|w| w.to_bits()),
        }
    }
}

/// Cached text buffer with its shaped content.
struct CachedTextBuffer {
    buffer: Buffer,
    /// Generation counter to detect stale entries
    generation: u64,
}

/// Text buffer cache to avoid recreating and shaping text every frame.
pub struct TextCache {
    cache: HashMap<TextCacheKey, CachedTextBuffer>,
    generation: u64,
}

impl TextCache {
    /// Create an empty text cache.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            generation: 0,
        }
    }

    /// Get a cached buffer or create and cache a new one.
    ///
    /// Returns a shaped text buffer ready for rendering.
    pub fn get_or_create(&mut self, font_system: &mut FontSystem, request: &TextRequest) -> Buffer {
        self.generation += 1;
        let current_gen = self.generation;
        let cache_key = TextCacheKey::from_request(request);

        // Try to get cached buffer
        if let Some(cached) = self.cache.get_mut(&cache_key) {
            cached.generation = current_gen;
            return cached.buffer.clone();
        }

        // Create and shape new buffer
        let mut buffer = Buffer::new(
            font_system,
            Metrics::new(request.size, request.size * DEFAULT_LINE_HEIGHT_MULTIPLIER),
        );

        // Set wrapping width before shaping so text wraps at the widget's width.
        // Add a small tolerance to avoid spurious wrapping caused by subpixel
        // rounding discrepancies between Taffy's layout width and glyphon's
        // measured natural width.
        if let Some(max_width) = request.max_width {
            buffer.set_size(font_system, Some(max_width + 0.5), None);
        }

        let color_rgba_u8: cosmic_text::Color = request.color.into();

        let mut attrs = Attrs::new().color(color_rgba_u8);
        if let Some(fam) = &request.font_family {
            attrs = attrs.family(Family::Name(fam));
        }
        buffer.set_text(
            font_system,
            &request.content,
            &attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(font_system, true);

        // Cache the buffer
        self.cache.insert(
            cache_key,
            CachedTextBuffer {
                buffer: buffer.clone(),
                generation: current_gen,
            },
        );

        buffer
    }

    /// Evict cache entries not used in recent frames.
    ///
    /// Entries unused for more than `MAX_STALE_FRAMES` are removed.
    pub fn evict_stale(&mut self) {
        let current_gen = self.generation;
        self.cache
            .retain(|_, cached| current_gen - cached.generation < MAX_STALE_FRAMES);
    }
}

impl Default for TextCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Color, Logical, Point};

    fn make_request(content: &str, family: Option<&str>) -> TextRequest {
        TextRequest {
            content: content.to_string(),
            position: Point::<Logical>::new(0.0, 0.0),
            size: 24.0,
            color: Color::BLACK,
            font_family: family.map(|s| s.to_string()),
            max_width: None,
            clip_bounds: None,
            rclip_snapshot: Vec::new(),
        }
    }

    #[test]
    fn cache_key_differs_by_font_family() {
        // Same content/size/color, different family → distinct keys.
        let r1 = make_request("\u{e001}", Some("iconfont"));
        let r2 = make_request("\u{e001}", Some("other"));
        let r3 = make_request("\u{e001}", None);
        let k1 = TextCacheKey::from_request(&r1);
        let k2 = TextCacheKey::from_request(&r2);
        let k3 = TextCacheKey::from_request(&r3);
        assert_ne!(k1, k2, "different family names must produce different keys");
        assert_ne!(k1, k3, "family set vs None must produce different keys");
        assert_ne!(k2, k3);
    }

    #[test]
    fn cache_key_equal_when_family_matches() {
        let r1 = make_request("\u{e001}", Some("iconfont"));
        let r2 = make_request("\u{e001}", Some("iconfont"));
        assert_eq!(
            TextCacheKey::from_request(&r1),
            TextCacheKey::from_request(&r2)
        );
    }

    #[test]
    fn cache_key_differs_by_content() {
        let r1 = make_request("\u{e001}", Some("iconfont"));
        let r2 = make_request("\u{e002}", Some("iconfont"));
        assert_ne!(
            TextCacheKey::from_request(&r1),
            TextCacheKey::from_request(&r2)
        );
    }

    #[test]
    fn get_or_create_returns_distinct_buffers_for_different_families() {
        // Two texts with the same codepoint but different families must not
        // share a cached buffer — otherwise the second one would render with
        // the first one's glyphs.
        let mut fs = crate::resource::new_font_system();
        let mut cache = TextCache::new();
        let r1 = make_request("A", Some("Roboto"));
        let r2 = make_request("A", None);
        let _b1 = cache.get_or_create(&mut fs, &r1);
        let _b2 = cache.get_or_create(&mut fs, &r2);
        // Both requests shared content/size/color but differed in family, so
        // the cache must hold two distinct entries.
        assert_eq!(cache.cache.len(), 2);
    }
}
