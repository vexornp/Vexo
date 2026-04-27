# Three-Tree Architecture for Vexo

**Date:** 2026-04-27
**Status:** Design Approved
**Author:** Claude

## Summary

Refactor Vexo from immediate-mode to retain-mode rendering using Flutter's three-tree architecture: Widget tree (immutable configuration), Element tree (stateful lifecycle), and RenderObject tree (layout and paint). This enables efficient diffing, incremental updates, and better state preservation.

## Motivation

Vexo currently rebuilds the entire widget tree, clears layout, and redraws everything each frame. This is inefficient for:
- Large UI trees with minimal changes
- Animations that only affect a few widgets
- State preservation (scroll position, text input)

Flutter's three-tree architecture solves these problems by:
1. Retaining layout and render state across frames
2. Diffing widget trees to update only what changed
3. Key-based identity for widgets across reorders

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    WIDGET TREE (Immutable)                      │
│  Rebuilt each frame, describes "what should exist"             │
│  Cheap to create, no state, cloneable                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ create_element() / can_update()
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    ELEMENT TREE (Stateful)                      │
│  Persistent across frames, manages lifecycle                    │
│  Holds widget state, connects widgets to render objects         │
│  Updated via reconciliation algorithm                            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ render_object()
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    RENDEROBJECT TREE (Layout/Paint)             │
│  Persistent across frames, performs layout and painting         │
│  Dirty tracking: only layout/paint what changed                 │
│  Hit testing for input events                                    │
└─────────────────────────────────────────────────────────────────┘
```

## Core Types

### Widget (Immutable Configuration)

```rust
/// Immutable widget configuration - rebuilt each frame
pub trait Widget: Clone {
    /// Optional key for identity across frames
    fn key(&self) -> Option<Key> { None }

    /// Create the corresponding element for this widget
    fn create_element(&self) -> Box<dyn Element>;

    /// Check if this widget can update an existing element
    fn can_update(&self, other: &dyn Widget) -> bool {
        self.type_id() == other.type_id() && self.key() == other.key()
    }
}

/// Key for widget identity
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key(String);
```

### Element (Stateful, Lifecycle)

```rust
/// Persistent element with state and lifecycle
pub trait Element {
    /// Called when element is added to the tree
    fn mount(&mut self, parent: ElementId, context: &mut ElementContext);

    /// Called when widget configuration changes
    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext);

    /// Called when element is removed from the tree
    fn unmount(&mut self, context: &mut ElementContext);

    /// Visit children for traversal
    fn visit_children(&self, visitor: &mut dyn FnMut(&dyn Element));

    /// Get associated render object (if any)
    fn render_object(&self) -> Option<RenderObjectId>;

    /// Handle input event
    fn handle_event(
        &mut self,
        event: &InputEvent,
        context: &mut ElementContext,
    ) -> EventResponse;
}

/// Unique identifier for elements
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ElementId(usize);
```

### RenderObject (Layout, Paint)

```rust
/// Persistent render object for layout and painting
pub trait RenderObject {
    /// Perform layout with given constraints, return computed size
    fn layout(&mut self, constraints: LayoutConstraints, context: &mut LayoutContext) -> Size;

    /// Generate paint commands
    fn paint(&self, context: &mut PaintContext) -> Vec<RenderCommand>;

    /// Hit test for pointer events
    fn hit_test(&self, position: Point, context: &HitTestContext) -> HitTestResult;

    /// Mark needs layout
    fn mark_needs_layout(&mut self);

    /// Mark needs paint
    fn mark_needs_paint(&mut self);

    /// Parent render object (for layout propagation)
    fn parent(&self) -> Option<RenderObjectId>;

    /// Children for container render objects
    fn children(&self) -> &[RenderObjectId];
}

