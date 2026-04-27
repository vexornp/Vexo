//! Text processing for render pipeline.
//!
//! Handles conversion of text requests and editor requests into
//! glyphon TextArea instances ready for rendering.

use glyphon::{Buffer, FontSystem, TextArea};

use crate::core::{Bounds, Color, Physical, Scale, Size};
use crate::renderer::{EditorRequest, TextRequest};
use crate::text_cache::TextCache;
use crate::widgets::WidgetContext;

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
    ///
    /// This private helper extracts the common logic for creating
    /// text area data used in rendering.
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
            let bounds = if req.clip_bounds[2] > 0.0 {
                // Clip bounds are in logical coordinates - convert to physical
                let logical_bounds = Bounds::<crate::core::Logical>::from_xywh(
                    req.clip_bounds[0],
                    req.clip_bounds[1],
                    req.clip_bounds[2],
                    req.clip_bounds[3],
                );
                logical_bounds.to_physical(scale)
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

    /// Process editor requests into prepared text.
    pub fn process_editor_requests(
        &mut self,
        widget_context: &mut WidgetContext,
        requests: Vec<EditorRequest>,
        scale: Scale,
    ) -> PreparedText {
        let mut buffers: Vec<Buffer> = Vec::new();
        let mut text_area_data: Vec<TextAreaData> = Vec::new();

        for req in &requests {
            // Convert logical bounds to physical
            let physical_bounds = req.bounds.to_physical(scale);

            let editor_ref = widget_context.get_or_create_editor(&req.id, "initial_text");
            let editor = editor_ref.borrow();
            let mut buf = editor.buffer().clone();
            buf.shape_until_scroll(&mut widget_context.font_system, true);

            let (buffer, data) = Self::create_text_area(
                buf,
                crate::core::Point::new(physical_bounds.left, physical_bounds.top),
                scale,
                physical_bounds,
                req.color,
            );
            buffers.push(buffer);
            text_area_data.push(data);
        }

        PreparedText {
            buffers,
            text_area_data,
        }
    }
}

impl Default for TextProcessor {
    fn default() -> Self {
        Self::new()
    }
}
