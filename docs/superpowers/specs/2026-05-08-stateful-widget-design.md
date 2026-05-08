# StatefulWidget for Vexo Retain Mode

**Date:** 2026-05-08
**Status:** Approved

## Overview

Implement Flutter-style stateful widgets for Vexo's retain mode, enabling widgets to maintain mutable state that persists across rebuilds.

## Goals

- Provide a simple, Rust-idiomatic API for stateful widgets
- State persists across widget tree rebuilds
- Minimal changes to existing architecture
- Easy to test and understand

## Non-Goals

- Full Flutter lifecycle hooks (`didUpdateWidget`, `didChangeDependencies`, etc.)
- `setState()` auto-rebuild convenience method
- InheritedWidget / dependency injection

## Design

### Core Trait

```rust
pub trait StatefulWidget: Sized + 'static {
    /// Mutable state that persists across rebuilds.
    type State: Default;

    /// Build the widget tree using current state.
    fn build(&self, state: &mut Self::State, ctx: &mut BuildContext) -> Box<dyn Widget>;
}
```

**Key decisions:**
- `State: Default` for simple initialization
- State passed explicitly to `build()` for clear ownership
- `BuildContext` provides rebuild trigger

### BuildContext

```rust
pub struct BuildContext<'a> {
    element_id: ElementId,
    state_storage: &'a mut StateStorage,
    dirty: &'a mut DirtyTracking,
    render_objects: &'a mut RenderObjectRegistry,
}

impl<'a> BuildContext<'a> {
    /// Request a rebuild of this element.
    pub fn request_rebuild(&mut self) {
        // Element will be rebuilt on next frame
    }

    /// Mark layout dirty for fine-grained updates.
    pub fn mark_needs_layout(&mut self) {
        // ...
    }

    /// Mark paint dirty for fine-grained updates.
    pub fn mark_needs_paint(&mut self) {
        // ...
    }
}
```

### StatefulElement

```rust
pub struct StatefulElement<W: StatefulWidget> {
    widget: W,
    element_id: ElementId,
    render_object_id: Option<RenderObjectId>,
    child_element_id: Option<ElementId>,
}
```

**Lifecycle:**

| Phase | Action |
|-------|--------|
| **mount** | 1. Create `State::default()` and store in `StateStorage`<br>2. Call `build()` to get child widget<br>3. Mount child element |
| **update** | 1. Update `widget` with new configuration<br>2. Retrieve state from storage<br>3. Call `build()` with new widget and existing state<br>4. Reconcile child element |
| **unmount** | 1. Remove state from `StateStorage`<br>2. Unmount child element |

**Key points:**
- StatefulElement is a wrapper - it doesn't render itself
- Single child - `build()` returns one widget tree
- State keyed by ElementId - persists as long as element exists

### Widget Trait Integration

StatefulWidget implementations also implement Widget for compatibility:

```rust
impl<W: StatefulWidget + Clone> Widget for W {
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
```

### Pipeline Integration

**Reconcile flow:**

```
Widget tree arrives
       │
       ▼
Pipeline.reconcile(root_widget)
       │
       ├─ Widget is StatefulWidget?
       │       │
       │       ▼
       │  StatefulElement exists with matching key/type?
       │       │
       │       ├─ Yes → update() with new widget, state preserved
       │       │
       │       └─ No → mount new StatefulElement, State::default()
       │
       ▼
StatefulElement.build(state, ctx)
       │
       ▼
Child widget reconciled with child element
```

**BuildOwner integration:**

```rust
impl BuildOwner {
    pub fn schedule_rebuild(&mut self, element_id: ElementId) {
        self.rebuild_queue.push(element_id);
    }

    pub fn perform_rebuilds(&mut self, registry: &mut ElementRegistry, ...) {
        while let Some(id) = self.rebuild_queue.pop() {
            if let Some(element) = registry.get_mut(id) {
                element.rebuild(...);
            }
        }
    }
}
```

## Example Usage

```rust
// Counter widget with persistent state
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

    fn build(&self, state: &mut CounterState, ctx: &mut BuildContext) -> Box<dyn Widget> {
        Column::new()
            .push(Text::new(format!("{}: {}", self.label, state.count)))
            .push(Button::new("Increment", || {
                state.count += 1;
                ctx.request_rebuild();
            }))
            .boxed()
    }
}
```

## Component Summary

| Component | Responsibility |
|-----------|----------------|
| `StatefulWidget` trait | Define state type and build logic |
| `StatefulElement` | Manage state lifecycle, delegate to child |
| `StateStorage` | Store type-erased state by ElementId (existing) |
| `BuildContext` | Provide rebuild trigger API |
| `BuildOwner` | Queue and execute scheduled rebuilds (existing) |

## Implementation Notes

1. **StateStorage already exists** - Reuse current implementation
2. **BuildOwner already exists** - Add `schedule_rebuild()` method
3. **StatefulElement is new** - Primary implementation work
4. **Widget impl for StatefulWidget** - Blanket implementation or derive macro

## Testing Strategy

1. **Unit tests for StatefulElement** - Test mount/update/unmount lifecycle
2. **Integration tests** - Test state persistence across rebuilds
3. **E2E test** - Counter example in demo app
