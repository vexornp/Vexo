# Eliminate Phase 2 Hit Check: Render Proxy Pattern Design

## Problem

Vexo's event dispatch uses a two-phase hit check:

- **Phase 1**: Hit test on the render tree, bubble events through elements in the hit path (deepest to shallowest)
- **Phase 2**: Walk up the element tree parent chain to find wrapper elements (StatefulElement) not in the render path

Phase 2 exists because `StatefulElement` creates an `EmptyRenderObject` that always returns `false` from `hit_test()`, making it invisible in the render tree. But StatefulElement still needs pointer events for focus management (e.g., TextEdit click-to-focus).

This two-phase approach has problems:
- Walks two trees for every pointer event (render tree + element tree)
- The boundary between "render tree elements" and "wrapper elements" is implicit
- Phase 2 is O(depth) per event, with no guarantee of correctness for nested wrappers
- The design is inconsistent with Flutter's proven architecture

## Flutter's Approach

Flutter hit tests **only on the render tree**. ComponentElements (StatefulElement, ProxyElement) have no render objects and are invisible to hit testing. They are "skipped over" during `attachRenderObject()` — a RenderObjectElement's render object connects directly to the nearest ancestor RenderObjectElement's render object, compressing the render tree.

Wrapper behavior gets into the render tree via **RenderProxyBox** subclasses:
- `Opacity` widget → `RenderAnimatedOpacity` (a RenderProxyBox)
- `IgnorePointer` widget → `RenderIgnorePointer` (a RenderProxyBox)
- `Listener` widget → `RenderPointerListener` (a RenderProxyBox)

These proxy render objects sit in the render tree, delegate layout/paint to their child, and can intercept `hitTest()` and `handleEvent()`. No Phase 2 needed.

Flutter's focus system is **completely separate** from hit testing — `FocusNode`/`FocusManager` form a parallel focus tree, and focus is requested programmatically (not via event dispatch).

## Proposed Design: Render Proxy Pattern

Replace `EmptyRenderObject` with a `ProxyRenderObject` that sits in the render tree between the parent and child. This makes StatefulElement visible to hit testing and eliminates Phase 2.

### 1. ProxyRenderObject

A new render object type that replaces `EmptyRenderObject`:

```rust
pub struct ProxyRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

Behavior:
- **Layout**: Pass-through. Creates a container node wrapping the child, same as `GestureDetectorRenderObject`
- **Paint**: No commands (invisible, same as EmptyRenderObject)
- **Hit test**: Uses computed bounds (same as GestureDetectorRenderObject). Returns `true` if pointer is inside bounds
- **Children**: Single child (the child element's render object)

This is essentially the same as `GestureDetectorRenderObject` — a pass-through render object that participates in the render tree for hit testing and layout, but has no visual representation.

### 2. StatefulElement Changes

StatefulElement currently:
1. Creates `EmptyRenderObject` (always `hit_test() = false`)
2. Delegates `render_object_id` to child via `child_mounted()` (line 459)

With ProxyRenderObject:
1. Creates `ProxyRenderObject` (has proper `hit_test()`, participates in render tree)
2. Links child's render object as a child of the ProxyRenderObject (same pattern as `GestureDetectorElement.child_mounted()`)
3. `render_object_id` is the ProxyRenderObject itself (not delegated to child)

This means StatefulElement's render object appears in the hit test path naturally. No Phase 2 needed.

### 3. Event Handler Changes

Remove Phase 2 from `EventHandler::handle_pointer_event()`:

```rust
// Before: two phases
// Phase 1: bubble through hit test path (deepest to shallowest)
// Phase 2: walk up element tree parent chain for wrapper elements

// After: single phase
// Bubble through hit test path only (deepest to shallowest)
// All elements are in the path because ProxyRenderObject is in the render tree
```

The event handler becomes a simple single-phase bubbling:
1. Hit test on render tree → get path
2. Bubble from deepest element to shallowest
3. First handler that returns a message stops propagation

### 4. Focus Management in StatefulElement

StatefulElement's `on_event()` currently handles focus:

```rust
// Pointer press inside bounds → request focus
if context.is_pointer_inside() {
    context.request_focus(id);
    return Some(Box::new(()));
}
```

This stays the same, but now it works **via Phase 1** instead of Phase 2. The ProxyRenderObject is in the render tree, so StatefulElement appears in the hit test path. When the event bubbles up to StatefulElement, it can request focus.

### 5. Render Tree Structure Change

Before (with EmptyRenderObject):
```
Render Tree:  ContainerRO → TextRO
Element Tree: ColumnElement → StatefulElement → LeafElement

