//! Component trait for widgets with persistent mutable state.

use std::any::Any;
use std::sync::Arc;

use super::build_owner::BuildOwner;
use super::dirty::DirtyTracking;
use super::element::Element;
use super::element_context::ElementContext;
use super::elements::RenderObjectElement;
use super::focus::attachment::FocusAttachment;
use super::id::ElementKey;
use super::id::RenderObjectKey;
use super::inherited_registry::{InheritedMap, InheritedRegistry};
use super::key::WidgetKey;
use super::render_object::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectRegistry,
};
use super::widgets::Widget;
use super::EventContext;
use crate::animation::AnimationTicker;
use crate::input::InputEvent;
use crate::render::RenderCommand;

// ============================================================================
// COMPONENT STATE TRAIT
// ============================================================================

/// Trait for state objects that belong to Components.
///
/// This is the Vexo equivalent of React's state hooks or Vue's reactive state.
/// Provides lifecycle hooks and a mechanism for wiring up reactive
/// fields (like `Signal`) to automatically mark the element
/// dirty when state changes.
///
/// # Implementing ComponentState
///
/// Every `Component::State` type must implement both `ComponentState` and `Default`.
/// For simple state types with no reactive fields, use the `SimpleState` wrapper
/// or implement `ComponentState` with an empty body (all methods have default no-op impls).
///
/// For state types containing `Signal` fields, implement `set_dirty_callback()`
/// to wire them up, or use `#[derive(ComponentState)]` which auto-wires them:
///
/// ```ignore
/// #[derive(ComponentState)]
/// struct MyState {
///     count: Signal<u32>,
/// }
/// ```
pub trait ComponentState: 'static {
    /// Called once when the element is first mounted.
    ///
    /// Maps to React's `useEffect([])` or Vue's `onMounted()`.
    /// Use this for one-time initialization and for subscribing to controller notifications.
    ///
    /// Access the widget via `ctx.widget()` and the dirty callback via
    /// `ctx.dirty_callback()` to wire up controllers:
    /// ```ignore
    /// fn on_mount(&mut self, ctx: &mut LifecycleContext) {
    ///     let text_edit = ctx.widget().downcast_ref::<TextEdit>().unwrap();
    ///     text_edit.controller.set_dirty_callback(ctx.dirty_callback());
    /// }
    /// ```
    fn on_mount(&mut self, _ctx: &mut LifecycleContext) {}

    /// Called when the parent widget is rebuilt with a new configuration.
    ///
    /// Maps to React's `useEffect([deps])` or Vue's `onUpdated()`.
    /// The framework has already updated the widget to the new instance before
    /// calling this method. Access the current widget via `ctx.widget()`, and
    /// compare with `old_widget` to detect changes.
    ///
    /// Use this to re-wire controller callbacks when the widget's controller
    /// changes:
    /// ```ignore
    /// fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut LifecycleContext) {
    ///     let old_te = old_widget.downcast_ref::<TextEdit>().unwrap();
    ///     let new_te = ctx.widget().downcast_ref::<TextEdit>().unwrap();
    ///     if !Rc::ptr_eq(&old_te.controller.editor(), &new_te.controller.editor()) {
    ///         old_te.controller.clear_dirty_callback();
    ///         new_te.controller.set_dirty_callback(ctx.dirty_callback());
    ///     }
    /// }
    /// ```
    fn on_update(&mut self, _old_widget: &dyn Any, _ctx: &mut LifecycleContext) {}

    /// Called when the element is removed from the tree.
    ///
    /// Maps to React's cleanup function or Vue's `onUnmounted()`.
    /// Use this for cleanup like canceling timers, releasing resources,
    /// and unwiring controller callbacks.
    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {}

    /// Wire up dirty callbacks for any `Signal` fields.
    ///
    /// Override this if your state contains `Signal` fields.
    /// The callback marks the owning element dirty in the BuildOwner,
    /// triggering a rebuild on the next frame.
    ///
    /// The default implementation does nothing (no reactive fields).
    /// For auto-wiring, use `#[derive(ComponentState)]`.
    fn set_dirty_callback(&mut self, _callback: Arc<dyn Fn() + Send + Sync>) {}

    /// Whether this element should request focus when clicked.
    ///
    /// Only widgets that accept text input (like TextEdit) should return `true`.
    /// Most widgets (hover effects, animations, counters) should return `false`
    /// to avoid stealing focus from descendant text fields.
    fn requests_focus_on_click(&self) -> bool {
        false
    }

    /// Handle an input event, delegating from StatefulElement::on_event().
    ///
    /// Override this if your widget needs to process events (e.g., TextEdit
    /// handling keyboard input). The widget is passed as `&dyn Any` for
    /// downcasting. Returns `Some(..)` if the event was consumed.
    ///
    /// The default implementation does nothing and returns `None`.
    fn on_event(
        &mut self,
        _widget: &dyn Any,
        _event: &InputEvent,
        _ctx: &mut crate::EventContext,
    ) -> Option<Box<dyn Any>> {
        None
    }

    /// Called every frame before render, for animations and per-frame logic.
    ///
    /// Maps to `requestAnimationFrame`. Override this to advance any
    /// AnimationControllers held by this state.
    fn on_tick(&mut self, _now: std::time::Instant) {}
}

