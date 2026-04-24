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
    ) -> Box<dyn Widget<Self::Output>>;

    fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output>;
}
