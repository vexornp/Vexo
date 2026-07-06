//! Text measurement types for accurate intrinsic size calculation.
//!
//! This module provides types for measuring text dimensions using
//! glyphon/cosmic-text shaping, integrated with Taffy's measure callback.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

use crate::core::{Logical, Size};

// ============================================================================
// MEASURE CONTEXT TYPES
// ============================================================================

/// Context data for text measurement nodes.
#[derive(Debug, Clone)]
pub struct TextMeasureContext {
    /// The text content to measure.
    pub content: String,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Line height multiplier (default 1.2).
    pub line_height: f32,
    /// Optional font family name. When set, text is shaped against this
    /// family; when `None`, the framework default is used.
    pub font_family: Option<String>,
}

/// Context for nodes that need custom measurement.
#[derive(Debug, Clone)]
pub enum MeasureContext {
    /// Text node with measurement parameters.
    Text(TextMeasureContext),
}

// ============================================================================
// TEXT MEASURER
// ============================================================================

/// Measures text dimensions using glyphon/cosmic-text.
pub struct TextMeasurer<'a> {
    font_system: &'a mut FontSystem,
}

impl<'a> TextMeasurer<'a> {
    /// Create a new text measurer with the given font system.
    pub fn new(font_system: &'a mut FontSystem) -> Self {
        Self { font_system }
    }

    /// Measure text with given constraints.
    ///
    /// Returns the measured size in logical pixels.
    ///
    /// # Arguments
    /// - `content`: Text to measure
    /// - `font_size`: Font size in logical pixels
    /// - `line_height`: Line height multiplier
    /// - `font_family`: Optional font family name (e.g. an icon font family)
    /// - `available_width`: Available width for wrapping (None = infinite)
    /// - `available_height`: Available height (None = infinite)
    pub fn measure(
        &mut self,
        content: &str,
        font_size: f32,
        line_height: f32,
        font_family: Option<&str>,
        available_width: Option<f32>,
        available_height: Option<f32>,
    ) -> Size<Logical> {
        let metrics = Metrics::new(font_size, font_size * line_height);
        let mut buffer = Buffer::new(self.font_system, metrics);

        // Set size constraints for wrapping
        buffer.set_size(self.font_system, available_width, available_height);

        // Set and shape the text
        let mut attrs = Attrs::new();
        if let Some(fam) = font_family {
            attrs = attrs.family(Family::Name(fam));
        }
        buffer.set_text(self.font_system, content, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(self.font_system, true);

        // Calculate dimensions from layout runs
        let mut max_width = 0.0f32;
        let mut total_height = 0.0f32;

        for run in buffer.layout_runs() {
            max_width = max_width.max(run.line_w);
            total_height = total_height.max(run.line_top + run.line_height);
        }

        Size::new(max_width, total_height)
    }
}

// ============================================================================
// MEASURE CACHE
// ============================================================================

/// Cache key for measurement results.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeasureCacheKey {
    content_hash: u64,
    font_size_bits: u32,
    line_height_bits: u32,
    font_family_hash: u64,
    available_width_bits: Option<u32>,
    available_height_bits: Option<u32>,
}

impl MeasureCacheKey {
    /// Create a new cache key.
    pub fn new(
        content: &str,
        font_size: f32,
        line_height: f32,
        font_family: Option<&str>,
        available_width: Option<f32>,
        available_height: Option<f32>,
    ) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        let content_hash = hasher.finish();

        let font_family_hash = match font_family {
            Some(fam) => {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                fam.hash(&mut h);
                h.finish()
            }
            None => 0,
        };

        Self {
            content_hash,
            font_size_bits: font_size.to_bits(),
            line_height_bits: line_height.to_bits(),
            font_family_hash,
            available_width_bits: available_width.map(|f| f.to_bits()),
            available_height_bits: available_height.map(|f| f.to_bits()),
        }
    }
}

/// Cache for measurement results to avoid redundant text shaping.
pub struct MeasureCache {
    entries: HashMap<MeasureCacheKey, Size<Logical>>,
    max_entries: usize,
}