/// Wrapper for simple state types that don't need reactive fields.
///
/// Use this when your `Component::State` is a plain `Default` type
/// with no `Signal` fields. It implements both `ComponentState` and `Default`
/// with no-op lifecycle hooks.
///
/// # Example
///
/// ```ignore
/// struct MyWidget;
/// impl Component for MyWidget {
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

impl<T: Default + 'static> ComponentState for SimpleState<T> {}

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
// LIFECYCLE CONTEXT
// ============================================================================

/// Context provided to `ComponentState` lifecycle methods.
///
/// Maps to React's effect context or Vue's lifecycle hook context.
/// The key method is `setState()`, which mutates state and marks the
/// element dirty for rebuild.
///
/// Unlike Flutter's `State.widget` getter, Vexo provides widget access
/// through `LifecycleContext::widget()` since Rust's trait objects cannot
/// be generic over the widget type. Downcast to the concrete type:
/// ```ignore
/// let text_edit = ctx.widget().downcast_ref::<TextEdit>().unwrap();
/// ```
pub struct LifecycleContext<'a> {
    /// The element ID of the owning StatefulElement.
    element_id: ElementKey,

    /// Build owner for dirty marking.
    ///
    /// Uses a shared reference because `mark_needs_build()` takes `&self`
    /// via RefCell interior mutability.
    build_owner: &'a BuildOwner,

    /// The current widget configuration, type-erased.
    /// State implementations can downcast to their concrete widget type.
    widget: &'a dyn Any,

    /// Dirty callback for wiring controller change notifications.
    /// Clone this to pass to controllers that need to trigger rebuilds.
    dirty_callback: Arc<dyn Fn() + Send + Sync>,

    /// Animation ticker for registering per-frame callbacks.
    animation_ticker: Arc<AnimationTicker>,
}

