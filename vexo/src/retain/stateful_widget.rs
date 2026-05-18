//! StatefulWidget trait for widgets with persistent mutable state.

use std::any::Any;
use std::sync::Arc;

use super::id::ElementKey;
use super::id::RenderObjectKey;
use super::dirty::DirtyTracking;
use super::render_object::{RenderObject, RenderObjectRegistry, LayoutContext, LayoutResult, PaintContext, HitTestContext};
use super::build_owner::BuildOwner;
use super::element::Element;
use super::element_context::ElementContext;
use super::key::WidgetKey;
use super::widgets::Widget;
use crate::core::Logical;
use crate::render::RenderCommand;

// ============================================================================
// STATE TRAIT
// ============================================================================

/// Trait for state objects that belong to StatefulElements.
///
/// This is the Vexo equivalent of Flutter's `State` class.
/// Provides lifecycle hooks and a mechanism for wiring up reactive
/// fields (like `StatefulMutable`) to automatically mark the element
/// dirty when state changes.
///
/// # Implementing State
///
/// Every `StatefulWidget::State` type must implement both `State` and `Default`.
/// For simple state types with no reactive fields, use the `SimpleState` wrapper
/// or implement `State` with an empty body (all methods have default no-op impls).
///
/// For state types containing `StatefulMutable` fields, implement `set_dirty_callback()`
/// to wire them up:
///
/// ```ignore
/// struct MyState {
///     count: StatefulMutable<u32>,
/// }
///
/// impl State for MyState {
///     fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
///         self.count.set_dirty_callback(callback);
///     }
/// }
/// ```
pub trait State: 'static {
    /// Called once when the StatefulElement is first mounted.
    ///
    /// Equivalent to Flutter's `initState()`. Use this for one-time
    /// initialization that requires access to the StateContext.
    fn init(&mut self, _ctx: &mut StateContext) {}

    /// Called when the StatefulElement is removed from the tree.
    ///
    /// Equivalent to Flutter's `dispose()`. Use this for cleanup
    /// like canceling timers or releasing resources.
    fn dispose(&mut self) {}

    /// Wire up dirty callbacks for any `StatefulMutable` fields.
    ///
    /// Override this if your state contains `StatefulMutable` fields.
    /// The callback marks the owning element dirty in the BuildOwner,
    /// triggering a rebuild on the next frame.
    ///
    /// The default implementation does nothing (no reactive fields).
    fn set_dirty_callback(&mut self, _callback: Arc<dyn Fn() + Send + Sync>) {}
}

/// Wrapper for simple state types that don't need reactive fields.
///
/// Use this when your `StatefulWidget::State` is a plain `Default` type
/// with no `StatefulMutable` fields. It implements both `State` and `Default`
/// with no-op lifecycle hooks.
///
/// # Example
///
/// ```ignore
/// struct MyWidget;
/// impl StatefulWidget for MyWidget {
///     type State = SimpleState<MyPlainState>;
///     // ...
/// }
/// ```
pub struct SimpleState<T: Default + 'static>(pub T);

impl<T: Default + 'static> Default for SimpleState<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

impl<T: Default + 'static> State for SimpleState<T> {}

impl<T: Default + 'static> std::ops::Deref for SimpleState<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Default + 'static> std::ops::DerefMut for SimpleState<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// ============================================================================
// STATE CONTEXT
// ============================================================================

/// Context provided to `State::init()` and available during state mutations.
///
/// This is the Vexo equivalent of Flutter's `State` class methods.
/// The key method is `setState()`, which mutates state and marks the
/// element dirty for rebuild.
pub struct StateContext<'a> {
    /// The element ID of the owning StatefulElement.
    element_id: ElementKey,

    /// Build owner for dirty marking.
    ///
    /// Uses a shared reference because `mark_needs_build()` takes `&self`
    /// via RefCell interior mutability.
    build_owner: &'a BuildOwner,
}

impl<'a> StateContext<'a> {
    /// Create a new StateContext. Only called by StatefulElement.
    fn new(element_id: ElementKey, build_owner: &'a BuildOwner) -> Self {
        Self {
            element_id,
            build_owner,
        }
    }

    /// Flutter-style setState: apply mutation, then mark dirty.
    ///
    /// The closure should contain all state mutations. After the closure
    /// runs, the element is marked dirty and will rebuild on the next frame.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.setState(state, |s| {
    ///     s.count += 1;
    /// });
    /// ```
    #[allow(non_snake_case)]
    pub fn setState<S, F>(&mut self, state: &mut S, callback: F)
    where
        F: FnOnce(&mut S),
    {
        callback(state); // Apply mutation immediately
        self.build_owner.mark_needs_build(self.element_id);
    }