impl MeasureCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: 1000,
        }
    }

    /// Get a cached measurement.
    pub fn get(&self, key: &MeasureCacheKey) -> Option<Size<Logical>> {
        self.entries.get(key).copied()
    }

    /// Insert a measurement into the cache.
    pub fn insert(&mut self, key: MeasureCacheKey, size: Size<Logical>) {
        if self.entries.len() >= self.max_entries {
            // Simple eviction: clear all entries
            self.entries.clear();
        }
        self.entries.insert(key, size);
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for MeasureCache {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MEASURE FUNCTION
// ============================================================================

/// Measure a text node for Taffy's measure callback.
///
/// This function is called by Taffy during layout computation with
/// the actual available space, enabling accurate text sizing.
pub fn measure_text_node(
    known_dimensions: taffy::prelude::Size<Option<f32>>,
    available_space: taffy::prelude::Size<taffy::prelude::AvailableSpace>,
    node_context: Option<&mut MeasureContext>,
    font_system: &mut FontSystem,
    cache: &mut MeasureCache,
) -> taffy::prelude::Size<f32> {
    use taffy::prelude::{AvailableSpace, Size};

    // If both dimensions are explicitly set, use them (capped at available space
    // to prevent overflow when known_dimensions exceed the container).
    if let Size {
        width: Some(w),
        height: Some(h),
    } = known_dimensions
    {
        let capped_w = match available_space.width {
            AvailableSpace::Definite(avail) if w > avail => avail,
            _ => w,
        };
        let capped_h = match available_space.height {
            AvailableSpace::Definite(avail) if h > avail => avail,
            _ => h,
        };
        return Size {
            width: capped_w,
            height: capped_h,
        };
    }

    let Some(context) = node_context else {
        return Size::ZERO;
    };

    match context {
        MeasureContext::Text(text_ctx) => {
            // Handle empty text
            if text_ctx.content.is_empty() {
                return Size {
                    width: known_dimensions.width.unwrap_or(0.0),
                    height: known_dimensions
                        .height
                        .unwrap_or(text_ctx.font_size * text_ctx.line_height),
                };
            }

            // First, look up the natural (unwrapped) size in the cache.
            // This avoids re-shaping text every time the measure function
            // is called with a different available width.
            let natural_key = MeasureCacheKey::new(
                &text_ctx.content,
                text_ctx.font_size,
                text_ctx.line_height,
                text_ctx.font_family.as_deref(),
                None,
                None,
            );

            let mut measurer = TextMeasurer::new(font_system);

            let natural_size = if let Some(cached) = cache.get(&natural_key) {
                cached
            } else {
                let size = measurer.measure(
                    &text_ctx.content,
                    text_ctx.font_size,
                    text_ctx.line_height,
                    text_ctx.font_family.as_deref(),
                    None,
                    None,
                );
                cache.insert(natural_key, size);
                size
            };

            // Handle MinContent: measure with a tiny width to force wrapping
            // at every word boundary. The resulting widest line is the widest
            // word — the true min-content width. This allows flex layout to
            // shrink the item below its natural (unwrapped) width down to the
            // widest word, enabling text wrapping in constrained containers.
            //
            // We return the natural (single-line) height rather than the fully
            // wrapped height to avoid inflating the item's min-height, which
            // would prevent vertical shrinking in a Column.
            if matches!(available_space.width, AvailableSpace::MinContent) {
                let min_key = MeasureCacheKey::new(
                    &text_ctx.content,
                    text_ctx.font_size,
                    text_ctx.line_height,
                    text_ctx.font_family.as_deref(),
                    Some(1.0),
                    None,
                );
                let min_width = if let Some(cached) = cache.get(&min_key) {
                    cached.width
                } else {
                    let size = measurer.measure(
                        &text_ctx.content,
                        text_ctx.font_size,
                        text_ctx.line_height,
                        text_ctx.font_family.as_deref(),
                        Some(1.0),
                        None,
                    );
                    cache.insert(min_key, size);
                    size.width
                };
                return Size {
                    width: known_dimensions.width.unwrap_or(min_width),
                    height: known_dimensions.height.unwrap_or(natural_size.height),
                };
            }

            // Determine if we need to constrain and remeasure for wrapping
            let definite_width = match available_space.width {
                AvailableSpace::Definite(w) if w > 0.0 => Some(w),
                AvailableSpace::Definite(_) => Some(1.0), // Minimum width
                _ => None,
            };

            let measured_size = if let Some(max_w) = definite_width {
                if natural_size.width <= max_w {
                    // Natural width fits — no wrapping needed
                    natural_size
                } else {
                    // Natural width exceeds available space — remeasure with constraint
                    let available_height = match available_space.height {
                        AvailableSpace::Definite(h) => Some(h),
                        _ => None,
                    };
                    measurer.measure(
                        &text_ctx.content,
                        text_ctx.font_size,
                        text_ctx.line_height,
                        text_ctx.font_family.as_deref(),
                        Some(max_w),
                        available_height,
                    )
                }
            } else {
                // No width constraint (MaxContent/MinContent) — use natural size
                natural_size
            };

            // Cache with the effective constraint key
            let cache_key = MeasureCacheKey::new(
                &text_ctx.content,
                text_ctx.font_size,
                text_ctx.line_height,
                text_ctx.font_family.as_deref(),
                if measured_size.width < natural_size.width {
                    definite_width
                } else {
                    None
                },
                None,
            );
            cache.insert(cache_key, measured_size);

            // Resolve final dimensions: prefer known_dimensions (explicit size
            // from style or parent cross-axis), but cap at the available space
            // to prevent overflow. Taffy may pass known_dimensions.width that
            // exceeds available_space.width (e.g. parent's content-box width
            // before flex shrinking); uncapped, this causes the leaf to report
            // a size larger than its container, leading to text overflow.
            let raw_width = known_dimensions.width.unwrap_or(measured_size.width);
            let raw_height = known_dimensions.height.unwrap_or(measured_size.height);

            let width = match available_space.width {
                AvailableSpace::Definite(avail) if raw_width > avail => avail,
                _ => raw_width,
            };
            let height = match available_space.height {
                AvailableSpace::Definite(avail) if raw_height > avail => avail,
                _ => raw_height,
            };

            let result = Size { width, height };

            result
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_font_system() -> FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_measure_single_line() {
        let mut font_system = create_test_font_system();
        let mut measurer = TextMeasurer::new(&mut font_system);

        let size = measurer.measure("Hello", 24.0, 1.2, None, None, None);

        assert!(size.width > 0.0, "Width should be positive");
        assert!(size.height > 0.0, "Height should be positive");
        assert!(
            size.height < 24.0 * 1.5,
            "Height should be close to line height"
        );
    }

    #[test]
    fn test_measure_with_wrapping() {
        let mut font_system = create_test_font_system();
        let mut measurer = TextMeasurer::new(&mut font_system);

        let size1 = measurer.measure("Hello World", 24.0, 1.2, None, None, None);
        let size2 = measurer.measure("Hello World", 24.0, 1.2, None, Some(50.0), None);

        // Wrapped text should be narrower but taller
        assert!(size2.width < size1.width, "Wrapped text should be narrower");
        assert!(size2.height > size1.height, "Wrapped text should be taller");
    }

    #[test]
    fn test_measure_multiline() {
        let mut font_system = create_test_font_system();
        let mut measurer = TextMeasurer::new(&mut font_system);

        let size = measurer.measure("Line1\nLine2\nLine3", 24.0, 1.2, None, None, None);

        // Should have height for 3 lines
        assert!(
            size.height >= 24.0 * 1.2 * 3.0,
            "Height should accommodate 3 lines"
        );
    }

    #[test]
    fn test_measure_empty() {
        let mut font_system = create_test_font_system();
        let mut measurer = TextMeasurer::new(&mut font_system);

        let size = measurer.measure("", 24.0, 1.2, None, None, None);

        assert_eq!(size.width, 0.0, "Empty text should have zero width");
        assert!(
            size.height > 0.0,
            "Empty text should still have line height"
        );
    }

    #[test]
    fn test_cache_hit() {
        let mut cache = MeasureCache::new();

        let key = MeasureCacheKey::new("test", 24.0, 1.2, None, None, None);
        cache.insert(key.clone(), Size::new(100.0, 30.0));

        let result = cache.get(&key);
        assert_eq!(result, Some(Size::new(100.0, 30.0)));
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = MeasureCache::new();
        cache.max_entries = 2;

        cache.insert(
            MeasureCacheKey::new("a", 24.0, 1.2, None, None, None),
            Size::new(1.0, 1.0),
        );
        cache.insert(
            MeasureCacheKey::new("b", 24.0, 1.2, None, None, None),
            Size::new(2.0, 2.0),
        );
        cache.insert(
            MeasureCacheKey::new("c", 24.0, 1.2, None, None, None),
            Size::new(3.0, 3.0),
        );

        // Cache should have been cleared when exceeding max_entries
        assert_eq!(cache.entries.len(), 1);
    }
}