/// Unique identifier for render objects
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderObjectId(usize);
```

## Reconciliation Algorithm

### Matching Strategy

Flutter's approach: **Type + Key matching**

1. **Keyed matching first** - Build map of `{key → element}` for existing children
2. **Match by key** - If new widget has key, look up in map
3. **Type + position fallback** - If no key, match by type at same position
4. **canUpdate check** - Verify `widget.can_update(&existing_widget)` returns true
5. **Handle unmatched** - Unmount elements without matching widgets, mount new widgets

### Reconciliation Actions

```rust
enum ReconciliationAction {
    /// New widget, no existing element → create and mount
    Insert { widget: Box<dyn Widget>, index: usize },

    /// Same widget type & key → update in place
    Update { element: ElementId, new_widget: Box<dyn Widget> },

    /// Different widget or key → remove old, insert new
    Replace { element: ElementId, widget: Box<dyn Widget> },

    /// Widget removed → unmount element
    Remove { element: ElementId },

    /// Widget moved to new position → reorder
    Move { element: ElementId, from: usize, to: usize },
}
```

### Algorithm Implementation

```rust
impl ElementRegistry {
    fn reconcile_children(&mut self, parent: ElementId, new_widgets: Vec<Box<dyn Widget>>) {
        // 1. Build key map for existing children
        let key_map: HashMap<Key, ElementId> = self.children(parent)
            .iter()
            .filter_map(|id| {
                self.get(*id).widget().key().map(|k| (k.clone(), *id))
            })
            .collect();

        // 2. Match new widgets to existing elements
        let mut new_children = Vec::new();
        let mut matched = HashSet::new();

        for (index, widget) in new_widgets.iter().enumerate() {
            let element = if let Some(key) = widget.key() {
                // Keyed: look up in map
                if let Some(&id) = key_map.get(&key) {
                    if self.get(id).widget().can_update(widget.as_ref()) {
                        matched.insert(id);
                        Some(id)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                // Non-keyed: match by type at same position
                let existing = self.children(parent).get(index);
                if let Some(&id) = existing {
                    if !matched.contains(&id) && self.get(id).widget().can_update(widget.as_ref()) {
                        matched.insert(id);
                        Some(id)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            // 3. Perform action
            if let Some(id) = element {
                self.update(id, widget.clone());
                new_children.push(id);
            } else {
                let id = self.mount(widget.clone(), Some(parent));
                new_children.push(id);
            }
        }

        // 4. Unmount unmatched elements
        for id in self.children(parent) {
            if !matched.contains(&id) {
                self.unmount(id);
            }
        }

        // 5. Update children order
        self.set_children(parent, new_children);
    }
}
```

## Element Tree Structure

### Element Registry

```rust
/// Central registry for all live elements
pub struct ElementRegistry {
    elements: HashMap<ElementId, Box<dyn Element>>,
    parent_map: HashMap<ElementId, Option<ElementId>>,
    children_map: HashMap<ElementId, Vec<ElementId>>,
    root: Option<ElementId>,
    next_id: usize,
}

impl ElementRegistry {
    fn mount(&mut self, widget: Box<dyn Widget>, parent: Option<ElementId>) -> ElementId;
    fn update(&mut self, element_id: ElementId, new_widget: Box<dyn Widget>);
    fn unmount(&mut self, element_id: ElementId);
    fn reconcile_children(&mut self, parent: ElementId, new_widgets: Vec<Box<dyn Widget>>);
    fn reconcile_root(&mut self, widget: Box<dyn Widget>);
}
```

### Element Context

```rust
/// Context provided to element lifecycle methods
pub struct ElementContext<'a> {
    registry: &'a mut ElementRegistry,
    render_objects: &'a mut RenderObjectRegistry,
    state_storage: &'a mut StateStorage,
}

impl<'a> ElementContext<'a> {
    /// Create a child element
    fn mount_child(&mut self, widget: Box<dyn Widget>) -> ElementId;

    /// Update an existing child
    fn update_child(&mut self, element: ElementId, widget: Box<dyn Widget>);

    /// Remove a child
    fn unmount_child(&mut self, element: ElementId);

    /// Access element state
    fn get_state<T: 'static>(&self, element: ElementId) -> Option<&T>;
    fn get_state_mut<T: 'static>(&mut self, element: ElementId) -> Option<&mut T>;
}
```

### State Storage

```rust
/// Type-erased state storage for elements
pub struct StateStorage {
    states: HashMap<ElementId, Box<dyn Any>>,
}

impl StateStorage {
    fn insert<T: 'static>(&mut self, element: ElementId, state: T);
    fn get<T: 'static>(&self, element: ElementId) -> Option<&T>;
    fn get_mut<T: 'static>(&mut self, element: ElementId) -> Option<&mut T>;
    fn remove(&mut self, element: ElementId);
}
```

## RenderObject Tree

### RenderObject Registry

```rust
/// Registry for render objects, keyed by ID
pub struct RenderObjectRegistry {
    objects: HashMap<RenderObjectId, Box<dyn RenderObject>>,
    element_map: HashMap<RenderObjectId, ElementId>,
    root: Option<RenderObjectId>,
    next_id: usize,
}

impl RenderObjectRegistry {
    fn create(&mut self, object: Box<dyn RenderObject>, owner: ElementId) -> RenderObjectId;
    fn get(&self, id: RenderObjectId) -> Option<&dyn RenderObject>;
    fn get_mut(&mut self, id: RenderObjectId) -> Option<&mut dyn RenderObject>;
    fn remove(&mut self, id: RenderObjectId);
}
```

### Layout Context

```rust
/// Layout context passed to RenderObject.layout()
pub struct LayoutContext<'a> {
    engine: &'a mut dyn LayoutEngine,
    needs_layout: &'a mut HashSet<RenderObjectId>,
}

impl<'a> LayoutContext<'a> {
    /// Layout a child render object
    fn layout_child(&mut self, child: RenderObjectId, constraints: LayoutConstraints) -> Size;

    /// Mark this object as needing layout
    fn mark_needs_layout(&mut self, id: RenderObjectId);
}
```

### Paint Context

```rust
/// Paint context passed to RenderObject.paint()
pub struct PaintContext<'a> {
    offset: Point,
    clip_bounds: Option<Bounds>,
    commands: &'a mut Vec<RenderCommand>,
    text_collector: &'a mut TextCollector,
}

impl<'a> PaintContext<'a> {
    /// Push a render command
    fn push_command(&mut self, command: RenderCommand);

    /// Paint a child render object
    fn paint_child(&mut self, child: RenderObjectId, offset: Point);
}
```

## Rendering Pipeline

### Frame Flow

```rust
impl WindowState {
    pub fn render(&mut self) {
        // 1. Generate new widget tree (immutable config)
        let new_widget_tree = self.app.view(&self.app_state);

        // 2. Reconcile: diff new widgets with existing element tree
        self.element_registry.reconcile_root(new_widget_tree);

        // 3. Layout: only objects marked dirty
        self.layout_dirty_objects();

        // 4. Paint: only objects marked dirty
        self.paint_dirty_objects();

        // 5. Submit to GPU
        self.backend.render(&self.batches);
    }

    fn layout_dirty_objects(&mut self) {
        while let Some(id) = self.layout_queue.pop() {
            if let Some(obj) = self.render_objects.get_mut(id) {
                obj.layout(self.root_constraints, &mut self.layout_context);
            }
        }
    }

    fn paint_dirty_objects(&mut self) {
        self.collect_paint_commands(self.root_render_object);
    }
}
```

### Dirty Tracking

```rust
/// Tracks which render objects need layout or paint
pub struct DirtyTracking {
    needs_layout: HashSet<RenderObjectId>,
    needs_paint: HashSet<RenderObjectId>,
}

impl DirtyTracking {
    /// Called when widget changes affect layout
    fn mark_needs_layout(&mut self, id: RenderObjectId) {
        self.needs_layout.insert(id);
        // Propagate to parents (layout changes bubble up)
    }

    /// Called when widget changes affect only paint
    fn mark_needs_paint(&mut self, id: RenderObjectId) {
        self.needs_paint.insert(id);
        // Propagate to children (paint changes trickle down)
    }
}
```

### Comparison: Before vs After

| Aspect | Before (Immediate) | After (Retain) |
|--------|-------------------|----------------|
| Widget tree | Rebuilt every frame | Rebuilt every frame |
| Layout | Full recomputation | Only dirty objects |
| Paint | Full redraw | Only dirty regions |
| State | Centralized registry | Per-element state |
| Identity | None (no keys) | Key-based across frames |

## Widget Types

### Leaf Widgets

```rust
/// Text widget - displays string
pub struct Text {
    key: Option<Key>,
    content: String,
    style: TextStyle,
    layout: Layout,
}

impl Widget for Text {
    fn key(&self) -> Option<Key> { self.key.clone() }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(LeafElement::new())
    }
}
```

### Container Widgets

```rust
/// Column widget - vertical layout
pub struct Column {
    key: Option<Key>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}

impl Widget for Column {
    fn key(&self) -> Option<Key> { self.key.clone() }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ContainerElement::new())
    }
}
```

### Modifier Widgets

```rust
/// Padding modifier - wraps single child
pub struct Padding {
    key: Option<Key>,
    amount: f32,
    child: Box<dyn Widget>,
}

