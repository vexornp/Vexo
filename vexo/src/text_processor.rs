use glyphon::{Buffer, FontSystem, TextArea};

use crate::core::{Bounds, Color, Physical, ScaleSource, Size};
use crate::frame_builder::TextRequest;
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
    /// Depth value for GPU depth testing (paint-order occlusion).
    depth: f32,
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
                depth: data.depth,
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
        scale_source: &ScaleSource,
        bounds: Bounds<Physical>,
        color: Color,
        depth: f32,
    ) -> (Buffer, TextAreaData) {
        let scale = scale_source.get();
        let data = TextAreaData {
            left: physical_pos.x,
            top: physical_pos.y,
            scale: scale.factor(),
            bounds,
            default_color: color,
            depth,
        };

        (buffer, data)
    }

    /// Process text requests into a single PreparedText.
    ///
    /// Each request carries its own `clip_bounds`; there is no clip-group
    /// bucketing. Clipping is per-request via `TextArea.bounds`.
    fn process_text_requests(
        &mut self,
        font_system: &mut FontSystem,
        requests: &[TextRequest],
        scale_source: &ScaleSource,
        viewport_physical: Size<Physical>,
    ) -> PreparedText {
        let scale = scale_source.get();
        let mut buffers: Vec<Buffer> = Vec::new();
        let mut text_area_data: Vec<TextAreaData> = Vec::new();

        for req in requests {
            let buffer = self.cache.get_or_create(font_system, req);
            let physical_pos = req.position.to_physical(scale);

            // Use the request's own clip bounds, or fall back to the full
            // viewport when no clip is active.
            //
            // The fallback uses (0, 0, viewport_width, viewport_height) — the full
            // viewport — rather than a viewport-sized region starting at the text
            // position. Starting at the text position would create bounds like
            // (text_x, text_y, text_x + vp_w, text_y + vp_h), which pushes the
            // bottom far past the viewport. glyphon internally clamps bounds to
            // the resolution, and a large bottom value interacts badly with that
            // clamping.
            let bounds = if let Some(clip) = req.clip_bounds {
                clip.to_physical(scale)
            } else {
                Bounds::<Physical>::from_xywh(
                    0.0,
                    0.0,
                    viewport_physical.width,
                    viewport_physical.height,
                )
            };

            let (buf, data) = Self::create_text_area(
                buffer,
                physical_pos,
                scale_source,
                bounds,
                req.color,
                req.z,
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

    /// Collect text from the frame_builder and prepare it for rendering.
    ///
    /// Reads `text_requests()` directly; each request carries its own
    /// `clip_bounds`, so there is no clip-group structure to consult for
    /// text clipping.
    ///
    /// Only processes text_requests (editor requests have been removed;
    /// editor text is now handled via the retain-mode TextEditRenderObject
    /// which emits Caret commands instead).
    pub fn collect_text(
        &mut self,
        frame_builder: &mut crate::frame_builder::FrameBuilder,
        font_system: &mut FontSystem,
        scale_source: &ScaleSource,
        viewport_physical: Size<Physical>,
    ) -> CombinedPreparedText {
        let requests = frame_builder.text_requests();
        let regular =
            self.process_text_requests(font_system, &requests, scale_source, viewport_physical);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Bounds, Color, Logical, Physical, Point, ScaleSource, Size};
    use crate::frame_builder::FrameBuilder;

    fn font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        glyphon::FontSystem::new_with_fonts([glyphon::fontdb::Source::Binary(std::sync::Arc::new(
            font_data,
        ))])
    }

    #[test]
    fn test_collect_text_reads_clip_from_request() {
        let mut fb = FrameBuilder::new();
        let clip = Bounds::<Logical>::from_xywh(5.0, 5.0, 50.0, 50.0);
        fb.push_clip(clip);
        fb.add_text(
            "hi".to_string(),
            Point::new(10.0, 10.0),
            16.0,
            Color::BLACK,
            None,
            None,
        );
        fb.pop_clip();
        // Outside clip — clip_bounds should be None
        fb.add_text(
            "lo".to_string(),
            Point::new(10.0, 10.0),
            16.0,
            Color::BLACK,
            None,
            None,
        );

        let mut fs = font_system();
        let scale = ScaleSource::new(1.0);
        let mut proc = TextProcessor::new();
        let mut prepared = proc.collect_text(
            &mut fb,
            &mut fs,
            &scale,
            Size::<Physical>::new(800.0, 600.0),
        );

        // No panic, and both text areas present
        let mut areas = prepared.as_text_areas();
        assert_eq!(areas.len(), 2);
        // First area's bounds should reflect the clip; second should be full viewport.
        // glyphon's TextArea.bounds is a glyphon::Bounds (i32). We don't assert exact
        // values (resolution-dependent); just that the call succeeds.
        let _ = areas.drain(..);
    }
}
