# Eliminate Vacate/Restore via Index-Based Access

**Date:** 2026-05-14
**Status:** Approved

## Problem

Vexo's retain mode uses a vacate/restore pattern for tree manipulation. Elements are removed from the `ElementRegistry` (SlotMap), operated on, then restored. This is error-prone: forgetting to restore after vacate leaks or loses the element. There are 11 instances across the codebase.

**Root cause:** Code needs `&mut element` (borrowed from the SlotMap) and `&mut ElementRegistry` (the SlotMap itself) simultaneously. Rust's borrow checker forbids this. Vacate/restore works around it by taking the element out.

**First-principles insight:** The vacate/restore pattern is index-based access done clumsily. The element is taken out so the registry can be used, then put back. The fix is to never hold `&mut element` across a call boundary — pass the `ElementKey` (which is `Copy`) and let the callee look up the element internally.

## Solution

Three changes that work together:

1. **`with_element`** — a borrow-safe accessor on `ElementRegistry` that replaces vacate/restore
2. **`ChildOps`** — a command pattern where elements emit child operations instead of executing them directly
3. **`ElementContext` redesign** — remove `Option` wrappers and `element_registry` field, add `child_ops`

### 1. `with_element` Accessor

```rust
impl ElementRegistry {
    pub fn with_element<C, R>(
        &mut self,
        key: ElementKey,
        context: &mut C,
        f: impl FnOnce(&mut Box<dyn Element>, &mut C) -> R,
    ) -> Option<R> {
        let element = self.slots.get_mut(key)?.as_mut()?;
        Some(f(element, context))
    }
}
```

`element` is borrowed from `self.slots`, `context` is a separate stack variable. Rust proves disjointness because they're different function parameters. The element never leaves the SlotMap.

Replaces all 6 Pattern 1 vacate/restore instances.

### 2. `ChildOps` Command Pattern

```rust
pub struct ChildOps {
    ops: Vec<ChildOp>,
}

pub enum ChildOp {
    Inflate {
        slot: Option<usize>,
        widget: Box<dyn Widget>,
        parent: ElementKey,
    },
    Update {
        child: ElementKey,
        widget: Box<dyn Widget>,
    },
    Unmount {
        child: ElementKey,
    },
}
```

Elements emit commands through `context.child_ops` instead of calling registry methods directly. After the element method returns, the pipeline drains `ChildOps` and executes them with full `&mut self` access.

Replaces all 5 Pattern 2 take/restore instances.

### 3. `ElementContext` Redesign

**Before:**
```rust
pub struct ElementContext<'a> {
    pub element_id: ElementKey,
    pub parent: Option<ElementKey>,
    pub render_object: Option<RenderObjectKey>,
    pub state: &'a mut StateStorage,
    pub dirty: &'a mut DirtyTracking,
    pub render_objects: Option<&'a mut RenderObjectRegistry>,  // take/restore
    pub element_registry: Option<&'a mut ElementRegistry>,      // root of vacate
    pub build_owner: Option<&'a BuildOwner>,
    pub dirty_sender: Option<&'a mpsc::Sender<ElementKey>>,
}
```

**After:**
```rust
pub struct ElementContext<'a> {
    pub element_id: ElementKey,
    pub parent: Option<ElementKey>,
    pub render_object: Option<RenderObjectKey>,
    pub state: &'a mut StateStorage,
    pub dirty: &'a mut DirtyTracking,
    pub render_objects: &'a mut RenderObjectRegistry,
    pub build_owner: &'a BuildOwner,
    pub dirty_sender: &'a mpsc::Sender<ElementKey>,
    pub child_ops: &'a mut ChildOps,
}
```

Changes:
- Removed `element_registry` — elements never access the registry directly
- Removed all `Option` wrappers — no more take/restore
- Added `child_ops` — accumulator for child operations

## Pipeline Execution Flow

### Rebuild

