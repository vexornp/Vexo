use crate::core::{Physical, ScaleSource, Size};
use crate::frame_builder::FrameBuilder;
use crate::render::{RenderError, WgpuBackend};
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
        scale_source: &ScaleSource,
        viewport_physical: Size<Physical>,
    ) -> CombinedPreparedText {
        self.text_processor.collect_text(
            frame_builder,
            font_system,
            scale_source,
            viewport_physical,
        )
    }

    /// Stage 2: Execute the render on the backend.
    ///
    /// Prepares both the main-pass text (from `FrameBuilder::text_requests`)
    /// and per-save-layer-group text (from `FrameBuilder::save_layer_groups`),
    /// then issues the GPU render pass.
    ///
    /// Each save-layer group gets its own pooled `TextRenderer` and `Viewport`
    /// (see `WgpuBackend::prepare_group_text`). Group text uses surface-sized
    /// offscreen targets, so no coordinate translation is needed — the
    /// `viewport_physical` passed to `process_text_requests` is the full
    /// surface size, matching the main-pass behavior.
    pub fn execute_render(
        &mut self,
        backend: &mut WgpuBackend,
        frame_builder: &FrameBuilder,
        mut prepared_text: CombinedPreparedText,
        font_system: &mut glyphon::FontSystem,
    ) -> Result<(), RenderError> {
        backend.upload_geometry(frame_builder);

        // Prepare all main-pass text together (glyphon's prepare() replaces
        // vertex buffer contents each call, so per-clip-group prepare would
        // lose previous groups). Text clipping is handled by glyphon's
        // TextArea.bounds per-request.
        backend.prepare_text(font_system, prepared_text.as_text_areas());

        // Prepare per-save-layer-group text. Each group's TextAreas are built
        // from its own `text_requests` slice and prepared into a pooled
        // group TextRenderer. The PreparedText must outlive the prepare call
        // (its owned buffers back the TextAreas), so it is scoped per-group
        // inside the loop body.
        let scale_source = backend.scale_source();
        let viewport_physical = backend.config().size;
        for (i, group) in frame_builder.save_layer_groups().iter().enumerate() {
            if group.text_requests.is_empty() {
                continue;
            }
            let mut prepared = self.text_processor.process_text_requests(
                font_system,
                &group.text_requests,
                &scale_source,
                viewport_physical,
            );
            backend.prepare_group_text(i, font_system, prepared.as_text_areas())?;
        }

        let viewport_width = backend.width();
        let viewport_height = backend.height();

        backend.execute_render_pass(viewport_width, viewport_height)?;

        Ok(())
    }
}