    /// Mark this element as needing rebuild without mutating state.
    ///
    /// Useful when an external event requires a rebuild but no state
    /// mutation is needed (e.g., a reactive signal changed).
    pub fn request_rebuild(&self) {
        self.build_owner.mark_needs_build(self.element_id);
    }

    /// Get the element ID of the owning StatefulElement.
    pub fn element_id(&self) -> ElementKey {
        self.element_id
    }
}

// ============================================================================
// BUILD CONTEXT
// ============================================================================

/// Context provided to StatefulWidget::build().
pub struct BuildContext<'a> {
    /// The element ID for this stateful element.
    pub element_id: ElementKey,

    /// Dirty tracking for marking layout/paint dirty.
    pub dirty: &'a mut DirtyTracking,

    /// Render object registry.
    pub render_objects: &'a mut RenderObjectRegistry,

    /// Build owner for scheduling rebuilds.
    /// Uses shared reference because mark_needs_build() takes &self
    /// via interior mutability (RefCell).
    pub build_owner: &'a BuildOwner,
}

impl<'a> BuildContext<'a> {
    /// Request a rebuild of this element.
    ///
    /// The element will be rebuilt during the next frame.
    pub fn request_rebuild(&mut self) {
        self.build_owner.mark_needs_build(self.element_id);
    }

    /// Mark the element's render object as needing layout.
    pub fn mark_needs_layout(&mut self, render_object_id: super::id::RenderObjectKey) {
        self.dirty.mark_needs_layout(render_object_id);
    }

    /// Mark the element's render object as needing paint.
    pub fn mark_needs_paint(&mut self, render_object_id: super::id::RenderObjectKey) {
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
    /// Must implement `State + Default` for initialization and lifecycle.
    /// The blanket `impl<T: Default + 'static> State for T {}` ensures
    /// backward compatibility with plain `Default` state types.
    type State: State + Default;

    /// Build the widget tree using current state.
    ///
    /// Called during mount, update, and state-driven rebuilds.
    /// The state is passed mutably so the widget can modify it.
    fn build(&self, state: &mut Self::State, ctx: &mut BuildContext) -> Box<dyn Widget>;
}

/// Element for StatefulWidget widgets.
///
/// StatefulElement wraps a StatefulWidget and:
/// - Stores the widget configuration
/// - Manages state in StateStorage (keyed by element ID)
/// - Builds a child widget tree on mount and update
/// - Delegates rendering to the child element
pub struct StatefulElement<W: StatefulWidget> {
    /// The widget configuration.
    widget: W,

    /// The element ID (set during mount).
    id: Option<ElementKey>,

    /// The widget key (if any).
    key: Option<WidgetKey>,

    /// The render object ID (delegated from child).
    render_object_id: Option<RenderObjectKey>,
}

impl<W: StatefulWidget> StatefulElement<W> {
    /// Create a new StatefulElement from a widget.
    pub fn new(widget: W) -> Self {
        Self {
            widget,
            id: None,
            key: None,
            render_object_id: None,
        }
    }
}

impl<W: StatefulWidget + Clone> StatefulElement<W> {
    /// Build the child widget using the element's state.
    fn build_child_widget(
        &self,
        element_id: ElementKey,
        state: &mut W::State,
        dirty: &mut DirtyTracking,
        render_objects: &mut RenderObjectRegistry,
        build_owner: &BuildOwner,
    ) -> Box<dyn Widget> {
        // Create BuildContext and build
        let mut build_ctx = BuildContext {
            element_id,
            dirty,
            render_objects,
            build_owner,
        };
        self.widget.build(state, &mut build_ctx)
    }
}

impl<W: StatefulWidget + Clone> Element for StatefulElement<W> {
    fn mount(&mut self, context: &mut ElementContext) {
        // Store the element ID
        self.id = Some(context.element_id);

        // Register global key if present
        if let Some(WidgetKey::Global(key)) = &self.key {
            let _ = context.register_global_key(key.clone(), context.element_id);
        }

        let element_id = context.element_id;

        // Initialize state with Default
        let mut state = W::State::default();

        // Wire up dirty callback using channel sender.
        let tx = context.dirty_sender.clone();
        let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.send(element_id);
        });
        state.set_dirty_callback(dirty_callback);