impl Widget for Padding {
    fn key(&self) -> Option<Key> { self.key.clone() }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ModifierElement::new())
    }
}
```

## Element Types

### Leaf Element

```rust
/// Element for leaf widgets (no children)
pub struct LeafElement {
    id: ElementId,
    widget: Box<dyn Widget>,
    state: Box<dyn Any>,
    render_object: Option<RenderObjectId>,
}

impl Element for LeafElement {
    fn mount(&mut self, parent: ElementId, context: &mut ElementContext) {
        self.id = context.next_element_id();
        self.render_object = Some(context.create_render_object(
            self.widget.create_render_object()
        ));
    }

    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext) {
        self.widget = new_widget;
        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
        }
        context.remove_state(self.id);
    }
}
```

### Container Element

```rust
/// Element for container widgets (multiple children)
pub struct ContainerElement {
    id: ElementId,
    widget: Box<dyn Widget>,
    children: Vec<ElementId>,
    render_object: Option<RenderObjectId>,
}

impl Element for ContainerElement {
    fn mount(&mut self, parent: ElementId, context: &mut ElementContext) {
        self.id = context.next_element_id();
        self.render_object = Some(context.create_render_object(
            self.widget.create_render_object()
        ));

        // Mount children
        for child_widget in self.widget.children() {
            self.children.push(context.mount_child(child_widget));
        }
    }

    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext) {
        let old_children = std::mem::take(&mut self.children);
        self.widget = new_widget;

        // Reconcile children
        context.reconcile_children(self.id, self.widget.children());

        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
        }
    }
}
```

## RenderObject Types

### TextRenderObject

```rust
pub struct TextRenderObject {
    content: String,
    style: TextStyle,
    computed_bounds: Bounds,
}