impl<'a> LifecycleContext<'a> {
    /// Create a new LifecycleContext. Only called by StatefulElement.
    fn new(
        element_id: ElementKey,
        build_owner: &'a BuildOwner,
        widget: &'a dyn Any,
        dirty_callback: Arc<dyn Fn() + Send + Sync>,
        animation_ticker: Arc<AnimationTicker>,
    ) -> Self {
        Self {
            element_id,
            build_owner,
            widget,
            dirty_callback,
            animation_ticker,
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

    /// Get the current widget configuration as a type-erased reference.
    ///
    /// Downcast to the concrete widget type in your ComponentState implementation:
    /// ```ignore
    /// let text_edit = ctx.widget().downcast_ref::<TextEdit>().unwrap();
    /// ```
    ///
    /// This is the Vexo equivalent of Flutter's `State.widget` getter.
    /// The widget is always the *new* (current) configuration — in
    /// `on_update()`, use the `old_widget` parameter for the
    /// previous configuration.
    pub fn widget(&self) -> &dyn Any {
        self.widget
    }

    /// Get the dirty callback for this element.
    ///
    /// Use this to wire controller callbacks that need to trigger rebuilds.
    /// Clone the Arc and pass it to controllers like TextEditingController.
    pub fn dirty_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.dirty_callback.clone()
    }

    /// Get the animation ticker for this element.
    ///
    /// Use this in `on_mount()` to wire AnimationControllers to the
    /// per-frame tick loop:
    /// ```ignore
    /// fn on_mount(&mut self, ctx: &mut LifecycleContext) {
    ///     self.controller.set_ticker(ctx.animation_ticker().clone());
    /// }
    /// ```
    pub fn animation_ticker(&self) -> &Arc<AnimationTicker> {
        &self.animation_ticker
    }
}

// ============================================================================
// RENDER CONTEXT
// ============================================================================

/// Context provided to `Component::render()`.
///
/// Maps to React's render function context or Vue's setup context.
pub struct RenderContext<'a> {
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

    /// Nearest-ancestor cache for inherited values (read-only here).
    pub inherited_map: &'a InheritedMap,

    /// Pipeline-owned registry; `depend_on_inherited_widget` uses interior
    /// mutability to register the caller as a dependent.
    pub inherited_registry: &'a InheritedRegistry,
}

impl<'a> RenderContext<'a> {
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

    /// Check if this element is currently focused.
    pub fn is_focused(&self) -> bool {
        self.build_owner.focused_element() == Some(self.element_id)
    }

    /// Current device safe-area insets in logical pixels.
    ///
    /// Reflects the live values written each frame by
    /// [`WindowState`](crate::window::WindowState) (status bar / notch / home
    /// indicator on mobile; all-zero on desktop). Widgets such as `SafeArea`
    /// call this during [`Component::render()`] to inset their children.
    ///
    /// The returned [`EdgeInsets`] uses `left, right, top, bottom` field order.
    pub fn safe_area(&self) -> crate::layout::EdgeInsets {
        self.build_owner.safe_area_source().get()
    }

    /// Read the nearest inherited value of type `V`. Establishes a
    /// dependency: the caller rebuilds when the provider's value changes.
    ///
    /// Returns `None` if no ancestor provides `V`. The returned value is
    /// cloned out of the registry (values are `Clone + PartialEq` by the
    /// `InheritedWidget` trait requirement).
    pub fn depend_on_inherited_widget<V: Clone + 'static>(&mut self) -> Option<V> {
        let type_id = std::any::TypeId::of::<V>();
        let provider = self.inherited_map.get(type_id)?;
        let value = self.inherited_registry.value_clone::<V>(provider)?;
        self.inherited_registry
            .add_dependent(provider, type_id, self.element_id);
        Some(value)
    }
}

// ============================================================================
// COMPONENT TRAIT
// ============================================================================