```rust
fn perform_rebuilds(&mut self) {
    self.drain_dirty_channel();
    if !self.build_owner.has_pending_rebuilds() { return; }

    self.build_owner.sort_dirty_by_depth(|id| self.element_registry.depth(id));
    let dirty_ids: Vec<ElementKey> = self.build_owner.drain_dirty_sorted();

    for element_id in dirty_ids {
        if !self.element_registry.contains(element_id) { continue; }
        if !self.build_owner.enter_build_scope(element_id) { continue; }

        let parent = self.element_registry.parent(element_id);
        let mut ctx = ElementContext::new(
            element_id, parent,
            &mut self.state, &mut self.dirty,
            &mut self.render_objects, &self.build_owner,
            &self.dirty_sender, &mut self.child_ops,
        );

        self.element_registry.with_element(element_id, &mut ctx, |element, ctx| {
            element.rebuild_from_state(ctx);
        });

        self.execute_child_ops();
        self.build_owner.exit_build_scope(element_id);
    }
}
```

### Mount

```rust
fn mount_element_tree(&mut self, parent: Option<ElementKey>, widget: Box<dyn Widget>) -> ElementKey {
    let element = widget.create_element();
    let element_key = self.element_registry.insert(element, parent);

    let mut ctx = ElementContext::new(
        element_key, parent,
        &mut self.state, &mut self.dirty,
        &mut self.render_objects, &self.build_owner,
        &self.dirty_sender, &mut self.child_ops,
    );
    self.element_registry.with_element(element_key, &mut ctx, |element, ctx| {
        element.mount(ctx);
    });
    self.execute_child_ops();

    element_key
}
```

### Unmount

```rust
fn unmount_element_tree(&mut self, element_id: ElementKey) {
    let children = self.element_registry.children(element_id).to_vec();
    for child_id in children {
        self.unmount_element_tree(child_id);
    }

    let parent = self.element_registry.parent(element_id);
    let mut ctx = ElementContext::new(
        element_id, parent,
        &mut self.state, &mut self.dirty,
        &mut self.render_objects, &self.build_owner,
        &self.dirty_sender, &mut self.child_ops,
    );
    self.element_registry.with_element(element_id, &mut ctx, |element, ctx| {
        element.unmount(ctx);
    });

    self.state.remove(element_id);
    self.element_registry.unmount(element_id);
}
```

### Execute Child Ops

```rust
fn execute_child_ops(&mut self) {
    let ops: Vec<ChildOp> = self.child_ops.drain();
    for op in ops {
        match op {
            ChildOp::Inflate { slot, widget, parent } => {
                let child_key = self.mount_element_tree(Some(parent), widget);
                self.element_registry.add_child(parent, child_key, slot);
            }
            ChildOp::Update { child, widget } => {
                self.rebuild_element(child, widget);
            }
            ChildOp::Unmount { child } => {
                self.unmount_element_tree(child);
            }
        }
    }
}
```

## Element Trait Changes

The trait signature stays the same. Only method bodies change — from direct registry calls to command emission.

**Example: ContainerElement::rebuild()**

Before:
```rust
fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
    for (i, child_widget) in new_children.iter().enumerate() {
        if let Some(existing) = self.children.get(i) {
            context.update_child(Some(*existing), child_widget.clone_boxed(), Some(i));
        } else {
            let new_id = context.inflate_widget(child_widget, Some(i));
            self.children.push(new_id);
        }
    }
    while self.children.len() > new_children.len() {
        if let Some(child) = self.children.pop() {
            context.unmount_child(child);
        }
    }
}
```

After:
```rust
fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
    for (i, child_widget) in new_children.iter().enumerate() {
        if let Some(existing) = self.children.get(i) {
            context.child_ops.update(*existing, child_widget.clone_boxed());
        } else {
            context.child_ops.inflate(Some(i), child_widget.clone_boxed(), context.element_id);
        }
    }
    while self.children.len() > new_children.len() {
        if let Some(child) = self.children.pop() {
            context.child_ops.unmount(child);
        }
    }
}
```