Hit test path: [ContainerRO, TextRO]
Element path:  [ColumnElement, LeafElement]
StatefulElement is MISSING from both paths → Phase 2 needed
```

After (with ProxyRenderObject):
```
Render Tree:  ContainerRO → ProxyRO → TextRO
Element Tree: ColumnElement → StatefulElement → LeafElement

Hit test path: [ContainerRO, ProxyRO, TextRO]
Element path:  [ColumnElement, StatefulElement, LeafElement]
StatefulElement is IN the path → Phase 2 eliminated
```

### 6. child_mounted() Change

Current StatefulElement `child_mounted()`:
```rust
fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, _context: &mut ElementContext) {
    self.render_object_id = child_ro;  // Delegates to child's RO
}
```

New StatefulElement `child_mounted()` (same pattern as GestureDetectorElement):
```rust
fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
    if let Some(child_ro_key) = child_ro {
        self.insert_child_render_object(child_ro_key, context);  // Link as child of ProxyRO
    }
}
```

StatefulElement needs to implement `RenderObjectElement` and `SingleChildRenderObjectElement` traits (same as GestureDetectorElement). This gives it:
- `mount_render_object()` — creates ProxyRenderObject during mount
- `update_render_object()` — updates render object properties during update
- `unmount_render_object()` — removes render object during unmount
- `insert_child_render_object()` — links child's render object as child of ProxyRenderObject

The key change: `render_object_id` is now the ProxyRenderObject itself (not delegated to child).

### 7. Impact on Existing Render Objects

| Render Object | Change |
|---|---|
| `EmptyRenderObject` | Removed (replaced by ProxyRenderObject) |
| `GestureDetectorRenderObject` | No change (already a proxy pattern) |
| `ContainerRenderObject` | No change |
| `TextRenderObject` | No change |
| `DecoratedContainerRenderObject` | No change |

### 8. Impact on Existing Elements

| Element | Change |
|---|---|
| `StatefulElement` | Creates ProxyRenderObject instead of EmptyRenderObject; links child via `insert_child_render_object()` instead of delegating `render_object_id` |
| `GestureDetectorElement` | No change (already uses proxy pattern) |
| `ContainerElement` | No change |
| `LeafRenderObjectElement` | No change |
| `DecoratedContainerElement` | No change |

### 9. Future Benefits

The ProxyRenderObject pattern opens the door for future render-tree behaviors:

- **IgnorePointer**: A ProxyRenderObject that blocks hit testing to its subtree
- **Opacity hit testing**: A ProxyRenderObject that checks opacity before claiming hits
- **Transform**: A ProxyRenderObject that transforms hit test coordinates
- **Focus proxy**: A ProxyRenderObject that requests focus on pointer down (separate from StatefulElement)

This matches Flutter's extensibility — any wrapper behavior can be added as a ProxyRenderObject subclass without changing the hit testing algorithm.

## Implementation Steps

1. Create `ProxyRenderObject` (pass-through layout, invisible paint, bounds-based hit test)
2. Modify `StatefulElement::mount()` to create ProxyRenderObject instead of EmptyRenderObject
3. Modify `StatefulElement::child_mounted()` to link child's render object as child of ProxyRenderObject
4. Ensure `StatefulElement` implements `SingleChildRenderObjectElement` trait (or equivalent)
5. Remove Phase 2 from `EventHandler::handle_pointer_event()`
6. Remove `EmptyRenderObject`
7. Update tests (hit test, event dispatch, StatefulElement integration)
8. Verify TextEdit click-to-focus still works via Phase 1 bubbling

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| ProxyRenderObject adds more render objects to the tree | Each StatefulElement adds one lightweight proxy — same cost as GestureDetectorRenderObject which already works |
| Layout pass-through might affect Taffy tree depth | ProxyRenderObject creates a container node wrapping the child, same as GestureDetector — no extra layout cost |
| StatefulElement's `on_event()` focus logic must work via Phase 1 | Test thoroughly with TextEdit click-to-focus scenario |
| Nested StatefulElements create nested ProxyRenderObjects | Each appears in hit test path naturally — bubbling handles them correctly |

## What We're Not Doing

- **Not building a separate FocusManager system** (like Flutter's FocusNode/FocusManager). That's a larger change. The ProxyRenderObject pattern lets StatefulElement handle focus via Phase 1 bubbling, which is sufficient for now.
- **Not changing GestureDetector** — it already uses the proxy pattern correctly.
- **Not changing keyboard event dispatch** — it already dispatches directly to the focused element, no hit testing involved.