/// Trait for widgets with persistent mutable state.
///
/// Maps to React's function component or Vue's component.
/// Use `render()` to describe the widget tree based on current state.
///
/// # Example
///
/// ```ignore
/// #[derive(Clone)]
/// struct Counter { label: String }
///
/// #[derive(ComponentState)]
/// struct CounterState { count: Signal<u32> }
///
/// impl Default for CounterState {
///     fn default() -> Self { Self { count: Signal::new(0) } }
/// }
///
/// impl Component for Counter {
///     type State = CounterState;
///
///     fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
///         Column::new()
///             .push(Text::new(format!("{}: {}", self.label, state.count.get())))
///             .boxed()
///     }
/// }
/// ```
pub trait Component: Sized + 'static {
    /// The mutable state type that persists across rebuilds.
    ///
    /// Must implement `ComponentState + Default` for initialization and lifecycle.
    /// The blanket `impl<T: Default + 'static> ComponentState for T {}` ensures
    /// backward compatibility with plain `Default` state types.
    type State: ComponentState + Default;

    /// Build the widget tree using current state.
    ///
    /// Called during mount, update, and state-driven rebuilds.
    /// The state is passed mutably so the widget can modify it.
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget>;

    /// Optional key for identity across frames.
    ///
    /// Widgets of the same type with *different* keys cannot update each
    /// other in place during reconciliation — the reconciler will unmount
    /// the old element and mount a fresh one. This is essential when sibling
    /// Components of the same type are reordered or inserted/removed: without
    /// distinct keys, positional matching pairs the wrong (widget, element)
    /// and stale per-element state (focus, cursor, blink) carries over to the
    /// wrong logical widget.
    ///
    /// Default: `None` (positional matching, fine for leaf components or
    /// static sibling sets). Override via `with_key()`-style builders.
    fn key(&self) -> Option<WidgetKey> {
        None
    }
}

/// Element for Component widgets.
///
/// StatefulElement wraps a Component and:
/// - Stores the widget configuration
/// - Manages state in StateStorage (keyed by element ID)
/// - Builds a child widget tree on mount and update
/// - Delegates rendering to the child element
pub struct StatefulElement<W: Component> {
    /// The widget configuration.
    widget: W,

    /// The element ID (set during mount).
    id: Option<ElementKey>,

    /// The widget key (if any).
    key: Option<WidgetKey>,

    /// The render object ID (ProxyRenderObject, set during mount).
    render_object_id: Option<RenderObjectKey>,

    /// Focus tree attachment for this element.
    focus_attachment: Option<FocusAttachment>,
}

impl<W: Component> StatefulElement<W> {
    /// Create a new StatefulElement from a widget.
    pub fn new(widget: W) -> Self {
        Self {
            widget,
            id: None,
            key: None,
            render_object_id: None,
            focus_attachment: None,
        }
    }
}

impl<W: Component + Clone> StatefulElement<W> {
    /// Build the child widget using the element's state.
    fn build_child_widget(
        &self,
        element_id: ElementKey,
        state: &mut W::State,
        dirty: &mut DirtyTracking,
        render_objects: &mut RenderObjectRegistry,
        build_owner: &BuildOwner,
        inherited_map: &InheritedMap,
        inherited_registry: &InheritedRegistry,
    ) -> Box<dyn Widget> {
        let mut render_ctx = RenderContext {
            element_id,
            dirty,
            render_objects,
            build_owner,
            inherited_map,
            inherited_registry,
        };
        self.widget.render(state, &mut render_ctx)
    }
}

impl<W: Component + Clone> RenderObjectElement for StatefulElement<W> {
    fn widget(&self) -> Option<&dyn Widget> {
        Some(&self.widget)
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(w) = widget.as_any().downcast_ref::<W>() {
            self.widget = w.clone();
        }
    }

    fn render_object_id(&self) -> Option<RenderObjectKey> {
        self.render_object_id
    }

    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) {
        self.render_object_id = id;
    }

    fn stored_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn set_stored_key(&mut self, key: Option<WidgetKey>) {
        self.key = key;
    }

    fn element_id(&self) -> Option<ElementKey> {
        self.id
    }

    fn set_element_id(&mut self, id: Option<ElementKey>) {
        self.id = id;
    }
}

