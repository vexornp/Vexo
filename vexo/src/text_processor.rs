//! Text processing for render pipeline.
//!
//! Handles conversion of text requests and editor requests into
//! glyphon TextArea instances ready for rendering.

use glyphon::{cosmic_text, Buffer, FontSystem, TextArea, TextBounds};

use crate::core::{Physical, Scale, Size};
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
    bounds_left: i32,
    bounds_top: i32,
    bounds_right: i32,
    bounds_bottom: i32,
    default_color: cosmic_text::Color,
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
                bounds: TextBounds {
                    left: data.bounds_left,
                    top: data.bounds_top,
                    right: data.bounds_right,
                    bottom: data.bounds_bottom,
                },
                default_color: data.default_color,
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
        bounds_left: i32,
        bounds_top: i32,
        bounds_right: i32,
        bounds_bottom: i32,
        color: [f32; 4],
    ) -> (Buffer, TextAreaData) {
        let default_color = cosmic_text::Color::rgba(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
            (color[3] * 255.0) as u8,
        );

        let data = TextAreaData {
            left: physical_pos.x,
            top: physical_pos.y,
            scale: scale.factor(),
            bounds_left,
            bounds_top,
            bounds_right,
            bounds_bottom,
            default_color,
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
            let (bounds_left, bounds_top, bounds_right, bounds_bottom) =
                if req.clip_bounds[2] > 0.0 {
                    // Clip bounds are in logical coordinates - convert to physical
                    let clip_left = req.clip_bounds[0] * scale.factor();
                    let clip_top = req.clip_bounds[1] * scale.factor();
                    let clip_right = (req.clip_bounds[0] + req.clip_bounds[2]) * scale.factor();
                    let clip_bottom = (req.clip_bounds[1] + req.clip_bounds[3]) * scale.factor();
                    (
                        clip_left.floor() as i32,
                        clip_top.floor() as i32,
                        clip_right.ceil() as i32,
                        clip_bottom.ceil() as i32,
                    )
                } else {
                    // No clipping - use full screen
                    (
                        physical_pos.x.floor() as i32,
                        physical_pos.y.floor() as i32,
                        viewport_physical.width_u32() as i32,
                        viewport_physical.height_u32() as i32,
                    )
                };

            let (buf, data) = Self::create_text_area(
                buffer,
                physical_pos,
                scale,
                bounds_left,
                bounds_top,
                bounds_right,
                bounds_bottom,
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
            let physical_rect = req.bounds.to_physical(scale);

            let bounds_left: i32 = physical_rect.origin.x.floor() as i32;
            let bounds_top: i32 = physical_rect.origin.y.floor() as i32;
            let bounds_right: i32 =
                (physical_rect.origin.x + physical_rect.size.width).ceil() as i32;
            let bounds_bottom: i32 =
                (physical_rect.origin.y + physical_rect.size.height).ceil() as i32;

            let editor_ref = widget_context.get_or_create_editor(&req.id, "initial_text");
            let editor = editor_ref.borrow();
            let mut buf = editor.buffer().clone();
            buf.shape_until_scroll(&mut widget_context.font_system, true);

            let (buffer, data) = Self::create_text_area(
                buf,
                physical_rect.origin,
                scale,
                bounds_left,
                bounds_top,
                bounds_right,
                bounds_bottom,
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
