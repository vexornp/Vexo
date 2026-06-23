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
use super::elements::RenderObjectElement;
use super::EventContext;
use super::focus::attachment::FocusAttachment;
use crate::animation::AnimationTicker;
use crate::input::InputEvent;
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
    /// initialization and for subscribing to controller notifications.
    ///
    /// Access the widget via `ctx.widget()` and the dirty callback via
    /// `ctx.dirty_callback()` to wire up controllers:
    /// ```ignore
    /// fn init(&mut self, ctx: &mut StateContext) {
    ///     let text_edit = ctx.widget().downcast_ref::<TextEdit>().unwrap();
    ///     text_edit.controller.set_dirty_callback(ctx.dirty_callback());
    /// }
    /// ```
    fn init(&mut self, ctx: &mut StateContext) {
        self.on_mount(ctx);
    }

    /// Called when the parent widget is rebuilt with a new configuration.
    ///
    /// Equivalent to Flutter's `didUpdateWidget()`. The framework has already
    /// updated the widget to the new instance before calling this method.
    /// Access the current widget via `ctx.widget()`, and compare with
    /// `old_widget` to detect changes.
    ///
    /// Use this to re-wire controller callbacks when the widget's controller
    /// changes:
    /// ```ignore
    /// fn did_update_widget(&mut self, old_widget: &dyn Any, ctx: &mut StateContext) {
    ///     let old_te = old_widget.downcast_ref::<TextEdit>().unwrap();
    ///     let new_te = ctx.widget().downcast_ref::<TextEdit>().unwrap();
    ///     if !Rc::ptr_eq(&old_te.controller.editor(), &new_te.controller.editor()) {
    ///         old_te.controller.clear_dirty_callback();
    ///         new_te.controller.set_dirty_callback(ctx.dirty_callback());
    ///     }
    /// }
    /// ```
    fn did_update_widget(&mut self, old_widget: &dyn Any, ctx: &mut StateContext) {
        self.on_update(old_widget, ctx);
    }

    /// Called when the StatefulElement is removed from the tree.
    ///
    /// Equivalent to Flutter's `dispose()`. Use this for cleanup
    /// like canceling timers, releasing resources, and unwiring
    /// controller callbacks.
    fn dispose(&mut self, ctx: &mut StateContext) {
        self.on_unmount(ctx);
    }

    /// Wire up dirty callbacks for any `StatefulMutable` fields.
    ///
    /// Override this if your state contains `StatefulMutable` fields.
    /// The callback marks the owning element dirty in the BuildOwner,
    /// triggering a rebuild on the next frame.
    ///
    /// The default implementation does nothing (no reactive fields).
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
    fn on_event(&mut self, _widget: &dyn Any, _event: &InputEvent, _ctx: &mut crate::EventContext) -> Option<Box<dyn Any>> {
        None
    }

    /// Advance animations before rebuild.
    ///
    /// Called by the reconciler on each frame before `rebuild_from_state`.
    /// Override this to advance any AnimationControllers held by this state.
    /// The `now` parameter is the current time, captured once at the start
    /// of the rebuild cycle.
    ///
    /// The default implementation does nothing.
    fn animate(&mut self, now: std::time::Instant) {
        self.on_tick(now);
    }

    // ========================================================================
    // Web-developer-friendly lifecycle aliases
    // ========================================================================

    /// Called once when the element is first mounted.
    ///
    /// Web-developer-friendly lifecycle method. Maps to React's `useEffect([])`
    /// or Vue's `onMounted()`. Override this instead of `init()`.
    fn on_mount(&mut self, _ctx: &mut StateContext) {}

    /// Called when the parent widget is rebuilt with new configuration.
    ///
    /// Web-developer-friendly lifecycle method. Maps to React's `useEffect([deps])`
    /// or Vue's `onUpdated()`. Override this instead of `did_update_widget()`.
    fn on_update(&mut self, _old_widget: &dyn Any, _ctx: &mut StateContext) {}

    /// Called when the element is removed from the tree.
    ///
    /// Web-developer-friendly lifecycle method. Maps to React's cleanup function
    /// or Vue's `onUnmounted()`. Override this instead of `dispose()`.
    fn on_unmount(&mut self, _ctx: &mut StateContext) {}

    /// Called every frame before render, for animations and per-frame logic.
    ///
    /// Web-developer-friendly lifecycle method. Maps to `requestAnimationFrame`.
    /// Override this instead of `animate()`.
    fn on_tick(&mut self, _now: std::time::Instant) {}
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
// COMPONENT STATE TRAIT
// ============================================================================