        // Call State::init() lifecycle hook
        let mut state_ctx = StateContext::new(element_id, context.build_owner);
        state.init(&mut state_ctx);

        // Store state in StateStorage
        context.insert_state(element_id, state);

        // Build the child widget tree using BuildContext
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.dirty,
                context.render_objects,
                context.build_owner,
            )
        };

        // Mount the child element tree via child_ops
        context.inflate_child(None, child_widget);
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast to the concrete widget type
        if let Ok(widget) = new_widget.downcast::<W>() {
            self.widget = *widget;
        }

        let element_id = context.element_id;

        // Build the child widget tree using BuildContext
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.dirty,
                context.render_objects,
                context.build_owner,
            )
        };

        // Reconcile child via child_ops
        let old_child = context.children().first().copied();
        match old_child {
            Some(old_child_key) => {
                // Update existing child
                context.update_child(old_child_key, child_widget);
            }
            None => {
                // Inflate new child
                context.inflate_child(None, child_widget);
            }
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Call State::dispose() lifecycle hook before removing state
        if let Some(id) = self.id {
            if let Some(state) = context.state.get_mut::<W::State>(id) {
                state.dispose();
            }
        }

        // Unregister global key if present
        if let Some(WidgetKey::Global(_)) = &self.key {
            if let Some(id) = self.id {
                context.unregister_global_key(id);
            }
        }

        // Unmount child element via child_ops
        if let Some(child_key) = context.children().first().copied() {
            context.unmount_child(child_key);
        }

        // Remove state from storage
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn render_object(&self) -> Option<RenderObjectKey> {
        self.render_object_id
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<W>().is_some()
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, _context: &mut ElementContext) {
        // StatefulElement delegates its render_object_id to its child's render object
        self.render_object_id = child_ro;
    }

    fn rebuild_from_state(&mut self, context: &mut ElementContext) {
        // Rebuild using the CURRENT widget + updated state.
        // This is called by perform_rebuilds() when setState() or
        // StatefulMutable::set() marked this element dirty.

        let element_id = self.id.unwrap_or(context.element_id);

        // Build the child widget tree using BuildContext
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.dirty,
                context.render_objects,
                context.build_owner,
            )
        };

        // Reconcile child via child_ops
        let old_child = context.children().first().copied();
        match old_child {
            Some(old_child_key) => {
                // Update existing child
                context.update_child(old_child_key, child_widget);
            }
            None => {
                // Inflate new child
                context.inflate_child(None, child_widget);
            }
        }
    }
}

// ============================================================================
// EMPTY RENDER OBJECT
// ============================================================================

/// Empty render object for StatefulElement.
///
/// StatefulElement doesn't render itself - it delegates to its child.
/// This render object exists only to satisfy the Widget trait.
pub struct EmptyRenderObject;

impl RenderObject for EmptyRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _children: &[crate::layout::LayoutNodeKey]) -> LayoutResult {
        let node = ctx.engine().create_leaf(&crate::layout::Layout::default());
        LayoutResult {
            node,
            size: crate::core::Size::new(0.0, 0.0),
        }
    }

    fn apply_layout(&mut self, _ctx: &LayoutContext) {}

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        Vec::new()
    }

    fn hit_test(&self, _position: crate::core::Point<Logical>, _ctx: &HitTestContext) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// WIDGET TRAIT IMPLEMENTATION FOR STATEFULWIDGET
// ============================================================================

