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
        // Upload geometry data to GPU
        backend.upload_geometry(frame_builder);

        // Prepare text for rendering
        backend.prepare_text(font_system, prepared_text.as_text_areas());

        // Execute the render pass
        let instance_count = frame_builder.quad_count();
        backend.execute_render_pass(instance_count)?;

        Ok(())
    }
}