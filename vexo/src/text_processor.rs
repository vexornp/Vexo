use glyphon::{Buffer, FontSystem, TextArea};

use crate::core::{Bounds, Color, Physical, Scale, Size};
use crate::renderer::TextRequest;
use crate::text_cache::TextCache;

/// Processes text requests into TextArea instances for rendering.
pub struct TextProcessor {
    /// Text cache for buffer reuse.
    cache: TextCache,
}

/// Owned text data ready for rendering.
///
/// This struct owns the buffers that TextAreas borrow from,
/// ensuring the buffers live long enough for rendering.
pub struct PreparedText {
    /// Owned buffers for text areas (TextAreas borrow from these).
    buffers: Vec<Buffer>,
    /// Metadata for creating TextAreas.
    text_area_data: Vec<TextAreaData>,
}

/// Data needed to create a TextArea, stored without borrowing.
struct TextAreaData {
    left: f32,
    top: f32,
    scale: f32,
    /// Physical bounds for glyphon conversion
    bounds: Bounds<Physical>,
    default_color: Color,
}

impl PreparedText {
    /// Create TextArea instances for rendering.
    ///
    /// The returned TextAreas borrow from the owned buffers in this struct.
    /// Must be called immediately before rendering.
    pub fn as_text_areas(&mut self) -> Vec<TextArea<'_>> {
        self.buffers
            .iter_mut()
            .zip(self.text_area_data.iter())
            .map(|(buffer, data)| TextArea {
                buffer,
                left: data.left,
                top: data.top,
                scale: data.scale,
                bounds: data.bounds.to_glyphon_bounds(),
                default_color: data.default_color.into(),
                custom_glyphs: &[],
            })
            .collect()
    }
}

impl TextProcessor {
    /// Create a new text processor.
    pub fn new() -> Self {
        Self {
            cache: TextCache::new(),
        }
    }

    /// Create a TextAreaData from buffer and positioning info.
    fn create_text_area(
        buffer: Buffer,
        physical_pos: crate::core::Point<Physical>,
        scale: Scale,
        bounds: Bounds<Physical>,
        color: Color,
    ) -> (Buffer, TextAreaData) {
        let data = TextAreaData {
            left: physical_pos.x,
            top: physical_pos.y,
            scale: scale.factor(),
            bounds,
            default_color: color,
        };

        (buffer, data)
    }

    /// Process regular text requests into prepared text.
    pub fn process_text_requests(
        &mut self,
        font_system: &mut FontSystem,
        requests: Vec<TextRequest>,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> PreparedText {
        let mut buffers: Vec<Buffer> = Vec::new();
        let mut text_area_data: Vec<TextAreaData> = Vec::new();

        for req in requests {
            let buffer = self.cache.get_or_create(font_system, &req);

            // Convert logical position to physical
            let physical_pos = req.position.to_physical(scale);

            // Use clip bounds if set, otherwise use screen bounds
            let bounds = if let Some(clip) = &req.clip_bounds {
                // Clip bounds are in logical coordinates - convert to physical
                clip.to_physical(scale)
            } else {
                // No clipping - use full screen
                Bounds::<Physical>::from_xywh(
                    physical_pos.x,
                    physical_pos.y,
                    viewport_physical.width,
                    viewport_physical.height,
                )
            };

            let (buf, data) = Self::create_text_area(
                buffer,
                physical_pos,
                scale,
                bounds,
                req.color,
            );
            buffers.push(buf);
            text_area_data.push(data);
        }

        // Periodically evict stale cache entries
        self.cache.evict_stale();

        PreparedText {
            buffers,
            text_area_data,
        }
    }

    /// Collect text from the batcher and prepare it for rendering.
    ///
    /// Only processes text_requests (editor_requests have been removed;
    /// editor text is now handled via the retain-mode TextEditRenderObject
    /// which emits Caret commands instead).
    pub fn collect_text(
        &mut self,
        batcher: &mut crate::renderer::UiBatcher,
        font_system: &mut FontSystem,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> CombinedPreparedText {
        let text_requests = std::mem::take(&mut batcher.text_requests);
        let regular = self.process_text_requests(
            font_system,
            text_requests,
            scale,
            viewport_physical,
        );

        CombinedPreparedText { regular }
    }
}

impl Default for TextProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Owned text data ready for rendering.
///
/// Combines regular text into a single owned container
/// that can provide unified text areas for rendering.
pub struct CombinedPreparedText {
    regular: PreparedText,
}

impl CombinedPreparedText {
    /// Create TextArea instances for rendering.
    ///
    /// The returned TextAreas borrow from the owned buffers in this struct.
    /// Must be called immediately before rendering.
    pub fn as_text_areas(&mut self) -> Vec<TextArea<'_>> {
        self.regular.as_text_areas()
    }
}