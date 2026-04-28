# Taffy Integration for Retain-Mode Layout Design

**Date:** 2026-04-28
**Status:** Design Approved

## Goal

Integrate Taffy layout engine into retain-mode pipeline so RenderObjects create and manage their own Taffy nodes, enabling proper two-pass layout (constraints down, sizes up).

## Architecture

Each RenderObject creates its own Taffy node(s) during layout:

```
RenderObject::layout()
    ↓
Creates Taffy node with style
    ↓
Recursively layouts children (passing Taffy engine)
    ↓
Taffy computes layout
    ↓
RenderObject applies computed bounds
```

This follows Flutter's pattern where each render object participates in layout.

## Components

### 1. LayoutContext Enhancement

Add Taffy engine and font system access to LayoutContext:

```rust
pub struct LayoutContext<'a> {
    engine: &'a mut dyn LayoutEngine,
    font_system: &'a mut FontSystem,
}

impl<'a> LayoutContext<'a> {
    pub fn engine(&mut self) -> &mut dyn LayoutEngine {
        self.engine
    }
    
    pub fn font_system(&mut self) -> &mut FontSystem {
        self.font_system
    }
}
```

### 2. LayoutResult Type

Return value from RenderObject::layout():

```rust
pub struct LayoutResult {
    /// The Taffy node ID for this render object
    node: LayoutNodeId,
    /// The computed size (after Taffy computation)
    size: Size<Logical>,
}
```

### 3. RenderObject Trait Update

```rust
pub trait RenderObject {
    /// Perform layout, creating Taffy node(s).
    /// Returns the LayoutResult with node ID and size.
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult;
    
    /// Apply computed layout from Taffy.
    /// Called after Taffy::compute() to read back results.
    fn apply_layout(&mut self, ctx: &LayoutContext);
    
    // ... other methods unchanged
}
```

### 4. Widget Type Behaviors

**Text (leaf node):**
```rust
fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
    let measure_ctx = MeasureContext::Text(TextMeasureContext {
        content: self.content.clone(),
        font_size: self.font_size,
        line_height: 1.2,
    });
    
    let node = ctx.engine().create_leaf_with_context(
        &Layout::default(),
        measure_ctx,
    );
    
    LayoutResult { node, size: Size::new(0.0, 0.0) }
}

fn apply_layout(&mut self, ctx: &LayoutContext) {
    if let Some(computed) = ctx.engine().get_layout(self.layout_node) {
        self.computed_bounds = Some(computed.bounds);
    }
}
```

**Container (Column/Row):**
```rust
fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
    // Layout children first
    let child_nodes: Vec<LayoutNodeId> = self.children.iter_mut()
        .map(|child| child.layout(ctx).node)
        .collect();
    
    // Create container node with children
    let style = Layout::default()
        .flex_direction(self.direction)
        .gap(self.gap);
    
    let node = ctx.engine().create_container(&style, &child_nodes);
    
    LayoutResult { node, size: Size::new(0.0, 0.0) }
}
```

**Modifiers (Background, Border, CornerRadius):**
```rust
fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
    // Pass through to child - modifiers don't affect layout
    if let Some(child) = &mut self.child {
        child.layout(ctx)
    } else {
        // No child, create empty leaf
        let node = ctx.engine().create_leaf(&Layout::default());
        LayoutResult { node, size: Size::new(0.0, 0.0) }
    }
}

fn apply_layout(&mut self, ctx: &LayoutContext) {
    // Apply to self (use child's bounds)
    if let Some(child) = &mut self.child {
        child.apply_layout(ctx);
        self.computed_bounds = child.computed_bounds;
    }
}
```

### 5. Pipeline Layout Flow

```rust
pub fn layout(&mut self, available_size: Size<Logical>, engine: &mut dyn LayoutEngine, font_system: &mut FontSystem) {
    let root_id = match self.render_objects.root() {
        Some(id) => id,
        None => return,
    };
    
    // Phase 1: Build Taffy tree (creates nodes)
    let mut ctx = LayoutContext::new(engine, font_system);
    let result = self.render_objects.get_mut(root_id).unwrap().layout(&mut ctx);
    
    // Phase 2: Compute layout with Taffy
    engine.compute(result.node, available_size, font_system);
    
    // Phase 3: Apply computed layouts back to render objects
    self.apply_layout_recursive(root_id, &ctx);
    
    // Clear dirty flags
    self.dirty.drain_layout().for_each(drop);
}
```

## Files to Modify

1. **vexo/src/retain/render_object.rs**
   - Add LayoutContext with engine/font_system
   - Add LayoutResult type
   - Update RenderObject trait signature

2. **vexo/src/retain/pipeline.rs**
   - Update layout() to use Taffy engine
   - Add apply_layout_recursive()
   - Remove hacky bottom-up code

3. **vexo/src/retain/render_objects/text.rs**
   - Implement layout() with text measure context
   - Implement apply_layout()

4. **vexo/src/retain/widgets/background.rs**
   - Update BackgroundRenderObject for new trait
   - Pass-through to child

5. **vexo/src/retain/widgets/border.rs**
   - Update BorderRenderObject for new trait
   - Pass-through to child

6. **vexo/src/retain/widgets/corner_radius.rs**
   - Update CornerRadiusRenderObject for new trait
   - Pass-through to child

7. **vexo/src/retain/widgets/container.rs**
   - Update ContainerRenderObject for new trait
   - Create container node with children

## Success Criteria

1. Text widgets get accurate intrinsic size from font measurement
2. Column/Row containers lay out children with proper flex behavior
3. Modifier widgets (Background, Border) size to their content
4. Gap/spacing works correctly in containers
5. Layout is computed once per frame, not per-object
6. Existing immediate-mode layout continues working unchanged

## Out of Scope

- Scroll view layout
- Grid layout
- Absolute positioning
- Aspect ratio constraints
