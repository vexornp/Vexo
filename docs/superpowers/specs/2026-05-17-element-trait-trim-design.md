# Element Trait Trim Design

**Date:** 2026-05-17
**Status:** Approved

## Problem

The `Element` trait has 13 methods, but 3 of them are dead code — never called by production code on the trait object. This bloats the trait interface and forces every implementation to provide boilerplate.

## Scope

Minimal trim: remove dead methods only. No structural changes to the trait hierarchy.

## Methods Being Removed

### `add_child(&mut self, _child_key: ElementKey)`

**Why dead:** Never called on the trait object. The reconciler calls `element_registry.add_child()` (the `ElementRegistry` method), not `element.add_child()` (the trait method). The registry already tracks parent-child relationships via `children_map`. The trait method duplicates this with per-element storage.

**Current implementations:**
- LeafRenderObjectElement: no-op (default)
- ContainerElement: pushes to `self.children` — but the registry also tracks this
- DecoratedContainerElement: sets `self.child_element` — redundant with registry
- GestureDetectorElement: sets `self.child_element` — redundant with registry
- StatefulElement: no-op (default)

### `has_children(&self) -> bool`

**Why dead:** Zero production callers. The reconciler never checks this. It was likely intended for pipeline logic that now uses the registry's `children()` method instead.

**Current implementations:**
- LeafRenderObjectElement: returns `false` (default)
- ContainerElement: returns `true`
- DecoratedContainerElement: returns `self.child_element.is_some()`
- GestureDetectorElement: returns `self.child_element.is_some()`
- StatefulElement: returns `self.child_element_id.is_some()`

### `visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element))`

**Why dead:** Only called in 1 test (`leaf.rs:307`). `ElementRegistry::children()` already provides equivalent traversal. All implementations just iterate the registry's children list — no custom logic.

**Current implementations:**
- LeafRenderObjectElement: no-op
- ContainerElement: iterates `self.children`, visits each from registry
- DecoratedContainerElement: visits single `self.child_element`
- GestureDetectorElement: visits single `self.child_element`
- StatefulElement: visits single `self.child_element_id`

## Methods Staying on Element

| Method | Reason |
|--------|--------|
| `mount` | Core lifecycle, called by reconciler |
| `update` | Core lifecycle, called by reconciler |
| `unmount` | Core lifecycle, called by reconciler |
| `rebuild` | Called by reconciler, has meaningful default (delegates to `update`) |
| `can_update` | Called by reconciler to decide update vs. replace |
| `render_object` | Called by reconciler to link render object tree |
| `widget_key` | Called by reconciler for key-based matching |
| `child_mounted` | Called by reconciler after inflate (keep with TODO) |
| `rebuild_from_state` | Called by reconciler in `perform_rebuilds()` |
| `on_event` | Called by EventHandler |

## Changes

### Files to modify

1. **`vexo/src/retain/element.rs`**: Remove `add_child`, `has_children`, `visit_children` from the `Element` trait. Add TODO comment on `child_mounted` noting overlap with `rebuild()`.

2. **`vexo/src/retain/elements/leaf.rs`**: Remove `visit_children` impl from `LeafRenderObjectElement`. Remove `test_leaf_element_no_children` test.

3. **`vexo/src/retain/elements/container.rs`**: Remove `add_child`, `has_children`, `visit_children` impls from `ContainerElement`.

4. **`vexo/src/retain/widgets/decorated_container.rs`**: Remove `add_child`, `has_children`, `visit_children` impls.

5. **`vexo/src/retain/widgets/gesture_detector.rs`**: Remove `add_child`, `has_children`, `visit_children` impls.

6. **`vexo/src/retain/stateful_widget.rs`**: Remove `has_children`, `visit_children` impls from `StatefulElement`.

7. **`vexo/src/retain/element_registry_tests.rs`**: Remove `visit_children` from `MockElement` impl.

8. **`vexo/src/retain/reconcile_tests.rs`**: Remove `visit_children` from `MockElement` impl.

9. **`vexo/src/retain/reconcile.rs`** (test module): Remove `visit_children` from `MockElement` impl.

10. **`vexo/src/retain/widgets/mod.rs`** (test module): Remove `visit_children` from `TestElement` impl.

### Not changing

- No structural changes to `RenderObjectElement`, `SingleChildRenderObjectElement`, `MultiChildRenderObjectElement`
- No changes to reconciler, event handler, or pipeline
- `on_event` and `rebuild_from_state` stay on Element (they have real callers)

## Verification

- `cargo build -p vexo` must pass
- `cargo test -p vexo` must pass