impl RenderObject for TextRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, ctx: &mut LayoutContext) -> Size {
        // Measure text using font system
        let size = ctx.measure_text(&self.content, &self.style);
        constraints.constrain(size)
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        vec![RenderCommand::text(
            self.computed_bounds.origin,
            &self.content,
            &self.style,
        )]
    }

    fn hit_test(&self, position: Point, _: &HitTestContext) -> HitTestResult {
        if self.computed_bounds.contains(position) {
            HitTestResult::hit()
        } else {
            HitTestResult::miss()
        }
    }
}
```

### ContainerRenderObject

```rust
pub struct ContainerRenderObject {
    children: Vec<RenderObjectId>,
    layout: Layout,
    computed_bounds: Bounds,
}

impl RenderObject for ContainerRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, ctx: &mut LayoutContext) -> Size {
        // Use Taffy for flexbox layout
        let mut total_size = Size::zero();

        for child in &self.children {
            let child_size = ctx.layout_child(*child, constraints);
            total_size = total_size.max(child_size);
        }

        constraints.constrain(total_size)
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Container doesn't paint itself, children do
        vec![]
    }

    fn children(&self) -> &[RenderObjectId] {
        &self.children
    }
}
```

## Input Handling

### Hit Testing

```rust
/// Hit test result
pub struct HitTestResult {
    /// Hit path from root to leaf
    path: Vec<ElementId>,
}

