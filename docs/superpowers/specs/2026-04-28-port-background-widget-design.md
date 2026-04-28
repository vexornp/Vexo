# Port Background Widget to Retain-Mode Design

**Date:** 2026-04-28
**Status:** Design Approved

## Goal

Port the `Background` modifier widget from immediate-mode to retain-mode as the first modifier widget in the three-tree architecture.

## Why Background

- Simplest modifier (just draws a colored rect behind child)
- Single child wrapper (matches existing retain widget patterns)
- Establishes pattern for other modifiers (Border, CornerRadius)

## Architecture

```
Background widget (immutable, rebuilt each frame)
    │
    │ create_element()
    ▼
ModifierElement (stateful, persistent)
    │
    │ create_render_object()
    ▼
BackgroundRenderObject (layout/paint, persistent)
    │
    │ children()
    ▼
Child RenderObject(s)
```

## Components

### 1. Background Widget

**File:** `vexo/src/retain/widgets/background.rs`

```rust
pub struct Background {
    key: Option<Key>,
    child: Box<dyn Widget>,
    color: Color,
}

impl Background {
    pub fn new(child: Box<dyn Widget>, color: Color) -> Self
    pub fn with_key(mut self, key: impl Into<Key>) -> Self
}

impl Widget for Background {
    fn key(&self) -> Option<Key>
    fn create_element(&self) -> Box<dyn Element>  // ModifierElement
    fn create_render_object(&self) -> Box<dyn RenderObject>  // BackgroundRenderObject
    fn clone_box(&self) -> Box<dyn Widget>
    fn as_any(&self) -> &dyn Any
}
```

### 2. ModifierElement

**File:** `vexo/src/retain/elements/modifier.rs` (already exists, needs update)

The existing ModifierElement needs to:
- Store the child widget
- Create child element on mount
- Update child on update
- Create render object on mount
- Manage child element lifecycle

```rust
pub struct ModifierElement {
    id: Option<ElementId>,
    key: Option<Key>,
    widget: Option<Box<dyn Widget>>,
    child_element: Option<ElementId>,
    render_object: Option<RenderObjectId>,
}

impl Element for ModifierElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Create own ID and render object
        // Create child element from widget's child
    }
    
    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext) {
        // Store new widget
        // Update child element
        // Mark render object dirty
    }
    
    fn unmount(&mut self, context: &mut ElementContext) {
        // Remove child element
        // Remove render object
    }
}
```

### 3. BackgroundRenderObject

**File:** `vexo/src/retain/widgets/background.rs` (inline) or `vexo/src/retain/render_objects/background.rs`

```rust
pub struct BackgroundRenderObject {
    color: Color,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl RenderObject for BackgroundRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, ctx: &mut LayoutContext) -> Size<Logical> {
        // Layout child first, then use child's size
        // Store computed bounds
    }
    
    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Return background rect command
        // Child paints separately via tree traversal
    }
    
    fn hit_test(&self, position: Point<Logical>, ctx: &HitTestContext) -> bool {
        // Check bounds
    }
    
    fn children(&self) -> &[RenderObjectId] {
        // Return child if present
    }
}
```

## Data Flow

1. **Mount:**
   - Background widget → create ModifierElement
   - ModifierElement.mount() → create BackgroundRenderObject + child element
   - Child element → create child render object

2. **Update:**
   - New Background widget → ModifierElement.update()
   - Update child element with new child widget
   - Mark render objects dirty

3. **Layout:**
   - BackgroundRenderObject.layout() → layout child, use child's size

4. **Paint:**
   - BackgroundRenderObject.paint() → return background rect command
   - Pipeline paints children after parent

## Files to Create

- `vexo/src/retain/widgets/background.rs` - Background widget + BackgroundRenderObject

## Files to Modify

- `vexo/src/retain/widgets/mod.rs` - Add Background export
- `vexo/src/retain/elements/modifier.rs` - Update to handle child widgets
- `vexo/src/retain/mod.rs` - Export Background

## Tests

- `test_background_widget_creation` - Verify widget construction
- `test_background_with_key` - Verify key handling
- `test_background_render_object_paint` - Verify paint returns rect command
- `test_background_layout_delegates_to_child` - Verify layout uses child size

## Out of Scope

- Border, CornerRadius modifiers (future work)
- Integration with TaffyLayoutEngine (uses mock for now)
- Full event handling (modifiers delegate to child)