/// Blanket Widget implementation for StatefulWidget types.
///
/// This allows StatefulWidget implementations to be used anywhere
/// a Widget is expected.
impl<W: StatefulWidget + Clone + 'static> Widget for W {
    fn key(&self) -> Option<WidgetKey> {
        None // StatefulWidget widgets can override this if needed
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(StatefulElement::new(self.clone()))
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(EmptyRenderObject)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    fn make_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    use super::*;
    use crate::retain::{DirtyTracking, StateStorage, RenderObjectRegistry, ElementRegistry, ElementContext, Text, BuildOwner, ChildOps};

    #[derive(Clone)]
    struct TestCounter {
        label: String,
    }

    struct TestCounterState {
        count: u32,
    }

    impl Default for TestCounterState {
        fn default() -> Self {
            Self { count: 0 }
        }
    }

    impl State for TestCounterState {}

    impl StatefulWidget for TestCounter {
        type State = TestCounterState;

        fn build(&self, state: &mut TestCounterState, _ctx: &mut BuildContext) -> Box<dyn Widget> {
            // Return a simple text widget showing the count
            Box::new(Text::new(format!("{}: {}", self.label, state.count)))
        }
    }

    fn create_test_context() -> (
        ElementKey,
        StateStorage,
        DirtyTracking,
        RenderObjectRegistry,
        ElementRegistry,
        BuildOwner,
        std::sync::mpsc::Sender<ElementKey>,
        ChildOps,
    ) {
        let (dirty_sender, _) = std::sync::mpsc::channel();
        (
            make_element_key(),
            StateStorage::new(),
            DirtyTracking::new(),
            RenderObjectRegistry::new(),
            ElementRegistry::new(),
            BuildOwner::new(),
            dirty_sender,
            ChildOps::new(),
        )
    }

    #[test]
    fn test_stateful_element_mount_creates_state() {
        let widget = TestCounter { label: "Count".to_string() };
        let element = StatefulElement::new(widget);

        let (element_id, mut state, mut dirty, mut render_objects, _element_registry, build_owner, dirty_sender, mut child_ops) = create_test_context();

        // Mount the element
        let mut ctx = ElementContext::new(
            element_id,
            None,
            Vec::new(),
            &mut state,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &dirty_sender,
            &mut child_ops,
        );

        let mut element = element;
        Element::mount(&mut element, &mut ctx);

        // State should be created with default value
        assert!(state.get::<TestCounterState>(element_id).is_some());
        assert_eq!(state.get::<TestCounterState>(element_id).unwrap().count, 0);
    }

    #[test]
    fn test_stateful_element_update_preserves_state() {
        let widget = TestCounter { label: "Count".to_string() };
        let mut element = StatefulElement::new(widget);

        let (element_id, mut state, mut dirty, mut render_objects, _element_registry, build_owner, dirty_sender, mut child_ops) = create_test_context();

        // Mount
        {
            let mut ctx = ElementContext::new(
                element_id,
                None,
                Vec::new(),
                &mut state,
                &mut dirty,
                &mut render_objects,
                &build_owner,
                &dirty_sender,
                &mut child_ops,
            );
            Element::mount(&mut element, &mut ctx);
        }

        // Modify state
        state.get_mut::<TestCounterState>(element_id).unwrap().count = 5;

        // Update with new widget
        let new_widget = TestCounter { label: "Updated".to_string() };
        {
            let mut ctx = ElementContext::new(
                element_id,
                None,
                Vec::new(),
                &mut state,
                &mut dirty,
                &mut render_objects,
                &build_owner,
                &dirty_sender,
                &mut child_ops,
            );
            Element::update(&mut element, Box::new(new_widget), &mut ctx);
        }

        // State should be preserved
        assert_eq!(state.get::<TestCounterState>(element_id).unwrap().count, 5);
    }

    #[test]
    fn test_stateful_element_unmount_removes_state() {
        let widget = TestCounter { label: "Count".to_string() };
        let mut element = StatefulElement::new(widget);

        let (element_id, mut state, mut dirty, mut render_objects, _element_registry, build_owner, dirty_sender, mut child_ops) = create_test_context();

        // Mount
        {
            let mut ctx = ElementContext::new(
                element_id,
                None,
                Vec::new(),
                &mut state,
                &mut dirty,
                &mut render_objects,
                &build_owner,
                &dirty_sender,
                &mut child_ops,
            );
            Element::mount(&mut element, &mut ctx);
        }

        // Verify state exists
        assert!(state.get::<TestCounterState>(element_id).is_some());

        // Unmount
        {
            let mut ctx = ElementContext::new(
                element_id,
                None,
                Vec::new(),
                &mut state,
                &mut dirty,
                &mut render_objects,
                &build_owner,
                &dirty_sender,
                &mut child_ops,
            );
            Element::unmount(&mut element, &mut ctx);
        }

        // State should be removed
        assert!(state.get::<TestCounterState>(element_id).is_none());
    }

    #[test]
    fn test_stateful_element_can_update_same_type() {
        let widget = TestCounter { label: "Count".to_string() };
        let element = StatefulElement::new(widget);

        let new_widget = TestCounter { label: "Updated".to_string() };
        // Create a reference to the widget for can_update
        let widget_ref: &dyn Any = &new_widget;

        assert!(element.can_update(widget_ref));
    }

    #[test]
    fn test_build_context_request_rebuild() {
        let (element_id, _state, mut dirty, mut render_objects, _, build_owner, _dirty_sender, _child_ops) = create_test_context();

        let mut ctx = BuildContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
        };

        ctx.request_rebuild();

        assert!(build_owner.is_dirty(element_id));
    }
}