impl RenderObjectRegistry {
    /// Hit test from root, return path to leaf
    pub fn hit_test(&self, position: Point) -> HitTestResult {
        let mut path = Vec::new();
        if let Some(root) = self.root {
            self.hit_test_recursive(root, position, &mut path);
        }
        HitTestResult { path }
    }

    fn hit_test_recursive(
        &self,
        id: RenderObjectId,
        position: Point,
        path: &mut Vec<ElementId>
    ) -> bool {
        let obj = match self.get(id) {
            Some(o) => o,
            None => return false,
        };

        if obj.hit_test(position) {
            path.push(self.element_map[&id]);

            // Test children in reverse order (top-most first)
            for child in obj.children().iter().rev() {
                if self.hit_test_recursive(*child, position, path) {
                    return true;
                }
            }
            return true;
        }
        false
    }
}
```

### Event Dispatch

```rust
impl WindowState {
    fn handle_event(&mut self, event: InputEvent) {
        match &event {
            InputEvent::PointerButton { position, .. } |
            InputEvent::PointerMoved { position } => {
                // Hit test to find target element
                let hit_result = self.render_objects.hit_test(*position);

                // Dispatch to element path (bubble up)
                for element_id in hit_result.path.iter().rev() {
                    if let Some(element) = self.elements.get_mut(*element_id) {
                        let response = element.handle_event(&event, &mut self.context);
                        if response.handled {
                            break;
                        }
                    }
                }
            }
            InputEvent::Keyboard { .. } => {
                // Dispatch to focused element
                if let Some(focused) = self.focused_element {
                    self.elements.get_mut(focused).handle_event(&event, &mut self.context);
                }
            }
        }
    }
}
```

### Event Response

```rust
pub struct EventResponse {
    /// Was the event consumed
    pub handled: bool,

    /// Message to send to application
    pub message: Option<Message>,

    /// Request to change focus
    pub focus_request: Option<ElementId>,
}
```

## Error Handling

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ReconciliationError {
    #[error("Element {0} not found in registry")]
    ElementNotFound(ElementId),

    #[error("Widget type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: TypeId, actual: TypeId },

    #[error("Duplicate key: {0}")]
    DuplicateKey(Key),

    #[error("Render object {0} not found")]
    RenderObjectNotFound(RenderObjectId),
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("Layout engine error: {0}")]
    EngineError(#[from] crate::layout::LayoutError),

    #[error("Infinite layout detected for {0}")]
    InfiniteLayout(RenderObjectId),

    #[error("Constraint overflow: {constraints} exceeded by {size}")]
    ConstraintViolation { constraints: LayoutConstraints, size: Size },
}
```

### Recovery Strategies

```rust
impl ElementRegistry {
    fn reconcile_children(&mut self, parent: ElementId, widgets: Vec<Box<dyn Widget>>) {
        // Detect duplicate keys before modifying tree
        if let Err(e) = self.validate_keys(&widgets) {
            log::error!("Reconciliation failed: {}", e);
            return; // Keep existing tree intact
        }

        // Perform reconciliation with rollback on error
        let snapshot = self.snapshot();
        match self.reconcile_children_inner(parent, widgets) {
            Ok(()) => {}
            Err(e) => {
                log::error!("Reconciliation failed, rolling back: {}", e);
                self.rollback(snapshot);
            }
        }
    }
}
```

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_reconciliation_inserts_new_element() {
    let mut registry = ElementRegistry::new();

    let widget = Text::new("Hello");
    let root = registry.mount(Box::new(widget), None);

    assert_eq!(registry.count(), 1);

    let new_widget = Column::new()
        .push(Text::new("Hello"))
        .push(Text::new("World"));

    registry.reconcile_root(Box::new(new_widget));

    assert_eq!(registry.count(), 3);
}

