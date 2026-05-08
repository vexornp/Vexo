//! StatefulWidget trait for widgets with persistent mutable state.

use super::id::ElementId;
use super::state::StateStorage;
use super::dirty::DirtyTracking;
use super::render_object::RenderObjectRegistry;
use super::build_owner::BuildOwner;
use super::widgets::Widget;

/// Context provided to StatefulWidget::build().
pub struct BuildContext<'a> {
    /// The element ID for this stateful element.
    pub element_id: ElementId,

    /// State storage for accessing element state.
    pub state_storage: &'a mut StateStorage,

    /// Dirty tracking for marking layout/paint dirty.
    pub dirty: &'a mut DirtyTracking,

    /// Render object registry.
    pub render_objects: &'a mut RenderObjectRegistry,

    /// Build owner for scheduling rebuilds.
    pub build_owner: &'a mut BuildOwner,
}

impl<'a> BuildContext<'a> {
    /// Request a rebuild of this element.
    ///
    /// The element will be rebuilt during the next frame.
    pub fn request_rebuild(&mut self) {
        self.build_owner.mark_needs_build(self.element_id);
    }

    /// Mark the element's render object as needing layout.
    pub fn mark_needs_layout(&mut self, render_object_id: super::id::RenderObjectId) {
        self.dirty.mark_needs_layout(render_object_id);
    }

    /// Mark the element's render object as needing paint.
    pub fn mark_needs_paint(&mut self, render_object_id: super::id::RenderObjectId) {
        self.dirty.mark_needs_paint(render_object_id);
    }
}

/// Trait for widgets that have persistent mutable state.
///
/// StatefulWidget is the Vexo equivalent of Flutter's StatefulWidget.
/// The state persists across widget tree rebuilds, allowing the widget
/// to maintain mutable data that survives reconciliation.
///
/// # Example
///
/// ```ignore
/// #[derive(Clone)]
/// struct Counter {
///     label: String,
/// }
///
/// struct CounterState {
///     count: u32,
/// }
///
/// impl Default for CounterState {
///     fn default() -> Self {
///         Self { count: 0 }
///     }
/// }
///
/// impl StatefulWidget for Counter {
///     type State = CounterState;
///
///     fn build(&self, state: &mut CounterState, ctx: &mut BuildContext) -> Box<dyn Widget> {
///         Column::new()
///             .push(Text::new(format!("{}: {}", self.label, state.count)))
///             .push(Button::new("Increment", || {
///                 state.count += 1;
///                 ctx.request_rebuild();
///             }))
///             .boxed()
///     }
/// }
/// ```
pub trait StatefulWidget: Sized + 'static {
    /// The mutable state type that persists across rebuilds.
    ///
    /// Must implement Default for initialization.
    type State: Default;

    /// Build the widget tree using current state.
    ///
    /// Called during mount and update. The state is passed mutably
    /// so the widget can modify it. Call `ctx.request_rebuild()`
    /// after modifying state to trigger a rebuild.
    fn build(&self, state: &mut Self::State, ctx: &mut BuildContext) -> Box<dyn Widget>;
}