impl<W: Component + Clone> Element for StatefulElement<W> {
    fn mount(&mut self, context: &mut ElementContext) {
        // Create focus attachment BEFORE mounting children.
        // Children will look up this element's focus node as their parent
        // when they mount, so it must exist before child mounting begins.
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context
            .focus_manager()
            .create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        // Use RenderObjectElement's default mount for render object creation
        // This creates the ProxyRenderObject and stores the element ID + key
        self.mount_render_object(context);

        let element_id = context.element_id;

        // Initialize state with Default
        let mut state = W::State::default();

        // Wire up dirty callback using channel sender.
        let tx = context.dirty_sender.clone();
        let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.send(element_id);
        });
        state.set_dirty_callback(dirty_callback.clone());

        // Call ComponentState::on_mount() lifecycle hook with widget access.
        // State can wire controller callbacks via ctx.widget() and ctx.dirty_callback().
        let mut lifecycle_ctx = LifecycleContext::new(
            element_id,
            context.build_owner,
            &self.widget as &dyn Any,
            dirty_callback,
            context.animation_ticker.clone(),
        );
        state.on_mount(&mut lifecycle_ctx);

        // Tag this element's focus node as a text input if its state requests
        // focus on click (e.g. TextEdit). The pipeline consults this flag to
        // decide whether to show the software keyboard / paint the cursor,
        // instead of walking the render-object subtree (which would wrongly
        // match when an ancestor like a ScrollView is focused).
        let is_text_input = state.requests_focus_on_click();

        // Store state in StateStorage
        context.insert_state(element_id, state);

        if let Some(attachment) = &self.focus_attachment {
            if let Some(node) = context.focus_manager().get_mut(attachment.node_id()) {
                node.is_text_input = is_text_input;
            }
        }

        // Build the child widget tree using RenderContext
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.dirty,
                context.render_objects,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
            )
        };

        // Mount the child element tree via child_ops
        context.inflate_child(None, child_widget);
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Save old widget for on_update before replacing
        let old_widget: W = self.widget.clone();

        // Downcast Box<dyn Any> → Box<dyn Widget> → W
        // The reconciler wraps widgets as Box<Box<dyn Widget>>, so we need
        // the two-step downcast (same as ContainerElement::rebuild and
        // RenderObjectElement::update_render_object).
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            if let Some(w) = widget.as_any().downcast_ref::<W>() {
                self.widget = w.clone();
            }
        }

        let element_id = context.element_id;

        // Call ComponentState::on_update() lifecycle hook.
        // The widget has already been updated to the new instance.
        // State can compare old vs. new controllers and re-wire callbacks.
        {
            let tx = context.dirty_sender.clone();
            let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let _ = tx.send(element_id);
            });
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let mut lifecycle_ctx = LifecycleContext::new(
                element_id,
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.on_update(&old_widget as &dyn Any, &mut lifecycle_ctx);
        }

        // Build the child widget tree using RenderContext
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.dirty,
                context.render_objects,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
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
        // Call ComponentState::on_unmount() lifecycle hook before removing state.
        // State can unwire controller callbacks via ctx.widget() and ctx.dirty_callback().
        if let Some(id) = self.id {
            if let Some(state) = context.state.get_mut::<W::State>(id) {
                let tx = context.dirty_sender.clone();
                let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                    let _ = tx.send(id);
                });
                let mut lifecycle_ctx = LifecycleContext::new(
                    id,
                    context.build_owner,
                    &self.widget as &dyn Any,
                    dirty_callback,
                    context.animation_ticker.clone(),
                );
                state.on_unmount(&mut lifecycle_ctx);
            }
        }

        // Use RenderObjectElement's default unmount for render object removal
        // This unregisters global key, removes render object, and removes state
        self.unmount_render_object(context);

        // Unmount child element via child_ops BEFORE detaching our focus node.
        // Children may need to reference their parent's focus node during unmount.
        if let Some(child_key) = context.children().first().copied() {
            context.unmount_child(child_key);
        }

        // Detach focus node from the focus tree after children are unmounted.
        if let Some(mut attachment) = self.focus_attachment.take() {
            attachment.detach(context.focus_manager());
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

    fn child_mounted(
        &mut self,
        _slot: Option<usize>,
        child_ro: Option<RenderObjectKey>,
        context: &mut ElementContext,
    ) {
        // Link the child's render object to our ProxyRenderObject
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }

    fn rebuild_from_state(&mut self, context: &mut ElementContext) {
        // Rebuild using the CURRENT widget + updated state.
        // This is called by perform_rebuilds() when setState() or
        // Signal::set() marked this element dirty.

        let element_id = self.id.unwrap_or(context.element_id);

        // Build the child widget tree using RenderContext
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.dirty,
                context.render_objects,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
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

    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
        state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        let element_id = match self.id {
            Some(id) => id,
            None => return None,
        };
        let state_ref = match state.get_mut::<W::State>(element_id) {
            Some(s) => s,
            None => return None,
        };
        state_ref.on_event(&self.widget, event, context)
    }

    fn animate(
        &mut self,
        now: std::time::Instant,
        context: &mut crate::element_context::ElementContext,
    ) {
        let element_id = match self.id {
            Some(id) => id,
            None => return,
        };
        if let Some(state_ref) = context.state.get_mut::<W::State>(element_id) {
            state_ref.on_tick(now);
        }
    }

    fn focus_attachment(&self) -> &Option<FocusAttachment> {
        &self.focus_attachment
    }

    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> {
        &mut self.focus_attachment
    }
}