/// Trait for state objects belonging to Components.
///
/// This is the web-developer-friendly name for `State`.
/// `State` remains available as the original name.
pub trait ComponentState: State {}

/// Blanket impl: anything implementing `State` is a `ComponentState`.
impl<T: State> ComponentState for T {}

// ============================================================================
// STATE CONTEXT
// ============================================================================

/// Context provided to `State::init()`, `did_update_widget()`, and `dispose()`.
///
/// This is the Vexo equivalent of Flutter's `State` class methods.
/// The key method is `setState()`, which mutates state and marks the
/// element dirty for rebuild.
///
/// Unlike Flutter's `State.widget` getter, Vexo provides widget access
/// through `StateContext::widget()` since Rust's trait objects cannot
/// be generic over the widget type. Downcast to the concrete type:
/// ```ignore
/// let text_edit = ctx.widget().downcast_ref::<TextEdit>().unwrap();
/// ```
pub struct StateContext<'a> {
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
    /// State::init() can use this to wire AnimationControllers:
    /// ```ignore
    /// fn init(&mut self, ctx: &mut StateContext) {
    ///     self.controller.set_ticker(ctx.animation_ticker().clone());
    /// }
    /// ```
    animation_ticker: Arc<AnimationTicker>,
}

impl<'a> StateContext<'a> {
    /// Create a new StateContext. Only called by StatefulElement.
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
    /// Downcast to the concrete widget type in your State implementation:
    /// ```ignore
    /// let text_edit = ctx.widget().downcast_ref::<TextEdit>().unwrap();
    /// ```
    ///
    /// This is the Vexo equivalent of Flutter's `State.widget` getter.
    /// The widget is always the *new* (current) configuration — in
    /// `did_update_widget()`, use the `old_widget` parameter for the
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
    /// Use this in `State::init()` to wire AnimationControllers to the
    /// per-frame tick loop:
    /// ```ignore
    /// fn init(&mut self, ctx: &mut StateContext) {
    ///     self.controller.set_ticker(ctx.animation_ticker().clone());
    /// }
    /// ```
    pub fn animation_ticker(&self) -> &Arc<AnimationTicker> {
        &self.animation_ticker
    }
}

/// Context provided to `ComponentState` lifecycle methods.
///
/// Web-developer-friendly name for `StateContext`.
/// Maps to React's effect context or Vue's lifecycle hook context.
pub type LifecycleContext<'a> = StateContext<'a>;

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

    /// Check if this element is currently focused.
    pub fn is_focused(&self) -> bool {
        self.build_owner.focused_element() == Some(self.element_id)
    }
}

/// Context provided to `Component::render()`.
///
/// Web-developer-friendly name for `BuildContext`.
/// Maps to React's render function context or Vue's setup context.
pub type RenderContext<'a> = BuildContext<'a>;

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
///         Flex::column()
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

// ============================================================================
// COMPONENT TRAIT
// ============================================================================

/// Trait for widgets with persistent mutable state.
///
/// This is the web-developer-friendly name for `StatefulWidget`.
/// Maps to React's function component or Vue's component.
///
/// Use `render()` instead of `build()`. The blanket `impl StatefulWidget`
/// for `Component` types delegates `build()` to `render()`, so you only
/// need to implement this trait.
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
    /// Must implement `State + Default` for initialization and lifecycle.
    type State: State + Default;

    /// Build the widget tree using current state.
    ///
    /// Called during mount, update, and state-driven rebuilds.
    /// The state is passed mutably so the widget can modify it.
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget>;
}

/// Blanket impl: any `Component` is also a `StatefulWidget`.
///
/// Delegates `StatefulWidget::build()` to `Component::render()`,
/// so developers only need to implement `Component::render()`.
impl<T: Component> StatefulWidget for T {
    type State = T::State;

    fn build(&self, state: &mut Self::State, ctx: &mut BuildContext) -> Box<dyn Widget> {
        self.render(state, ctx)
    }
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

    /// The render object ID (ProxyRenderObject, set during mount).
    render_object_id: Option<RenderObjectKey>,

    /// Focus tree attachment for this element.
    focus_attachment: Option<FocusAttachment>,
}

