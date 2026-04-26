//! Text buffer cache to avoid recreating and shaping text every frame.
//!
//! This module provides caching for shaped text buffers, which is expensive
//! to recreate on every frame. The cache uses a generation-based eviction
//! strategy to remove stale entries.

use std::collections::HashMap;

use glyphon::{cosmic_text, Attrs, Buffer, FontSystem, Metrics, Shaping};

use crate::renderer::TextRequest;

/// Maximum number of frames a cache entry can remain unused before eviction.
const MAX_STALE_FRAMES: u64 = 100;

/// Cache key for text buffers to avoid recreating/shaping every frame.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextCacheKey {
    content: String,
    font_size_bits: u32,
    color_bits: [u32; 4],
}

impl TextCacheKey {
    fn from_request(req: &TextRequest) -> Self {
        Self {
            content: req.content.clone(),
            font_size_bits: req.size.to_bits(),
            color_bits: [
                req.color[0].to_bits(),
                req.color[1].to_bits(),
                req.color[2].to_bits(),
                req.color[3].to_bits(),
            ],
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
    pub fn get_or_create(
        &mut self,
        font_system: &mut FontSystem,
        request: &TextRequest,
    ) -> Buffer {
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
            Metrics::new(request.size, request.size * 1.2),
        );

        let color_rgba_u8 = cosmic_text::Color::rgba(
            (request.color[0] * 255.0) as u8,
            (request.color[1] * 255.0) as u8,
            (request.color[2] * 255.0) as u8,
            (request.color[3] * 255.0) as u8,
        );

        buffer.set_text(
            font_system,
            &request.content,
            &Attrs::new().color(color_rgba_u8),
            Shaping::Advanced,
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
        self.cache.retain(|_, cached| current_gen - cached.generation < MAX_STALE_FRAMES);
    }
}

impl Default for TextCache {
    fn default() -> Self {
        Self::new()
    }
}