// ============================================================================
// PROXY RENDER OBJECT
// ============================================================================

/// Proxy render object for StatefulElement.
///
/// StatefulElement doesn't render itself - it delegates painting to its child.
/// But unlike EmptyRenderObject, ProxyRenderObject participates in the render tree:
/// - Pass-through layout (wraps child's Taffy node)
/// - No paint commands (invisible)
/// - Bounds-based hit test (enables StatefulElement to appear in hit test path)
///
/// This eliminates the need for Phase 2 ancestor walking in event dispatch.
pub struct ProxyRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<crate::core::Bounds<crate::core::Logical>>,
    layout_node: Option<crate::layout::LayoutNodeKey>,
}

impl ProxyRenderObject {
    /// Create a new ProxyRenderObject.
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }
}

impl Default for ProxyRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for ProxyRenderObject {
    fn layout(
        &mut self,
        ctx: &mut LayoutContext,
        child_nodes: &[crate::layout::LayoutNodeKey],
    ) -> LayoutResult {
        let layout = crate::layout::Layout::default()
            .flex_direction(crate::layout::FlexDirection::Column)
            .align(crate::layout::AlignItems::Stretch);
        let node = ctx.engine().create_container(&layout, child_nodes);
        self.layout_node = Some(node);
        LayoutResult {
            node,
            size: crate::core::Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        Vec::new()
    }

    fn hit_test(
        &self,
        position: crate::core::Point<crate::core::Logical>,
        _ctx: &HitTestContext,
    ) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<crate::layout::LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
    }
}

// ============================================================================
// WIDGET TRAIT IMPLEMENTATION FOR COMPONENT
// ============================================================================

