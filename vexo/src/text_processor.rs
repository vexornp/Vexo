use glyphon::{Buffer, FontSystem, TextArea};

use crate::core::{Bounds, Color, Logical, Physical, Scale, Size};
use crate::frame_builder::{ClipGroup, TextRequest};
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

    /// Check if there are any text areas.
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

/// Prepared text for a single clip group, ready for rendering.
pub struct PreparedClipGroup {
    /// The clip bounds for this group (logical coordinates).
    pub clip_bounds: Option<Bounds<Logical>>,
    /// Prepared text areas (owned buffers + metadata).
    pub prepared: PreparedText,
}

impl PreparedClipGroup {
    /// Convert to glyphon TextArea instances for rendering.
    pub fn as_text_areas(&mut self) -> Vec<TextArea<'_>> {
        self.prepared.as_text_areas()
    }

    /// Check if this group has any text to render.
    pub fn is_empty(&self) -> bool {
        self.prepared.is_empty()
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

    /// Process text requests into prepared text for a single clip group.
    fn process_text_requests(
        &mut self,
        font_system: &mut FontSystem,
        requests: &[TextRequest],
        scale: Scale,
        viewport_physical: Size<Physical>,
        clip_bounds: Option<Bounds<Logical>>,
    ) -> PreparedText {
        let mut buffers: Vec<Buffer> = Vec::new();
        let mut text_area_data: Vec<TextAreaData> = Vec::new();

        for req in requests {
            let buffer = self.cache.get_or_create(font_system, req);

            // Convert logical position to physical
            let physical_pos = req.position.to_physical(scale);

            // Use clip bounds if set, otherwise use screen bounds
            let bounds = if let Some(clip) = &clip_bounds {
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

    /// Process all clip groups and produce per-group prepared text.
    pub fn collect_clip_groups(
        &mut self,
        clip_groups: &[ClipGroup],
        font_system: &mut FontSystem,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> Vec<PreparedClipGroup> {
        let mut result = Vec::new();
        for group in clip_groups {
            if group.text_requests.is_empty() { continue; }
            let prepared = self.process_text_requests(
                font_system,
                &group.text_requests,
                scale,
                viewport_physical,
                group.clip_bounds,
            );
            result.push(PreparedClipGroup {
                clip_bounds: group.clip_bounds,
                prepared,
            });
        }
        result
    }

    /// Collect text from the frame_builder and prepare it for rendering.
    ///
    /// Only processes text_requests (editor requests have been removed;
    /// editor text is now handled via the retain-mode TextEditRenderObject
    /// which emits Caret commands instead).
    pub fn collect_text(
        &mut self,
        frame_builder: &mut crate::frame_builder::FrameBuilder,
        font_system: &mut FontSystem,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> CombinedPreparedText {
        let clip_groups = frame_builder.clip_groups();
        let groups = self.collect_clip_groups(clip_groups, font_system, scale, viewport_physical);
        CombinedPreparedText { groups }
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
    groups: Vec<PreparedClipGroup>,
}

impl CombinedPreparedText {
    /// Get the prepared clip groups mutably.
    pub fn groups_mut(&mut self) -> &mut Vec<PreparedClipGroup> {
        &mut self.groups
    }

    /// Get the prepared clip groups.
    pub fn groups(&mut self) -> &mut [PreparedClipGroup] {
        &mut self.groups
    }

    /// Create TextArea instances for all groups (flattened).
    ///
    /// The returned TextAreas borrow from the owned buffers in this struct.
    /// Must be called immediately before rendering.
    pub fn as_text_areas(&mut self) -> Vec<TextArea<'_>> {
        self.groups
            .iter_mut()
            .flat_map(|g| g.prepared.as_text_areas())
            .collect()
    }
}