**How parents learn new child keys:** When an element emits `ChildOp::Inflate`, it doesn't know the new child's `ElementKey` yet (assigned when the pipeline executes the inflate). After the pipeline executes the inflate, it calls `element_registry.add_child(parent, child_key, slot)` to update the registry metadata. The element's internal `self.children` list is then updated via a second `with_element` call on the parent:

```rust
// In execute_child_ops, after inflating a child:
ChildOp::Inflate { slot, widget, parent } => {
    let child_key = self.mount_element_tree(Some(parent), widget);
    self.element_registry.add_child(parent, child_key, slot);
    // Notify parent element of new child key
    self.element_registry.with_element(parent, &mut (), |element, _| {
        element.child_mounted(child_key, slot);
    });
}
```

The `Element` trait gains an optional `child_mounted(&mut self, child: ElementKey, slot: Option<usize>)` method with a default no-op implementation. Elements that track children internally (e.g., `ContainerElement`) override it to push the key into `self.children`.

## ElementRegistry API Changes

**Removed:**
- `vacate(key)` / `restore(key, element)` / `VacatedElement`
- `mount_element(key, widget, context)` — lifecycle moves to pipeline
- `update_child(key, new_widget, slot, context)` — lifecycle moves to pipeline
- `inflate_widget(widget, parent, context)` — lifecycle moves to pipeline

**Added:**
- `with_element(key, context, f)` — borrow-safe accessor
- `insert(element, parent)` — insert into SlotMap + set metadata (no lifecycle call)
- `add_child(parent, child, slot)` — update children metadata after pipeline inflates a child

**Kept as-is:**
- `get(key)` / `get_mut(key)`
- `parent(key)` / `children(key)` / `depth(key)` / `contains(key)`
- `unmount(key)` — remove from SlotMap + clean metadata
- `root()` / `set_root(key)`

## Render Object Tree

No changes. The render object tree doesn't use vacate/restore. Layout, paint, and hit testing use immutable borrows. `update()` uses `get_mut()` on a single render object with no registry-level conflict.

## Files Changed

| File | Change |
|------|--------|
| `retain/element.rs` | Remove vacate/restore. Add with_element. Remove lifecycle methods. Add insert/add_child. |
| `retain/element_context.rs` | Remove Option wrappers. Remove element_registry. Add child_ops. Remove take/restore methods. |
| `retain/pipeline.rs` | Add child_ops field. Add execute_child_ops. Replace vacate/restore with with_element. Move lifecycle logic here. |
| `retain/child_ops.rs` | **New.** ChildOps struct and ChildOp enum. |
| `retain/elements/container.rs` | Replace context.inflate_widget/update_child/unmount_child with child_ops.*. |
| `retain/elements/single_child.rs` | Same as container. |
| `retain/elements/multi_child.rs` | Same as container. |
| `retain/elements/leaf.rs` | No change. |
| `retain/elements/render_object_element.rs` | Replace registry access with child_ops where needed. Implement `child_mounted`. |
| `retain/stateful_widget.rs` | Remove take/restore. Replace registry calls with child_ops. |
| `retain/element.rs` (trait) | Add optional `child_mounted` method with default no-op. |
| `retain/mod.rs` | Add mod child_ops. |

## Vacate/Restore Instances Eliminated

| # | Location | Pattern | Solution |
|---|----------|---------|----------|
| 1 | pipeline::rebuild_root | vacate/restore | with_element |
| 2 | pipeline::perform_rebuilds | vacate/restore | with_element |
| 3 | pipeline::reconcile_element | vacate/restore | with_element |
| 4 | pipeline::unmount_element_tree | vacate/restore | with_element |
| 5 | element_registry::mount_element | inline vacate | pipeline-level mount |
| 6 | element_registry::update_child | inline vacate | pipeline-level update |
| 7 | element_context::inflate_widget | take/restore | child_ops.inflate |
| 8 | element_context::update_child | take/restore | child_ops.update |
| 9 | element_context::unmount_child | take/restore | child_ops.unmount |
| 10 | stateful_widget::rebuild | take/restore | direct field access |
| 11 | stateful_widget::update | take/restore | direct field access |

All 11 instances eliminated. No vacate, no restore, no leak risk.
