//! Render pipeline orchestration.
//!
//! Coordinates text collection and GPU execution stages.

use glyphon::TextArea;

use crate::core::{Physical, Scale, Size};
use crate::render::{RenderError, WgpuBackend};
use crate::renderer::UiBatcher;
use crate::text_processor::PreparedText;
use crate::text_processor::TextProcessor;
use crate::widgets::WidgetContext;

/// Owned text data ready for rendering.
///
/// Combines regular text and editor text into a single owned container
/// that can provide unified text areas for rendering.
pub struct CombinedPreparedText {
    regular: PreparedText,
    editor: PreparedText,
}

impl CombinedPreparedText {
    /// Create TextArea instances for rendering.
    ///
    /// The returned TextAreas borrow from the owned buffers in this struct.
    /// Must be called immediately before rendering.
    pub fn as_text_areas(&mut self) -> Vec<TextArea<'_>> {
        let text_areas = self.regular.as_text_areas();
        let editor_areas = self.editor.as_text_areas();
        text_areas.into_iter().chain(editor_areas).collect()
    }
}

/// Orchestrates the render pipeline stages.
pub struct RenderPipeline {
    text_processor: TextProcessor,
}

impl RenderPipeline {
    /// Create a new render pipeline.
    pub fn new() -> Self {
        Self {
            text_processor: TextProcessor::new(),
        }
    }

    /// Stage 3: Collect text areas from batcher.
    ///
    /// Returns a CombinedPreparedText that owns all text buffers.
    /// Call `as_text_areas()` on the result to get TextArea instances.
    pub fn collect_text(
        &mut self,
        batcher: &mut UiBatcher,
        widget_context: &mut WidgetContext,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> CombinedPreparedText {
        // Process regular text requests
        let text_requests = std::mem::take(&mut batcher.text_requests);
        let regular = self.text_processor.process_text_requests(
            &mut widget_context.font_system,
            text_requests,
            scale,
            viewport_physical,
        );

        // Process editor requests
        let editor_requests = std::mem::take(&mut batcher.editor_requests);
        let editor = self.text_processor.process_editor_requests(
            widget_context,
            editor_requests,
            scale,
        );

        CombinedPreparedText { regular, editor }
    }

    /// Stage 4: Execute GPU render.
    pub fn execute_render(
        &mut self,
        backend: &mut WgpuBackend,
        batcher: &UiBatcher,
        mut prepared_text: CombinedPreparedText,
        widget_context: &mut WidgetContext,
    ) -> Result<(), RenderError> {
        // Get text areas from prepared text
        let text_areas = prepared_text.as_text_areas();

        // Upload geometry
        backend.upload_geometry(batcher);

        // Prepare text
        backend.prepare_text(&mut widget_context.font_system, text_areas);

        // Execute render pass
        let instance_count = batcher.quad_instances.len();
        backend.execute_render_pass(instance_count)?;

        Ok(())
    }
}

impl Default for RenderPipeline {
    fn default() -> Self {
        Self::new()
    }
}
