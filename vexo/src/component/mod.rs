//! Component system for reusable UI building blocks.
//!
//! Components provide:
//! - Local state that persists across view tree rebuilds
//! - Message isolation (each component has its own message type)
//! - Auto-scoped WidgetIds to prevent collisions

mod context;
mod storage;
mod widget;

pub use context::{ComponentContext, KeyPath};
pub use storage::ComponentStateStorage;
pub use widget::ComponentWidget;

use crate::widgets::Widget;

/// A reusable component with local state and message isolation.
pub trait Component: Sized + 'static {
    type Message: Clone + std::fmt::Debug + Send;
    type Output: Clone + std::fmt::Debug + Send;
    type State: Default;

    fn initial_state() -> Self::State {
        Self::State::default()
    }

    fn update(state: &mut Self::State, message: Self::Message);

    fn view(
        state: &Self::State,
        ctx: &mut ComponentContext<'_, Self::Message>,
    ) -> Box<dyn Widget<Self::Message>>;

    fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output>;
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Scale;
    use crate::core::WidgetId;
    use crate::widgets::{Column, Widget};
    use glyphon::FontSystem;

    #[derive(Clone, Debug)]
    enum TestMessage {
        Increment,
    }

    #[derive(Clone, Debug)]
    enum TestOutput {
        CountReached(u32),
    }

    #[derive(Default)]
    struct TestState {
        count: u32,
    }

    struct TestComponent;

    impl Component for TestComponent {
        type Message = TestMessage;
        type Output = TestOutput;
        type State = TestState;

        fn update(state: &mut Self::State, message: Self::Message) {
            match message {
                TestMessage::Increment => state.count += 1,
            }
        }

        fn view(
            _state: &Self::State,
            _ctx: &mut ComponentContext<'_, Self::Message>,
        ) -> Box<dyn Widget<Self::Message>> {
            Box::new(Column::new())
        }

        fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output> {
            match message {
                TestMessage::Increment if state.count >= 3 => {
                    Some(TestOutput::CountReached(state.count))
                }
                _ => None,
            }
        }
    }

    #[test]
    fn test_component_widget_creation() {
        let widget = ComponentWidget::<TestComponent>::new("test");
        assert_eq!(widget.storage_key(), "test");
        assert_eq!(widget.state().count, 0);
    }

    #[test]
    fn test_component_state_update() {
        let mut widget = ComponentWidget::<TestComponent>::new("test");
        TestComponent::update(&mut widget.state_mut(), TestMessage::Increment);
        assert_eq!(widget.state().count, 1);
        TestComponent::update(&mut widget.state_mut(), TestMessage::Increment);
        assert_eq!(widget.state().count, 2);
    }

    #[test]
    fn test_component_message_mapping() {
        let mut widget = ComponentWidget::<TestComponent>::new("test");
        for _ in 0..3 {
            TestComponent::update(&mut widget.state_mut(), TestMessage::Increment);
        }
        let output = TestComponent::map_message(TestMessage::Increment, &widget.state());
        assert!(matches!(output, Some(TestOutput::CountReached(3))));
    }

    #[test]
    fn test_component_context_widget_id() {
        let mut storage = ComponentStateStorage::new();
        let mut font_system = FontSystem::new();
        let ctx = ComponentContext::<TestMessage>::new(
            KeyPath::root().child("my_component"),
            &mut storage,
            &mut font_system,
            Scale::new(1.0),
        );

        let id = ctx.widget_id("my_widget");
        assert_eq!(id, WidgetId::from_key("my_component/my_widget"));
    }

    #[test]
    fn test_component_context_auto_id() {
        let mut storage = ComponentStateStorage::new();
        let mut font_system = FontSystem::new();
        let ctx = ComponentContext::<TestMessage>::new(
            KeyPath::root().child("comp"),
            &mut storage,
            &mut font_system,
            Scale::new(1.0),
        );

        let id1 = ctx.auto_id();
        let id2 = ctx.auto_id();
        assert_ne!(id1, id2);
        assert_eq!(id1, WidgetId::from_key("comp/auto_0"));
        assert_eq!(id2, WidgetId::from_key("comp/auto_1"));
    }
}
