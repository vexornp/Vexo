use crate::core::{Physical, Scale, Size};
use crate::render::{RenderError, WgpuBackend};
use crate::frame_builder::FrameBuilder;
use crate::text_processor::{CombinedPreparedText, TextProcessor};

/// Text preparation and GPU submission pipeline.
///
/// Wraps glyphon text processing and orchestrates the final GPU
/// render pass (geometry upload + text render + draw call).
pub struct TextPipeline {
    text_processor: TextProcessor,
}

impl Default for TextPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl TextPipeline {
    pub fn new() -> Self {
        Self {
            text_processor: TextProcessor::new(),
        }
    }

    /// Stage 1: Collect text from the frame_builder and prepare for rendering.
    pub fn collect_text(
        &mut self,
        frame_builder: &mut FrameBuilder,
        font_system: &mut glyphon::FontSystem,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> CombinedPreparedText {
        self.text_processor
            .collect_text(frame_builder, font_system, scale, viewport_physical)
    }

    /// Stage 2: Execute the render on the backend.
    pub fn execute_render(
        &mut self,
        backend: &mut WgpuBackend,
        frame_builder: &FrameBuilder,
        mut prepared_text: CombinedPreparedText,
        font_system: &mut glyphon::FontSystem,
    ) -> Result<(), RenderError> {
        // Upload geometry data to GPU (flattened instances + draw ranges)
        backend.upload_geometry(frame_builder);

        // Upload image geometry data to GPU
        backend.upload_image_geometry(frame_builder);

        let flattened = frame_builder.flatten_quads();
        let clip_groups = frame_builder.clip_groups();

        // Get image draw ranges
        let (_, image_draw_ranges) = frame_builder.flatten_image_requests();

        // Prepare all text together (glyphon's prepare() replaces vertex buffer
        // contents each call, so per-clip-group prepare would lose previous groups).
        // Text clipping is handled by glyphon's TextArea.bounds per-request.
        backend.prepare_text(font_system, prepared_text.as_text_areas());

        // Execute the render pass with per-clip-group scissor rects for quads,
        // then render all text with full-viewport scissor.
        let scale_factor = backend.current_config()
            .map(|c| c.scale_factor())
            .unwrap_or(1.0);
        let viewport_width = backend.width();
        let viewport_height = backend.height();

        backend.execute_render_pass(
            clip_groups,
            &flattened.draw_ranges,
            &image_draw_ranges,
            scale_factor,
            viewport_width,
            viewport_height,
        )?;

        Ok(())
    }
}
