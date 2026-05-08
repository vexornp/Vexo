//! Integration tests for StatefulWidget with ThreeTreePipeline.

#[cfg(test)]
mod tests {
    use crate::retain::{StatefulWidget, BuildContext, ThreeTreePipeline, Widget, Text};
    use crate::core::Size;
    use crate::layout::TaffyLayoutEngine;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[derive(Clone)]
    struct Counter {
        label: String,
    }

    struct CounterState {
        count: u32,
    }

    impl Default for CounterState {
        fn default() -> Self {
            Self { count: 0 }
        }
    }

    impl StatefulWidget for Counter {
        type State = CounterState;

        fn build(&self, state: &mut CounterState, _ctx: &mut BuildContext) -> Box<dyn Widget> {
            Box::new(Text::new(format!("{}: {}", self.label, state.count)))
        }
    }

    #[test]
    fn test_stateful_widget_in_pipeline() {
        let mut pipeline = ThreeTreePipeline::new();

        // Create a stateful widget
        let counter = Counter { label: "Count".to_string() };

        // Reconcile with the stateful widget
        pipeline.reconcile(Box::new(counter));

        // Should have elements
        assert!(!pipeline.element_registry().is_empty());
    }

    #[test]
    fn test_stateful_widget_state_persists_across_rebuild() {
        let mut pipeline = ThreeTreePipeline::new();

        // Initial reconcile
        let counter = Counter { label: "Count".to_string() };
        pipeline.reconcile(Box::new(counter));

        // Get the root element ID
        let root_id = pipeline.element_registry().root().unwrap();

        // Update with new widget (same type, different label)
        let counter_updated = Counter { label: "Updated".to_string() };
        pipeline.reconcile(Box::new(counter_updated));

        // Root element should be the same (updated, not remounted)
        assert_eq!(pipeline.element_registry().root(), Some(root_id));
    }

    #[test]
    fn test_stateful_widget_layout_and_paint() {
        let mut pipeline = ThreeTreePipeline::new();

        let counter = Counter { label: "Count".to_string() };
        pipeline.reconcile(Box::new(counter));

        // Layout
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Paint
        let commands = pipeline.paint();

        // Should have generated render commands from the child Text widget
        assert!(!commands.is_empty());
    }
}