/// Blanket Widget implementation for Component types.
///
/// This allows Component implementations to be used anywhere
/// a Widget is expected.
impl<W: Component + Clone + 'static> Widget for W {
    fn key(&self) -> Option<WidgetKey> {
        // Delegate to the Component's `key()` so that components can opt
        // into identity-based reconciliation (overriding `Component::key`).
        // Without this delegation, the blanket impl would hard-code `None`
        // and `with_key()` builders on Component types would silently no-op.
        <Self as Component>::key(self)
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(StatefulElement::new(self.clone()))
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ProxyRenderObject::new())
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
    use crate::inherited_registry::{InheritedMap, InheritedRegistry};
    use crate::reactive::Signal;
    use crate::ComponentState;
    use crate::{
        BuildOwner, ChildOps, DirtyTracking, ElementContext, ElementRegistry, FocusManager,
        RenderObjectRegistry, StateStorage, Text,
    };
    use slotmap::SecondaryMap;

    #[derive(Clone)]
    struct TestCounter {
        label: String,
    }

    struct TestCounterState {
        count: Signal<u32>,
    }

    impl Default for TestCounterState {
        fn default() -> Self {
            Self {
                count: Signal::new(0),
            }
        }
    }

    impl ComponentState for TestCounterState {
        fn set_dirty_callback(&mut self, callback: std::sync::Arc<dyn Fn() + Send + Sync>) {
            self.count.set_dirty_callback(callback);
        }
    }

    impl Component for TestCounter {
        type State = TestCounterState;

        fn render(
            &self,
            state: &mut TestCounterState,
            _ctx: &mut RenderContext,
        ) -> Box<dyn Widget> {
            Box::new(Text::new(format!("{}: {}", self.label, state.count.get())))
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
        FocusManager,
        InheritedRegistry,
        SecondaryMap<ElementKey, Arc<InheritedMap>>,
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
            FocusManager::new(),
            InheritedRegistry::new(),
            SecondaryMap::new(),
        )
    }

    #[test]
    fn test_stateful_element_mount_creates_state() {
        let widget = TestCounter {
            label: "Count".to_string(),
        };
        let element = StatefulElement::new(widget);

        let (
            element_id,
            mut state,
            mut dirty,
            mut render_objects,
            _element_registry,
            build_owner,
            dirty_sender,
            mut child_ops,
            mut focus_manager,
            inherited_registry,
            mut inherited_maps,
        ) = create_test_context();
        let empty_map = InheritedMap::empty();

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
            &mut focus_manager,
            None,
            Arc::new(AnimationTicker::new()),
            &empty_map,
            &inherited_registry,
            &mut inherited_maps,
        );

        let mut element = element;
        Element::mount(&mut element, &mut ctx);

        // State should be created with default value
        assert!(state.get::<TestCounterState>(element_id).is_some());
        assert_eq!(
            state
                .get::<TestCounterState>(element_id)
                .unwrap()
                .count
                .get(),
            0
        );
    }

    #[test]
    fn test_stateful_element_update_preserves_state() {
        let widget = TestCounter {
            label: "Count".to_string(),
        };
        let mut element = StatefulElement::new(widget);

        let (
            element_id,
            mut state,
            mut dirty,
            mut render_objects,
            _element_registry,
            build_owner,
            dirty_sender,
            mut child_ops,
            mut focus_manager,
            inherited_registry,
            mut inherited_maps,
        ) = create_test_context();
        let empty_map = InheritedMap::empty();

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
                &mut focus_manager,
                None,
                Arc::new(AnimationTicker::new()),
                &empty_map,
                &inherited_registry,
                &mut inherited_maps,
            );
            Element::mount(&mut element, &mut ctx);
        }

        // Modify state
        state
            .get_mut::<TestCounterState>(element_id)
            .unwrap()
            .count
            .set(5);

        // Update with new widget
        let new_widget = TestCounter {
            label: "Updated".to_string(),
        };
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
                &mut focus_manager,
                None,
                Arc::new(AnimationTicker::new()),
                &empty_map,
                &inherited_registry,
                &mut inherited_maps,
            );
            Element::update(&mut element, Box::new(new_widget), &mut ctx);
        }

        // State should be preserved
        assert_eq!(
            state
                .get::<TestCounterState>(element_id)
                .unwrap()
                .count
                .get(),
            5
        );
    }

    #[test]
    fn test_stateful_element_unmount_removes_state() {
        let widget = TestCounter {
            label: "Count".to_string(),
        };
        let mut element = StatefulElement::new(widget);

        let (
            element_id,
            mut state,
            mut dirty,
            mut render_objects,
            _element_registry,
            build_owner,
            dirty_sender,
            mut child_ops,
            mut focus_manager,
            inherited_registry,
            mut inherited_maps,
        ) = create_test_context();
        let empty_map = InheritedMap::empty();

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
                &mut focus_manager,
                None,
                Arc::new(AnimationTicker::new()),
                &empty_map,
                &inherited_registry,
                &mut inherited_maps,
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
                &mut focus_manager,
                None,
                Arc::new(AnimationTicker::new()),
                &empty_map,
                &inherited_registry,
                &mut inherited_maps,
            );
            Element::unmount(&mut element, &mut ctx);
        }

        // State should be removed
        assert!(state.get::<TestCounterState>(element_id).is_none());
    }

    #[test]
    fn test_stateful_element_can_update_same_type() {
        let widget = TestCounter {
            label: "Count".to_string(),
        };
        let element = StatefulElement::new(widget);

        let new_widget = TestCounter {
            label: "Updated".to_string(),
        };
        // Create a reference to the widget for can_update
        let widget_ref: &dyn Any = &new_widget;

        assert!(element.can_update(widget_ref));
    }

    #[test]
    fn test_render_context_request_rebuild() {
        let (
            element_id,
            _state,
            mut dirty,
            mut render_objects,
            _,
            build_owner,
            _dirty_sender,
            _child_ops,
            _focus_manager,
            inherited_registry,
            _inherited_maps,
        ) = create_test_context();
        let empty_map = InheritedMap::empty();

        let mut ctx = RenderContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
            inherited_map: &empty_map,
            inherited_registry: &inherited_registry,
        };

        ctx.request_rebuild();

        assert!(build_owner.is_dirty(element_id));
    }

    #[test]
    fn test_render_context_is_focused() {
        let (
            element_id,
            _state,
            mut dirty,
            mut render_objects,
            _,
            build_owner,
            _dirty_sender,
            _child_ops,
            _focus_manager,
            inherited_registry,
            _inherited_maps,
        ) = create_test_context();
        let empty_map = InheritedMap::empty();

        // Not focused initially
        let ctx = RenderContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
            inherited_map: &empty_map,
            inherited_registry: &inherited_registry,
        };
        assert!(!ctx.is_focused());

        // Set this element as focused
        build_owner.set_focused_element(Some(element_id));
        let ctx = RenderContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
            inherited_map: &empty_map,
            inherited_registry: &inherited_registry,
        };
        assert!(ctx.is_focused());

        // Clear focus
        build_owner.set_focused_element(None);
        let ctx = RenderContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
            inherited_map: &empty_map,
            inherited_registry: &inherited_registry,
        };
        assert!(!ctx.is_focused());
    }

    #[test]
    fn depend_on_inherited_widget_returns_value_when_provider_present() {
        // Set up a registry with one provider exposing u32=42.
        let reg = InheritedRegistry::new();
        let provider_key = make_element_key();
        reg.register_provider(provider_key, std::any::TypeId::of::<u32>(), Box::new(42u32));

        // Build an InheritedMap that points u32 -> provider_key.
        let map = InheritedMap::empty().with_insert(std::any::TypeId::of::<u32>(), provider_key);

        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();
        let element_id = make_element_key();

        let mut ctx = RenderContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
            inherited_map: &map,
            inherited_registry: &reg,
        };

        let v = ctx.depend_on_inherited_widget::<u32>();
        assert_eq!(v, Some(42));
    }

    #[test]
    fn depend_on_inherited_widget_returns_none_when_no_provider() {
        let reg = InheritedRegistry::new();
        let map = InheritedMap::empty();

        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();
        let element_id = make_element_key();

        let mut ctx = RenderContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
            inherited_map: &map,
            inherited_registry: &reg,
        };

        let v = ctx.depend_on_inherited_widget::<u32>();
        assert_eq!(v, None);
    }

    #[test]
    fn depend_on_inherited_widget_registers_dependent() {
        let reg = InheritedRegistry::new();
        let provider_key = make_element_key();
        reg.register_provider(provider_key, std::any::TypeId::of::<u32>(), Box::new(0u32));

        let map = InheritedMap::empty().with_insert(std::any::TypeId::of::<u32>(), provider_key);

        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();
        let element_id = make_element_key();

        let mut ctx = RenderContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
            inherited_map: &map,
            inherited_registry: &reg,
        };

        let _ = ctx.depend_on_inherited_widget::<u32>();

        // The caller's element_id should now be in the provider's dependents.
        let deps = reg.dependents_for(provider_key);
        assert!(deps.contains(&element_id));
    }
}