#[test]
fn test_reconciliation_updates_matching_key() {
    let mut registry = ElementRegistry::new();

    let widget = Text::new("Hello").with_key("greeting");
    registry.mount(Box::new(widget), None);

    let new_widget = Text::new("Hello World").with_key("greeting");
    registry.reconcile_root(Box::new(new_widget));

    assert_eq!(registry.count(), 1);
}

#[test]
fn test_render_object_layout() {
    let mut obj = TextRenderObject::new("Hello", TextStyle::default());
    let constraints = LayoutConstraints::tight(Size::new(100.0, 50.0));
    let mut ctx = LayoutContext::mock();

    let size = obj.layout(constraints, &mut ctx);

    assert!(size.width <= 100.0);
    assert!(size.height <= 50.0);
}
```

### Integration Tests

```rust
#[test]
fn test_full_frame_flow() {
    let mut window = TestWindow::new();

    // First frame
    let widget = Column::new()
        .push(Text::new("First"))
        .push(Text::new("Second"));

    window.render_frame(widget);

    assert_eq!(window.element_count(), 3);
    assert_eq!(window.render_object_count(), 3);

    // Second frame - update text
    let widget = Column::new()
        .push(Text::new("First Updated"))
        .push(Text::new("Second"));

    window.render_frame(widget);

    // Only first text should have been repainted
    assert_eq!(window.repaint_count(), 1);
}
```

## Migration Path

### Phase 1: Core Infrastructure (Week 1-2)
- Implement `Key`, `ElementId`, `RenderObjectId` types
- Implement `ElementRegistry` with mount/update/unmount
- Implement `RenderObjectRegistry`
- Implement `StateStorage` for per-element state
- Implement reconciliation algorithm
- Unit tests for core types

### Phase 2: RenderObject Layer (Week 2-3)
- Implement `RenderObject` trait
- Implement `TextRenderObject`, `ContainerRenderObject`
- Integrate with existing `LayoutEngine` (Taffy)
- Implement dirty tracking
- Unit tests for layout/paint

### Phase 3: Element Layer (Week 3-4)
- Implement `Element` trait
- Implement `LeafElement`, `ContainerElement`, `ModifierElement`
- Implement event handling through element tree
- Unit tests for lifecycle

### Phase 4: Widget Layer (Week 4-5)
- Implement new `Widget` trait (immutable, cloneable)
- Port existing widgets: `Text`, `Button`, `TextEdit`, `Column`, `Row`
- Port modifiers: `Padding`, `Background`, `Border`
- Integration tests

### Phase 5: Application Integration (Week 5-6)
- Update `Application` trait to use new widgets
- Update `WindowState` to use three-tree pipeline
- Implement hit testing and event dispatch
- End-to-end tests

### Phase 6: Cleanup (Week 6)
- Remove old `Widget<M>` trait
- Remove old immediate-mode rendering code
- Update documentation
- Performance benchmarks

### Backward Compatibility
- Old `Widget<M>` trait remains during migration
- New widgets can be used alongside old during transition
- Final cutover removes old system

## Open Questions

1. **Animation support** - Should animations be handled at Element or RenderObject level?
2. **Scroll optimization** - How to handle large scrollable lists (virtualization)?
3. **Debugging tools** - Widget inspector, element tree visualization?

## References

- Flutter Architecture: https://docs.flutter.dev/ui/architecture
- Flutter Element class: https://api.flutter.dev/flutter/widgets/Element-class.html
- Flutter RenderObject: https://api.flutter.dev/flutter/rendering/RenderObject-class.html
