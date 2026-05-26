use crate::core::{Physical, Scale, Size};
use crate::render::{RenderError, WgpuBackend};
use crate::renderer::UiBatcher;
use crate::text_processor::{CombinedPreparedText, TextProcessor};

/// Rendering pipeline that orchestrates the stages of rendering.
///
/// This separates the render pipeline concerns from the main window state,
/// making each stage independently testable.
pub struct RenderPipeline {
    text_processor: TextProcessor,
}

impl Default for RenderPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderPipeline {
    pub fn new() -> Self {
        Self {
            text_processor: TextProcessor::new(),
        }
    }

    /// Stage 1: Collect text from the batcher and prepare for rendering.
    pub fn collect_text(
        &mut self,
        batcher: &mut UiBatcher,
        font_system: &mut glyphon::FontSystem,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> CombinedPreparedText {
        self.text_processor
            .collect_text(batcher, font_system, scale, viewport_physical)
    }

    /// Stage 2: Execute the render on the backend.
    pub fn execute_render(
        &mut self,
        backend: &mut WgpuBackend,
        batcher: &UiBatcher,
        mut prepared_text: CombinedPreparedText,
        font_system: &mut glyphon::FontSystem,
    ) -> Result<(), RenderError> {
        // Upload geometry data to GPU
        backend.upload_geometry(batcher);

        // Prepare text for rendering
        backend.prepare_text(font_system, prepared_text.as_text_areas());

        // Execute the render pass
        let instance_count = batcher.quad_instances.len();
        backend.execute_render_pass(instance_count)?;

        Ok(())
    }
}