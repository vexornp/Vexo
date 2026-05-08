//! StatefulWidget trait for widgets with persistent mutable state.

use std::any::Any;

use super::id::ElementId;
use super::id::RenderObjectId;
use super::dirty::DirtyTracking;
use super::render_object::{RenderObject, RenderObjectRegistry, LayoutContext, LayoutResult, PaintContext, HitTestContext};
use super::build_owner::BuildOwner;
use super::element::{Element, ElementRegistry};
use super::element_context::ElementContext;
use super::key::WidgetKey;
use super::widgets::Widget;
use crate::core::Logical;
use crate::render::RenderCommand;

/// Context provided to StatefulWidget::build().
pub struct BuildContext<'a> {
    /// The element ID for this stateful element.
    pub element_id: ElementId,

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
    id: Option<ElementId>,

    /// The widget key (if any).
    key: Option<WidgetKey>,

    /// The child element ID (from build()).
    child_element_id: Option<ElementId>,

    /// The render object ID (from child, if any).
    render_object_id: Option<RenderObjectId>,
}

impl<W: StatefulWidget> StatefulElement<W> {
    /// Create a new StatefulElement from a widget.
    pub fn new(widget: W) -> Self {
        let key = None; // StatefulWidget widgets can have keys via Widget trait
        Self {
            widget,
            id: None,
            key,
            child_element_id: None,
            render_object_id: None,
        }
    }
}

impl<W: StatefulWidget + Clone> StatefulElement<W> {
    /// Build the child widget using the element's state.
    fn build_child_widget(
        &self,
        element_id: ElementId,
        state: &mut W::State,
        dirty: &mut DirtyTracking,
        render_objects: &mut RenderObjectRegistry,
        build_owner: &mut BuildOwner,
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

        // Initialize state with Default
        let state = W::State::default();
        context.insert_state(context.element_id, state);

        // Build the child widget tree
        // We need to split borrows carefully:
        // 1. First, extract render_objects and build_owner from context
        // 2. Then, get state and dirty separately

        let element_id = context.element_id;
        let child_widget;

        {
            // Extract render_objects and build_owner first (they're Options)
            let render_objects = context.render_objects.take();
            let build_owner = context.build_owner.take();

            // Now we can borrow state and dirty without conflict
            // Use reborrow to get &mut from &'a mut
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let dirty = &mut *context.dirty;

            // Build with the extracted references
            if let (Some(ro), Some(bo)) = (render_objects, build_owner) {
                child_widget = self.build_child_widget(element_id, state_ref, dirty, ro, bo);

                // Restore the taken values
                context.render_objects = Some(ro);
                context.build_owner = Some(bo);
            } else {
                child_widget = Box::new(super::widgets::Text::new("Error: Missing registries"));
            }
        }

        // Mount the child element - take the registry to avoid double borrow
        let element_registry = context.element_registry.take();
        if let Some(registry) = element_registry {
            let child_id = registry.mount(child_widget.create_element(), Some(context.element_id));
            self.child_element_id = Some(child_id);

            // Get the child's render object
            self.render_object_id = registry.get(child_id)
                .and_then(|el| el.render_object());

            // Restore the registry
            context.element_registry = Some(registry);
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast to the concrete widget type
        // downcast::<W>() returns Box<W>, so we need to dereference
        if let Ok(widget) = new_widget.downcast::<W>() {
            self.widget = *widget;
        }

        // Build the child widget tree
        let element_id = context.element_id;
        let child_widget;

        {
            // Extract render_objects and build_owner first (they're Options)
            let render_objects = context.render_objects.take();
            let build_owner = context.build_owner.take();

            // Now we can borrow state and dirty without conflict
            // Use reborrow to get &mut from &'a mut
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let dirty = &mut *context.dirty;

            // Build with the extracted references
            if let (Some(ro), Some(bo)) = (render_objects, build_owner) {
                child_widget = self.build_child_widget(element_id, state_ref, dirty, ro, bo);

                // Restore the taken values
                context.render_objects = Some(ro);
                context.build_owner = Some(bo);
            } else {
                child_widget = Box::new(super::widgets::Text::new("Error: Missing registries"));
            }
        }

        // Reconcile child element - take the registry to avoid double borrow
        let element_registry = context.element_registry.take();
        if let Some(registry) = element_registry {
            if let Some(child_id) = self.child_element_id {
                if registry.contains(child_id) {
                    // Update existing child
                    let widget_any: Box<dyn Any> = Box::new(child_widget.clone_boxed());
                    registry.update_element(child_id, widget_any, context);
                } else {
                    // Mount new child
                    let new_child_id = registry.mount(child_widget.create_element(), Some(context.element_id));
                    self.child_element_id = Some(new_child_id);
                }
            } else {
                // No existing child, mount new
                let child_id = registry.mount(child_widget.create_element(), Some(context.element_id));
                self.child_element_id = Some(child_id);
            }

            // Update render object reference
            if let Some(child_id) = self.child_element_id {
                self.render_object_id = registry.get(child_id)
                    .and_then(|el| el.render_object());
            }

            // Restore the registry
            context.element_registry = Some(registry);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Unregister global key if present
        if let Some(WidgetKey::Global(_)) = &self.key {
            if let Some(id) = self.id {
                context.unregister_global_key(id);
            }
        }

        // Unmount child element
        if let Some(child_id) = self.child_element_id {
            if let Some(registry) = &mut context.element_registry {
                registry.unmount(child_id);
            }
        }

        // Remove state from storage
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        if let Some(child_id) = self.child_element_id {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object_id
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<W>().is_some()
    }

    fn has_children(&self) -> bool {
        self.child_element_id.is_some()
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
    fn layout(&mut self, ctx: &mut LayoutContext, _children: &[crate::layout::LayoutNodeId]) -> LayoutResult {
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