impl<W: StatefulWidget> StatefulElement<W> {
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

impl<W: StatefulWidget + Clone> RenderObjectElement for StatefulElement<W> {
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

impl<W: StatefulWidget + Clone> Element for StatefulElement<W> {
    fn mount(&mut self, context: &mut ElementContext) {
        // Create focus attachment BEFORE mounting children.
        // Children will look up this element's focus node as their parent
        // when they mount, so it must exist before child mounting begins.
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context.focus_manager().create_node_for_element(element_key, parent_id);
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

        // Call State::init() lifecycle hook with widget access.
        // State can wire controller callbacks via ctx.widget() and ctx.dirty_callback().
        let mut state_ctx = StateContext::new(
            element_id,
            context.build_owner,
            &self.widget as &dyn Any,
            dirty_callback,
            context.animation_ticker.clone(),
        );
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
        // Save old widget for did_update_widget before replacing
        let old_widget: W = self.widget.clone();

        // Downcast to the concrete widget type
        if let Ok(widget) = new_widget.downcast::<W>() {
            self.widget = *widget;
        }

        let element_id = context.element_id;

        // Call State::did_update_widget() lifecycle hook.
        // The widget has already been updated to the new instance.
        // State can compare old vs. new controllers and re-wire callbacks.
        {
            let tx = context.dirty_sender.clone();
            let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let _ = tx.send(element_id);
            });
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let mut state_ctx = StateContext::new(
                element_id,
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.did_update_widget(&old_widget as &dyn Any, &mut state_ctx);
        }

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
        // Call State::dispose() lifecycle hook before removing state.
        // State can unwire controller callbacks via ctx.widget() and ctx.dirty_callback().
        if let Some(id) = self.id {
            if let Some(state) = context.state.get_mut::<W::State>(id) {
                let tx = context.dirty_sender.clone();
                let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                    let _ = tx.send(id);
                });
                let mut state_ctx = StateContext::new(
                    id,
                    context.build_owner,
                    &self.widget as &dyn Any,
                    dirty_callback,
                    context.animation_ticker.clone(),
                );
                state.dispose(&mut state_ctx);
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

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
        // Link the child's render object to our ProxyRenderObject
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
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

    fn animate(&mut self, now: std::time::Instant, context: &mut crate::element_context::ElementContext) {
        let element_id = match self.id {
            Some(id) => id,
            None => return,
        };
        if let Some(state_ref) = context.state.get_mut::<W::State>(element_id) {
            state_ref.animate(now);
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
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[crate::layout::LayoutNodeKey]) -> LayoutResult {
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

    fn hit_test(&self, position: crate::core::Point<crate::core::Logical>, _ctx: &HitTestContext) -> bool {
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

    fn layout_node(&self) -> Option<crate::layout::LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
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
    use crate::{DirtyTracking, StateStorage, RenderObjectRegistry, ElementRegistry, ElementContext, Text, BuildOwner, ChildOps, FocusManager};

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
        FocusManager,
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
        )
    }

    #[test]
    fn test_stateful_element_mount_creates_state() {
        let widget = TestCounter { label: "Count".to_string() };
        let element = StatefulElement::new(widget);

        let (element_id, mut state, mut dirty, mut render_objects, _element_registry, build_owner, dirty_sender, mut child_ops, mut focus_manager) = create_test_context();

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

        let (element_id, mut state, mut dirty, mut render_objects, _element_registry, build_owner, dirty_sender, mut child_ops, mut focus_manager) = create_test_context();

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
                &mut focus_manager,
                None,
            Arc::new(AnimationTicker::new()),
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

        let (element_id, mut state, mut dirty, mut render_objects, _element_registry, build_owner, dirty_sender, mut child_ops, mut focus_manager) = create_test_context();

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
        let (element_id, _state, mut dirty, mut render_objects, _, build_owner, _dirty_sender, _child_ops, _focus_manager) = create_test_context();

        let mut ctx = BuildContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
        };

        ctx.request_rebuild();

        assert!(build_owner.is_dirty(element_id));
    }

    #[test]
    fn test_build_context_is_focused() {
        let (element_id, _state, mut dirty, mut render_objects, _, build_owner, _dirty_sender, _child_ops, _focus_manager) = create_test_context();

        // Not focused initially
        let ctx = BuildContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
        };
        assert!(!ctx.is_focused());

        // Set this element as focused
        build_owner.set_focused_element(Some(element_id));
        let ctx = BuildContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
        };
        assert!(ctx.is_focused());

        // Clear focus
        build_owner.set_focused_element(None);
        let ctx = BuildContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
        };
        assert!(!ctx.is_focused());
    }
}
