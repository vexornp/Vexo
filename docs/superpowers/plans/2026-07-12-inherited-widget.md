# InheritedWidget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Flutter-`InheritedWidget`-equivalent primitive to Vexo that exposes immutable values to all descendants, plus a `Theme` built-in that proves the ergonomic `Theme::of(ctx)` lookup pattern.

**Architecture:** A new `InheritedWidget` trait (separate from `Component`) backs an `InheritedElement` that registers itself in a pipeline-owned `InheritedRegistry` under `TypeId::of::<V>()`. Each element carries a cached `InheritedMap` (nearest-ancestor map, `Arc`-cloned from parent, copy-on-write at providers) so render-time lookups are O(1) and don't touch the element tree. Dependents register on first lookup; when a provider's value changes, it marks dependents dirty via the existing `BuildOwner` rebuild machinery.

**Tech Stack:** Rust, slotmap, `std::any::{Any, TypeId}`, `std::cell::RefCell`, `std::sync::Arc`, `std::collections::{HashMap, HashSet}`.

## Global Constraints

- No changes to the `Element` trait, `Widget` trait, `RenderObject` trait, `BuildOwner`, `ElementRegistry`, or any existing widget/element.
- `InheritedWidget` is a **separate trait** from `Component`; a blanket `impl<T: InheritedWidget> Widget for T` bridges them.
- Values are immutable; changes happen via ancestor rebuild, never in-place mutation.
- Whole-value dependency only (no aspect-based `InheritedModel`).
- `InheritedRegistry` uses `RefCell` interior mutability (same pattern as `BuildOwner`); borrowed as `&InheritedRegistry` from both `ElementContext` and `RenderContext`.
- Vexo never reparents; the per-element `InheritedMap` is built top-down at mount and never invalidated post-mount.
- Every task ends with `cargo build -p vexo` passing and `cargo test -p vexo` passing; commit after each task.

**Spec:** `docs/superpowers/specs/2026-07-12-inherited-widget-design.md`

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| `vexo/src/inherited_registry.rs` | Create | `InheritedRegistry` (provider values + dependents, `RefCell` interior mutability) and `InheritedMap` (per-element nearest-ancestor cache). |
| `vexo/src/inherited_widget.rs` | Create | `InheritedWidget` trait, `InheritedElement`, blanket `impl<T: InheritedWidget> Widget for T`. |
| `vexo/src/widgets/theme.rs` | Create | `Theme` widget, `ThemeData` value type, `Theme::of(ctx)` lookup. |
| `vexo/src/widgets/mod.rs` | Modify | Register `theme` submodule, re-export `Theme`, `ThemeData`. |
| `vexo/src/element_context.rs` | Modify | Add `inherited_map: &InheritedMap` and `inherited_registry: &InheritedRegistry` fields; extend `ElementContext::new()`. |
| `vexo/src/stateful_widget.rs` | Modify | Add same two fields to `RenderContext`; thread them through `StatefulElement::build_child_widget()` and its three call sites (mount/update/rebuild_from_state). Add `RenderContext::depend_on_inherited_widget::<V>()`. |
| `vexo/src/reconciler.rs` | Modify | At each of the 7 `ElementContext::new()` call sites: compute `inherited_map` from parent and pass both new args. |
| `vexo/src/elements/leaf.rs` | Modify | At each of the 4 `ElementContext::new()` call sites: pass empty `InheritedMap` and the registry (leaf-element test helpers only). |
| `vexo/src/pipeline.rs` | Modify | Add `inherited_registry: InheritedRegistry` and `inherited_maps: SecondaryMap<ElementKey, Arc<InheritedMap>>` fields to `ThreeTreePipeline`; thread to reconciler. Clear `inherited_maps` on unmount. |
| `vexo/src/lib.rs` | Modify | `mod inherited_registry; mod inherited_widget;` and re-export `InheritedWidget`, `Theme`, `ThemeData`. |
| `vexo/src/inherited_integration_test.rs` | Create | Element-level integration tests (mount/update/unmount, dependency, nested providers, fallback). |

---

### Task 1: `InheritedMap` — the per-element nearest-ancestor cache

**Files:**
- Create: `vexo/src/inherited_registry.rs`
- Modify: `vexo/src/lib.rs` (add `mod inherited_registry;`)

**Interfaces:**
- Consumes: `crate::id::ElementKey`
- Produces: `InheritedMap` struct with: `InheritedMap::empty() -> Self`, `get(&self, type_id: TypeId) -> Option<ElementKey>`, `with_insert(type_id: TypeId, key: ElementKey) -> Self` (returns a new map, COW-style), `Clone` (cheap, few entries).

- [ ] **Step 1: Write the failing test**

Create `vexo/src/inherited_registry.rs` with just the test:

```rust
//! Per-element nearest-ancestor cache for inherited values.
//!
//! Each element holds an `Arc<InheritedMap>` built at mount by cloning the
//! parent's map (cheap: few entries) and inserting self if the element is an
//! `InheritedElement`. Lookups are O(1) `HashMap` reads.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use crate::id::ElementKey;

/// Nearest-ancestor cache: `TypeId` of the exposed value → provider element.
///
/// Built top-down at mount. Never mutated post-mount (only swapped wholesale
/// on rebuild of an ancestor provider). Vexo never reparents, so the map is
/// always consistent with tree position.
#[derive(Clone, Default)]
pub struct InheritedMap {
    inner: HashMap<TypeId, ElementKey>,
}

impl InheritedMap {
    /// Empty map (used by root and by descendants with no providers).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Look up the nearest provider element for value type `V`.
    pub fn get(&self, type_id: TypeId) -> Option<ElementKey> {
        self.inner.get(&type_id).copied()
    }

    /// Return a new map with `type_id → key` inserted. Used by
    /// `InheritedElement::mount` to produce the map its subtree will see.
    pub fn with_insert(&self, type_id: TypeId, key: ElementKey) -> Self {
        let mut clone = self.clone();
        clone.inner.insert(type_id, key);
        clone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    #[test]
    fn empty_map_returns_none() {
        let map = InheritedMap::empty();
        assert_eq!(map.get(TypeId::of::<u32>()), None);
    }

    #[test]
    fn with_insert_then_get() {
        let k = make_key();
        let map = InheritedMap::empty().with_insert(TypeId::of::<u32>(), k);
        assert_eq!(map.get(TypeId::of::<u32>()), Some(k));
    }

    #[test]
    fn with_insert_does_not_mutate_original() {
        let k = make_key();
        let base = InheritedMap::empty();
        let _child = base.with_insert(TypeId::of::<u32>(), k);
        // Original is unchanged (COW).
        assert_eq!(base.get(TypeId::of::<u32>()), None);
    }

    #[test]
    fn with_insert_overrides_existing_type() {
        let k1 = make_key();
        let k2 = make_key();
        let map = InheritedMap::empty()
            .with_insert(TypeId::of::<u32>(), k1)
            .with_insert(TypeId::of::<u32>(), k2);
        // Nearest ancestor wins (last insert).
        assert_eq!(map.get(TypeId::of::<u32>()), Some(k2));
    }
}
```

- [ ] **Step 2: Register the module in `lib.rs`**

In `vexo/src/lib.rs`, after the `mod global_key_registry;` line (around line 69), add:

```rust
mod inherited_registry;
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test -p vexo inherited_registry::tests -- --nocapture`
Expected: PASS (4 tests).

- [ ] **Step 4: Build the whole crate to catch any breakage**

Run: `cargo build -p vexo`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/inherited_registry.rs vexo/src/lib.rs
git commit -m "feat(inherited): add InheritedMap per-element nearest-ancestor cache"
```

---

### Task 2: `InheritedRegistry` — provider values + dependents

**Files:**
- Modify: `vexo/src/inherited_registry.rs` (append)
- Modify: `vexo/src/lib.rs` (no change beyond Task 1)

**Interfaces:**
- Consumes: `crate::id::ElementKey`, `std::any::{Any, TypeId}`
- Produces: `InheritedRegistry` with methods (all take `&self`, use interior `RefCell`):
  - `InheritedRegistry::new() -> Self`
  - `register_provider(&self, key: ElementKey, type_id: TypeId, value: Box<dyn Any + Send + Sync>)`
  - `update_value(&self, key: ElementKey, type_id: TypeId, value: Box<dyn Any + Send + Sync>)`
  - `remove_provider(&self, key: ElementKey)` — also drops its dependents
  - `add_dependent(&self, provider: ElementKey, type_id: TypeId, dep: ElementKey)`
  - `value::<V: 'static>(&self, provider: ElementKey) -> Option<std::cell::Ref<'_, V>>`
  - `dependents_for(&self, provider: ElementKey) -> Vec<ElementKey>` (snapshot, no held borrow)

- [ ] **Step 1: Append the failing tests to `vexo/src/inherited_registry.rs`**

Append inside the existing `#[cfg(test)] mod tests` block:

```rust
    use super::InheritedRegistry;

    #[test]
    fn registry_register_and_value() {
        let reg = InheritedRegistry::new();
        let k = make_key();
        reg.register_provider(k, TypeId::of::<u32>(), Box::new(42u32));
        let v = reg.value_clone::<u32>(k).expect("provider should expose u32");
        assert_eq!(v, 42);
    }

    #[test]
    fn registry_value_missing_provider_returns_none() {
        let reg = InheritedRegistry::new();
        let k = make_key();
        assert!(reg.value_clone::<u32>(k).is_none());
    }

    #[test]
    fn registry_update_value() {
        let reg = InheritedRegistry::new();
        let k = make_key();
        reg.register_provider(k, TypeId::of::<u32>(), Box::new(1u32));
        reg.update_value(k, TypeId::of::<u32>(), Box::new(99u32));
        let v = reg.value_clone::<u32>(k).unwrap();
        assert_eq!(v, 99);
    }

    #[test]
    fn registry_remove_provider_drops_value_and_dependents() {
        let reg = InheritedRegistry::new();
        let provider = make_key();
        let dep = make_key();
        reg.register_provider(provider, TypeId::of::<u32>(), Box::new(7u32));
        reg.add_dependent(provider, TypeId::of::<u32>(), dep);
        reg.remove_provider(provider);
        assert!(reg.value::<u32>(provider).is_none());
        assert!(reg.dependents_for(provider).is_empty());
    }

    #[test]
    fn registry_add_dependent_idempotent() {
        let reg = InheritedRegistry::new();
        let provider = make_key();
        let dep = make_key();
        reg.register_provider(provider, TypeId::of::<u32>(), Box::new(0u32));
        reg.add_dependent(provider, TypeId::of::<u32>(), dep);
        reg.add_dependent(provider, TypeId::of::<u32>(), dep);
        let deps = reg.dependents_for(provider);
        assert_eq!(deps, vec![dep]);
    }

    #[test]
    fn registry_dependents_snapshot_does_not_hold_borrow() {
        // This test verifies that dependents_for returns an owned Vec, so the
        // caller can iterate while calling other &self methods.
        let reg = InheritedRegistry::new();
        let provider = make_key();
        let dep = make_key();
        reg.register_provider(provider, TypeId::of::<u32>(), Box::new(0u32));
        reg.add_dependent(provider, TypeId::of::<u32>(), dep);
        let deps = reg.dependents_for(provider);
        // Can still call other methods while iterating the snapshot.
        for d in deps {
            reg.add_dependent(provider, TypeId::of::<u32>(), d);
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo inherited_registry::tests::registry_`
Expected: FAIL with "cannot find type `InheritedRegistry` in this scope".

- [ ] **Step 3: Implement `InheritedRegistry`**

Append to `vexo/src/inherited_registry.rs` (above the `#[cfg(test)]` block):

```rust
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Pipeline-owned registry of inherited-value providers and their dependents.
///
/// Uses `RefCell` interior mutability so it can be borrowed as
/// `&InheritedRegistry` from both `ElementContext` and `RenderContext` while
/// methods take `&self` (same pattern as `BuildOwner`).
///
/// # Borrow safety
///
/// `InheritedElement::mount`/`update`/`unmount` never invoke user code while
/// holding a `RefCell` borrow, so re-entry is structurally prevented.
pub struct InheritedRegistry {
    /// Value each provider exposes, keyed by provider element.
    /// Stored as `Box<dyn Any + Send + Sync>` so lookups don't touch the
    /// element tree.
    values: RefCell<HashMap<ElementKey, (TypeId, Box<dyn Any + Send + Sync>)>>,

    /// Dependents per (provider, type). Used by `InheritedElement::update`
    /// to mark dependents dirty when the value changes.
    dependents:
        RefCell<HashMap<ElementKey, HashMap<TypeId, HashSet<ElementKey>>>>,
}

impl InheritedRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            values: RefCell::new(HashMap::new()),
            dependents: RefCell::new(HashMap::new()),
        }
    }

    /// Register a provider. Stores `value` under `key` for later lookup via
    /// `value::<V>(key)`. Called by `InheritedElement::mount`.
    pub fn register_provider(
        &self,
        key: ElementKey,
        type_id: TypeId,
        value: Box<dyn Any + Send + Sync>,
    ) {
        self.values.borrow_mut().insert(key, (type_id, value));
    }

    /// Replace the stored value for an existing provider. Called by
    /// `InheritedElement::update` when `update_should_notify` returned true.
    pub fn update_value(
        &self,
        key: ElementKey,
        type_id: TypeId,
        value: Box<dyn Any + Send + Sync>,
    ) {
        self.values.borrow_mut().insert(key, (type_id, value));
    }

    /// Remove a provider and all its dependents. Called by
    /// `InheritedElement::unmount`.
    pub fn remove_provider(&self, key: ElementKey) {
        self.values.borrow_mut().remove(&key);
        self.dependents.borrow_mut().remove(&key);
    }

    /// Register `dep` as a dependent of `provider` for value type `type_id`.
    /// Idempotent: adding the same dependent twice has no effect.
    pub fn add_dependent(
        &self,
        provider: ElementKey,
        type_id: TypeId,
        dep: ElementKey,
    ) {
        self.dependents
            .borrow_mut()
            .entry(provider)
            .or_default()
            .entry(type_id)
            .or_default()
            .insert(dep);
    }

    /// Read the value exposed by `provider` as type `V`, cloned out of the
    /// registry. Values are `Clone + PartialEq` by the `InheritedWidget` trait
    /// requirement, so cloning is always available.
    ///
    /// Returns `None` if `provider` is not registered or the stored value
    /// is not a `V`.
    pub fn value_clone<V: Clone + 'static>(&self, provider: ElementKey) -> Option<V> {
        self.values
            .borrow()
            .get(&provider)
            .and_then(|(_, v)| v.downcast_ref::<V>())
            .cloned()
    }

    /// Snapshot of dependents for `provider`. Returns an owned `Vec` so the
    /// caller can iterate without holding a `RefCell` borrow (important: the
    /// caller will call `BuildOwner::mark_needs_build` during iteration).
    pub fn dependents_for(&self, provider: ElementKey) -> Vec<ElementKey> {
        self.dependents
            .borrow()
            .get(&provider)
            .map(|by_type| {
                by_type
                    .values()
                    .flat_map(|set| set.iter().copied())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for InheritedRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

Note: the registry ships **only** `value_clone::<V>()` (returns `Option<V>`, clones inside the borrow). This is simpler and avoids `RefCell::Ref` lifetime gymnastics. Values are `Clone + PartialEq` by the `InheritedWidget` trait, so cloning is always available. Task 6's `depend_on_inherited_widget` uses `value_clone`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo inherited_registry::tests -- --nocapture`
Expected: PASS (10 tests: 4 from Task 1 + 6 from Task 2).

- [ ] **Step 5: Build the whole crate**

Run: `cargo build -p vexo`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/inherited_registry.rs
git commit -m "feat(inherited): add InheritedRegistry for provider values and dependents"
```

---

### Task 3: Wire `InheritedWidget` through contexts, pipeline, and reconciler

> **Atomic task — Tasks 3, 4, and 5 commit together.** This task (Part A: contexts), Task 4 (Part B: trait + element), and Task 5 (Part C: pipeline + reconciler) form a single compilation unit. The build will not pass until all three parts are done. There is exactly **one commit** at the end of Task 5. Do not run `cargo build` until Task 5 Step 8.

This task adds the new fields to both contexts and updates the constructor signature. No behavior changes yet — just plumbing. The new fields are borrowed, so existing test harnesses that build `ElementContext::new(...)` directly will need updating (done in Task 5).

**Files:**
- Modify: `vexo/src/element_context.rs`
- Modify: `vexo/src/stateful_widget.rs` (`RenderContext` struct + `build_child_widget`)
- Modify: `vexo/src/reconciler.rs` (7 `ElementContext::new` call sites)
- Modify: `vexo/src/elements/leaf.rs` (4 `ElementContext::new` call sites — test helpers)
- Modify: `vexo/src/stateful_widget.rs` (in-module tests: 3 `ElementContext::new` call sites in `tests` mod)
- Modify: any other test file that calls `ElementContext::new` — find with `rg -n "ElementContext::new" vexo/src/`

**Interfaces:**
- Consumes: `InheritedMap`, `InheritedRegistry` from Task 1/2.
- Produces: `ElementContext::new(...)` and `RenderContext { ... }` with three new fields (`inherited_map`, `inherited_registry`, `inherited_map_storage`). `StatefulElement::build_child_widget()` gains two params (`inherited_map`, `inherited_registry` — storage is only on `ElementContext`, not `RenderContext`).

- [ ] **Step 1: Find every `ElementContext::new` call site**

Run: `rg -n "ElementContext::new" vexo/src/`
Record every file + line — they all need updating. Expected: `reconciler.rs` (7), `elements/leaf.rs` (4), `stateful_widget.rs` tests (3), possibly more in test files.

- [ ] **Step 2: Extend `ElementContext` struct + constructor**

In `vexo/src/element_context.rs`:

Add to imports (top of file):
```rust
use crate::inherited_registry::{InheritedMap, InheritedRegistry};
use slotmap::SecondaryMap;
use std::sync::Arc;
```

Add three fields to the `ElementContext<'a>` struct (after `animation_ticker`):
```rust
    /// Nearest-ancestor cache for inherited values. Read-only here; built
    /// top-down at mount by the pipeline. Elements that are `InheritedElement`s
    /// produce a new map for their subtree (see `inherited_widget.rs`).
    pub inherited_map: &'a InheritedMap,

    /// Pipeline-owned registry of inherited-value providers. Used by
    /// `InheritedElement` to register/remove itself and by `RenderContext` to
    /// register dependents at lookup time.
    pub inherited_registry: &'a InheritedRegistry,

    /// Pipeline-owned per-element map storage. `InheritedElement::mount`
    /// writes its subtree map here so children can read it via their
    /// `inherited_map` (which the reconciler resolves from this storage).
    pub inherited_map_storage:
        &'a mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
```

Extend `ElementContext::new()` signature (add the three new params at the end, before the closing `)`):
```rust
    pub fn new(
        element_id: ElementKey,
        parent: Option<ElementKey>,
        children: Vec<ElementKey>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a mpsc::Sender<ElementKey>,
        child_ops: &'a mut ChildOps,
        focus_manager: &'a mut FocusManager,
        parent_focus_node_id: Option<FocusNodeId>,
        animation_ticker: Arc<AnimationTicker>,
        inherited_map: &'a InheritedMap,
        inherited_registry: &'a InheritedRegistry,
        inherited_map_storage: &'a mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
    ) -> Self {
        Self {
            element_id,
            parent,
            children,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
            focus_manager,
            parent_focus_node_id,
            animation_ticker,
            inherited_map,
            inherited_registry,
            inherited_map_storage,
        }
    }
```

- [ ] **Step 3: Extend `RenderContext` struct**

In `vexo/src/stateful_widget.rs`, find the `RenderContext<'a>` struct (around line 308). Add:

Import at top of file:
```rust
use crate::inherited_registry::{InheritedMap, InheritedRegistry};
```

Add two fields:
```rust
pub struct RenderContext<'a> {
    pub element_id: ElementKey,
    pub dirty: &'a mut DirtyTracking,
    pub render_objects: &'a mut RenderObjectRegistry,
    pub build_owner: &'a BuildOwner,
    /// Nearest-ancestor cache for inherited values (read-only here).
    pub inherited_map: &'a InheritedMap,
    /// Pipeline-owned registry; `depend_on_inherited_widget` uses interior
    /// mutability to register the caller as a dependent.
    pub inherited_registry: &'a InheritedRegistry,
}
```

- [ ] **Step 4: Extend `StatefulElement::build_child_widget`**

In `vexo/src/stateful_widget.rs` around line 462, change the signature and body:

```rust
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
```

- [ ] **Step 5: Update the three call sites of `build_child_widget`**

In `vexo/src/stateful_widget.rs`, `build_child_widget` is called in `mount` (around line 576), `update` (around line 627), and `rebuild_from_state` (around line 719). Each currently passes `(element_id, state_ref, context.dirty, context.render_objects, context.build_owner)`. Add the two new args from the `context`:

For each call site, change:
```rust
self.build_child_widget(
    element_id,
    state_ref,
    context.dirty,
    context.render_objects,
    context.build_owner,
)
```
to:
```rust
self.build_child_widget(
    element_id,
    state_ref,
    context.dirty,
    context.render_objects,
    context.build_owner,
    context.inherited_map,
    context.inherited_registry,
)
```

- [ ] **Step 6: Update all `ElementContext::new` call sites in `reconciler.rs`**

For each of the 7 call sites in `vexo/src/reconciler.rs` (lines around 281, 364, 430, 497, 615, 721, 905), the call currently ends with `..., parent_focus_node_id, animation_ticker.clone())`. Add two args before the closing paren:

```rust
            parent_focus_node_id,
            animation_ticker.clone(),
            inherited_map,
            inherited_registry,
        );
```

The `inherited_map` and `inherited_registry` need to be in scope at each call site. They will be threaded as new params on the reconciler methods (Task 5 threads them from the pipeline). For now, to keep this task compiling in isolation, pass placeholder values:

The `inherited_map` and `inherited_registry` need to be in scope at each call site. They will be threaded as new params on the reconciler methods (Task 5 threads them from the pipeline). The call-site updates happen in Task 5 — skip them here.

- [ ] **Step 7: Update `ElementContext::new` call sites in `elements/leaf.rs` and test files**

Same situation as Step 6 — these are test helpers that will break until they pass the new args. The call-site updates happen in Task 5. Skip here.

- [ ] **Step 8: Do NOT commit yet**

Part A of the atomic Task 3-4-5 unit. The build will not pass until Task 5. Proceed to Task 4 (Part B), then Task 5 (Part C) which wires everything and commits all three parts together.

---

### Task 4: `InheritedWidget` trait + `InheritedElement` + blanket `Widget` impl

> **Part B of the atomic Task 3-4-5 unit.** No commit yet — the build still won't pass until Task 5.

**Files:**
- Create: `vexo/src/inherited_widget.rs`
- Modify: `vexo/src/lib.rs` (add `mod inherited_widget;`)

**Interfaces:**
- Consumes: `InheritedMap`, `InheritedRegistry` (Task 1/2); `Element`, `ElementContext`, `Widget`, `RenderObject`, `ProxyRenderObject` (existing); `RenderObjectElement` trait (existing, in `crate::elements`).
- Produces: `InheritedWidget` trait (with `Value`, `value()`, `child()`, `key()`, `update_should_notify()`); `InheritedElement<W>` struct implementing `Element`; blanket `impl<T: InheritedWidget> Widget for T`.

- [ ] **Step 1: Write the trait + element + blanket impl**

Create `vexo/src/inherited_widget.rs`:

```rust
//! `InheritedWidget` trait and `InheritedElement` — the provider primitive.
//!
//! An `InheritedWidget` exposes an immutable value of type `Self::Value` to
//! all descendants. Dependents read it via
//! `RenderContext::depend_on_inherited_widget::<V>()` (added in a later task)
//! and auto-rebuild when the value changes (the provider's `update()` calls
//! `BuildOwner::mark_needs_build` on each dependent).
//!
//! See `docs/superpowers/specs/2026-07-12-inherited-widget-design.md`.

use std::any::{Any, TypeId};

use crate::element::Element;
use crate::element_context::ElementContext;
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::id::{ElementKey, RenderObjectKey};
use crate::inherited_registry::{InheritedMap, InheritedRegistry};
use crate::key::WidgetKey;
use crate::render_object::{RenderObject, RenderObjectRegistry};
use crate::stateful_widget::ProxyRenderObject;
use crate::update_result::UpdateResult;
use crate::widgets::Widget;

/// A widget that exposes a value of type `Self::Value` to all descendants.
///
/// Immutable: to change the value, an ancestor rebuilds with a new
/// `InheritedWidget`. Dependents auto-rebuild via the `InheritedRegistry`.
///
/// Requires `Clone` so the blanket `Widget` impl can satisfy `clone_boxed()`.
///
/// # Implementing
///
/// ```ignore
/// #[derive(Clone)]
/// struct MyEnv { value: u32, child: Box<dyn Widget> }
///
/// impl InheritedWidget for MyEnv {
///     type Value = u32;
///     fn value(&self) -> &u32 { &self.value }
///     fn child(&self) -> &dyn Widget { self.child.as_ref() }
/// }
/// ```
pub trait InheritedWidget: Clone + 'static {
    /// The value type exposed to descendants. `Clone + PartialEq` so the
    /// default `update_should_notify` can compare old vs new.
    type Value: Clone + PartialEq + 'static;

    /// The current value exposed to descendants.
    fn value(&self) -> &Self::Value;

    /// The single child subtree that gets access to this value.
    fn child(&self) -> &dyn Widget;

    /// Optional key for identity across frames (default `None`).
    fn key(&self) -> Option<WidgetKey> {
        None
    }

    /// Whether updating `old → new` should rebuild dependents.
    ///
    /// Default: rebuild iff `value()` changed.
    fn update_should_notify(&self, old: &Self, new: &Self) -> bool {
        old.value() != new.value()
    }
}

/// Blanket `Widget` impl for any `InheritedWidget`.
///
/// Bridges `InheritedWidget` (provider trait) to `Widget` (the widget-tree
/// trait). Creates an `InheritedElement` and a pass-through `ProxyRenderObject`
/// (reused from `stateful_widget.rs`).
impl<T: InheritedWidget> Widget for T {
    fn key(&self) -> Option<WidgetKey> {
        <Self as InheritedWidget>::key(self)
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(InheritedElement::<T>::new(self.clone()))
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ProxyRenderObject::new())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(<Self as InheritedWidget>::child(self))
    }

    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        // InheritedWidget's only mutable state is the value, which lives in
        // the registry, not the render object. The proxy RO has no
        // layout/paint-affecting properties.
        UpdateResult::NONE
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

/// Element backing an `InheritedWidget`. Registers itself as a provider in
/// `InheritedRegistry` at mount, marks dependents dirty on update, and
/// unregisters at unmount.
pub struct InheritedElement<W: InheritedWidget> {
    widget: W,
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object_id: Option<RenderObjectKey>,
    /// The `Arc<InheritedMap>` this element produced for its subtree.
    /// Held so children can read it; dropped on unmount.
    subtree_map: Option<std::sync::Arc<InheritedMap>>,
    focus_attachment: Option<FocusAttachment>,
}

impl<W: InheritedWidget> InheritedElement<W> {
    pub fn new(widget: W) -> Self {
        Self {
            widget,
            id: None,
            key: None,
            render_object_id: None,
            subtree_map: None,
            focus_attachment: None,
        }
    }

    fn type_id_of_value() -> TypeId {
        TypeId::of::<W::Value>()
    }

    fn get_child_widget(&self) -> &dyn Widget {
        self.widget.child()
    }
}

impl<W: InheritedWidget> RenderObjectElement for InheritedElement<W> {
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

impl<W: InheritedWidget> Element for InheritedElement<W> {
    fn mount(&mut self, context: &mut ElementContext) {
        let element_key = context.element_id;

        // 1. Focus node (same pattern as SafeAreaElement).
        let parent_id = context.parent_focus_node_id();
        let node_id = context
            .focus_manager()
            .create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        // 2. Register self as a provider in InheritedRegistry.
        let type_id = Self::type_id_of_value();
        context
            .inherited_registry
            .register_provider(element_key, type_id, Box::new(self.widget.value().clone()));

        // 3. Build this element's subtree map: parent's map + self.
        // The pipeline stores this map so children can read it.
        // NOTE: The pipeline-owned `SecondaryMap` is updated via a method we
        // add in Task 5 (`context.store_inherited_map`). For now, build the
        // map and store it on `self.subtree_map` so Task 5 can wire it.
        let new_map = context.inherited_map.with_insert(type_id, element_key);
        self.subtree_map = Some(std::sync::Arc::new(new_map));

        // 4. Mount render object (pass-through proxy).
        self.mount_render_object(context);

        // 5. Inflate child — child's mount will receive this element's map
        // (Task 5 wires the pipeline to pass `self.subtree_map` as the
        // child's `inherited_map`).
        context.inflate_child(None, self.get_child_widget().clone_boxed());
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast to W.
        let new_w: W = {
            if let Ok(boxed) = new_widget.downcast::<Box<dyn Widget>>() {
                if let Some(w) = boxed.as_any().downcast_ref::<W>() {
                    w.clone()
                } else {
                    return;
                }
            } else {
                return;
            }
        };

        let should_notify = self.widget.update_should_notify(&self.widget, &new_w);
        self.widget = new_w;

        if should_notify {
            let element_key = context.element_id;
            let type_id = Self::type_id_of_value();
            // Update the stored value.
            context
                .inherited_registry
                .update_value(element_key, type_id, Box::new(self.widget.value().clone()));
            // Mark all dependents dirty.
            let deps = context.inherited_registry.dependents_for(element_key);
            for dep in deps {
                context.build_owner.mark_needs_build(dep);
            }
        }

        // Reconcile child via child_ops (same as SafeAreaElement::rebuild).
        let old_child = context.children().first().copied();
        let child_widget = self.get_child_widget().clone_boxed();
        match old_child {
            Some(old_child_key) => {
                context.update_child(old_child_key, child_widget);
            }
            None => {
                context.inflate_child(None, child_widget);
            }
        }

        if let Some(attachment) = self.focus_attachment.as_ref() {
            let new_parent_id = context.parent_focus_node_id();
            attachment.reparent_to(new_parent_id, context.focus_manager());
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        let element_key = context.element_id;

        // Remove from registry first (drops value + dependents).
        context.inherited_registry.remove_provider(element_key);

        // Drop subtree map (pipeline's SecondaryMap entry cleared in Task 5).
        self.subtree_map = None;

        // Use RenderObjectElement's default unmount.
        self.unmount_render_object(context);

        // Unmount child.
        if let Some(child_key) = context.children().first().copied() {
            context.unmount_child(child_key);
        }

        // Detach focus node.
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
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Same logic as update().
        self.update(new_widget, context);
    }

    fn focus_attachment(&self) -> &Option<FocusAttachment> {
        &self.focus_attachment
    }

    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> {
        &mut self.focus_attachment
    }
}
```

- [ ] **Step 2: Register the module in `lib.rs`**

In `vexo/src/lib.rs`, after `mod inherited_registry;` (added in Task 1), add:

```rust
mod inherited_widget;
```

- [ ] **Step 3: Build (expect errors from unwired Task 3 plumbing)**

Run: `cargo build -p vexo`
Expected: FAIL — `ElementContext` is missing `inherited_map`/`inherited_registry` fields (Task 3 not yet committed). Also `context.inherited_registry` and `context.inherited_map` references in the new file won't resolve.

This is expected — Task 5 completes the wiring. **Do not commit yet.** Proceed to Task 5.

- [ ] **Step 4: Add unit test for the trait + blanket impl**

Append to `vexo/src/inherited_widget.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[derive(Clone)]
    struct TestEnv {
        value: u32,
        child: Box<dyn Widget>,
    }

    impl TestEnv {
        fn new(value: u32, child: impl Widget + 'static) -> Self {
            Self { value, child: Box::new(child) }
        }
    }

    impl InheritedWidget for TestEnv {
        type Value = u32;
        fn value(&self) -> &u32 { &self.value }
        fn child(&self) -> &dyn Widget { self.child.as_ref() }
    }

    #[test]
    fn blanket_widget_impl_creates_inherited_element() {
        let env = TestEnv::new(42, Text::new("hi"));
        let _elem = env.create_element(); // should not panic
    }

    #[test]
    fn blanket_widget_impl_child_delegates() {
        let env = TestEnv::new(42, Text::new("hi"));
        let child = Widget::child(&env);
        assert!(child.as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn blanket_widget_impl_clone_boxed() {
        let env = TestEnv::new(42, Text::new("hi"));
        let cloned = env.clone_boxed();
        assert!(cloned.as_any().downcast_ref::<TestEnv>().is_some());
    }

    #[test]
    fn update_should_notify_default_compares_value() {
        let old = TestEnv::new(1, Text::new("a"));
        let new_same = TestEnv::new(1, Text::new("b")); // value same, child differs
        let new_diff = TestEnv::new(2, Text::new("c"));
        assert!(!old.update_should_notify(&old, &new_same));
        assert!(old.update_should_notify(&old, &new_diff));
    }
}
```

- [ ] **Step 5: Do NOT commit yet**

Part B of the atomic Task 3-4-5 unit. Proceed to Task 5 (Part C), which completes the wiring and commits all three parts together.

---

### Task 5: Wire pipeline ownership + reconciler call sites + commit (completes Tasks 3-4-5)

> **Part C of the atomic Task 3-4-5 unit.** This is where the build first passes and the single commit happens.

**Files:**
- Modify: `vexo/src/pipeline.rs` (add `inherited_registry` + `inherited_maps` fields; thread to reconciler; clear maps on unmount)
- Modify: `vexo/src/reconciler.rs` (7 `ElementContext::new` call sites: compute `inherited_map` from parent, pass registry)
- Modify: `vexo/src/elements/leaf.rs` (4 call sites: pass empty map + fresh registry for test helpers)
- Modify: `vexo/src/stateful_widget.rs` (3 in-module test call sites)
- Modify: any other test file calling `ElementContext::new` (search in Step 1)
- Modify: `vexo/src/element_context.rs` (add `store_inherited_map` helper if needed for `InheritedElement::mount`)

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces: A compiling, working `InheritedWidget` system (no `Theme` yet, no `depend_on_inherited_widget` helper yet — those are Tasks 6-7).

- [ ] **Step 1: Find every remaining `ElementContext::new` call site**

Run: `rg -n "ElementContext::new" vexo/src/`
Expected list includes: `reconciler.rs` (7), `elements/leaf.rs` (4), `stateful_widget.rs` (3 in `tests` mod), and possibly `stateful_integration_test.rs` or others. Record every match.

- [ ] **Step 2: Add fields to `ThreeTreePipeline`**

In `vexo/src/pipeline.rs`:

Add imports:
```rust
use slotmap::SecondaryMap;
use std::sync::Arc;
use crate::inherited_registry::{InheritedMap, InheritedRegistry};
```

Add two fields to `ThreeTreePipeline` (after `animation_ticker`):
```rust
    /// Pipeline-owned registry of inherited-value providers and dependents.
    /// Passed by `&` to every `ElementContext` and `RenderContext`.
    inherited_registry: InheritedRegistry,

    /// Per-element `Arc<InheritedMap>`. Built top-down at mount: each element
    /// inherits its parent's map (Arc clone), and `InheritedElement`s insert
    /// their own type. Cleared on unmount.
    inherited_maps: SecondaryMap<ElementKey, Arc<InheritedMap>>,
```

In `ThreeTreePipeline::new()`, initialize:
```rust
            inherited_registry: InheritedRegistry::new(),
            inherited_maps: SecondaryMap::new(),
```

- [ ] **Step 3: Expose the registry and maps to the reconciler**

The reconciler methods (`reconcile`, `rebuild_root`, `perform_rebuilds`, `reconcile_element`, etc.) currently take `&mut ElementRegistry`, `&mut RenderObjectRegistry`, etc. as params. Add `inherited_registry: &InheritedRegistry` and `inherited_maps: &SecondaryMap<ElementKey, Arc<InheritedMap>>` as new params on every reconciler method that constructs an `ElementContext`.

**Important borrow-ordering detail:** `inherited_maps` is read-only at `ElementContext::new` call sites (we look up the parent's map). `inherited_registry` is borrowed as `&` (interior mutability). Neither conflicts with `&mut ElementRegistry`.

For each `ElementContext::new` call site in `reconciler.rs`, compute the map from the parent:

```rust
            let inherited_map: &InheritedMap = match parent {
                None => EMPTY_MAP,                // root
                Some(p) => inherited_maps
                    .get(p)
                    .map(|arc| arc.as_ref())
                    .unwrap_or(EMPTY_MAP),
            };
```

where `EMPTY_MAP` is a thread-local or `once_cell` empty map. To avoid lifetime gymnastics, use a `const`-style empty map. Add near the top of `reconciler.rs`:

```rust
thread_local! {
    static EMPTY_INHERITED_MAP: InheritedMap = InheritedMap::empty();
}
```

and at each call site:
```rust
            let inherited_map: &InheritedMap = match parent {
                None => EMPTY_INHERITED_MAP.with(|m| m),
                Some(p) => inherited_maps
                    .get(p)
                    .map(|arc| arc.as_ref())
                    .unwrap_or_else(|| EMPTY_INHERITED_MAP.with(|m| m)),
            };
```

Then pass `inherited_map, inherited_registry, inherited_maps` (the `&mut SecondaryMap`) as the last three args to `ElementContext::new`.

- [ ] **Step 4: Store the subtree map when `InheritedElement` mounts**

`InheritedElement::mount` (Task 4) builds `new_map` and stores it in `self.subtree_map`. It must also write it into the pipeline's `SecondaryMap` so children can read it via the reconciler's parent lookup (Step 3).

The `inherited_map_storage: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>` field was already added to `ElementContext` in Task 3 Step 2. Update `InheritedElement::mount` (in `vexo/src/inherited_widget.rs` from Task 4) — replace the `self.subtree_map = Some(...)` line with:

```rust
        let arc = std::sync::Arc::new(new_map);
        context.inherited_map_storage.insert(element_key, arc.clone());
        self.subtree_map = Some(arc);
```

And in `InheritedElement::unmount`, clear the storage entry:
```rust
        context.inherited_map_storage.remove(element_key);
```
(add this right before `self.subtree_map = None;`)

- [ ] **Step 5: Update the 3 in-module test call sites in `stateful_widget.rs`**

In `vexo/src/stateful_widget.rs` `#[cfg(test)] mod tests`, the helper `create_test_context()` returns a tuple. Each test builds `ElementContext::new(...)`. Add an empty `InheritedMap`, a fresh `InheritedRegistry`, and a fresh `SecondaryMap` to the helper, and pass them:

```rust
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
        crate::inherited_registry::InheritedRegistry,
        slotmap::SecondaryMap<ElementKey, std::sync::Arc<crate::inherited_registry::InheritedMap>>,
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
            crate::inherited_registry::InheritedRegistry::new(),
            slotmap::SecondaryMap::new(),
        )
    }
```

In each test, destructure the two new entries (`inherited_registry`, `inherited_maps`) and pass them. Also create an empty `InheritedMap` for the `inherited_map` param:

```rust
        let empty_map = crate::inherited_registry::InheritedMap::empty();
```

and pass `&empty_map, &inherited_registry, &mut inherited_maps` as the last three args to `ElementContext::new`.

- [ ] **Step 6: Update the 4 call sites in `elements/leaf.rs`**

Same pattern as Step 5 — these are test helpers. Add an empty map, fresh registry, and fresh `SecondaryMap` to each test that constructs `ElementContext::new`.

- [ ] **Step 7: Update any other test call sites found in Step 1**

For each remaining `ElementContext::new` call site (e.g. in `stateful_integration_test.rs`), apply the same pattern. Run `rg -n "ElementContext::new" vexo/src/` after edits to confirm zero un-updated sites.

- [ ] **Step 8: Build the whole crate**

Run: `cargo build -p vexo`
Expected: PASS. If errors remain, fix them — they'll be missing args at call sites.

- [ ] **Step 9: Run the full test suite**

Run: `cargo test -p vexo`
Expected: PASS (all pre-existing tests still pass; new unit tests from Tasks 1, 2, 4 also pass).

- [ ] **Step 10: Commit (completes the atomic Task 3-4-5 unit)**

```bash
git add vexo/src/element_context.rs vexo/src/stateful_widget.rs vexo/src/reconciler.rs vexo/src/elements/leaf.rs vexo/src/pipeline.rs vexo/src/inherited_widget.rs vexo/src/lib.rs vexo/src/stateful_integration_test.rs
git commit -m "feat(inherited): wire InheritedWidget through contexts, pipeline, and reconciler"
```

---

### Task 6: `RenderContext::depend_on_inherited_widget` helper

**Files:**
- Modify: `vexo/src/stateful_widget.rs` (add method to `RenderContext`)

**Interfaces:**
- Consumes: `InheritedMap::get`, `InheritedRegistry::value` / `value_clone`, `InheritedRegistry::add_dependent`.
- Produces: `RenderContext::depend_on_inherited_widget::<V: Clone + 'static>(&mut self) -> Option<V>`.

- [ ] **Step 1: Write the failing test**

Append to `vexo/src/stateful_widget.rs` `#[cfg(test)] mod tests`:

```rust
    use crate::inherited_registry::{InheritedMap, InheritedRegistry};
    use crate::id::ElementKey;

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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo stateful_widget::tests::depend_on_inherited_widget`
Expected: FAIL with "no method `depend_on_inherited_widget` found".

- [ ] **Step 3: Implement the method**

In `vexo/src/stateful_widget.rs`, add to `impl<'a> RenderContext<'a>`:

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo stateful_widget::tests::depend_on_inherited_widget`
Expected: PASS (3 tests).

- [ ] **Step 5: Build the whole crate**

Run: `cargo build -p vexo`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/stateful_widget.rs
git commit -m "feat(inherited): add RenderContext::depend_on_inherited_widget helper"
```

---

### Task 7: `Theme` widget + `ThemeData`

**Files:**
- Create: `vexo/src/widgets/theme.rs`
- Modify: `vexo/src/widgets/mod.rs` (add `mod theme;` and re-export)

**Interfaces:**
- Consumes: `InheritedWidget` trait (Task 4), `RenderContext::depend_on_inherited_widget` (Task 6), `crate::core::Color`.
- Produces: `ThemeData` struct, `Theme` widget, `Theme::of(ctx)`.

- [ ] **Step 1: Write `ThemeData` and `Theme`**

Create `vexo/src/widgets/theme.rs`:

```rust
//! `Theme` — an `InheritedWidget` exposing `ThemeData` to descendants.
//!
//! Proves the ergonomic lookup pattern: descendants call `Theme::of(ctx)`
//! to read the nearest theme and auto-rebuild when it changes.
//!
//! See `docs/superpowers/specs/2026-07-12-inherited-widget-design.md`.

use crate::core::Color;
use crate::inherited_widget::InheritedWidget;
use crate::key::WidgetKey;
use crate::stateful_widget::RenderContext;
use crate::widgets::Widget;

/// Immutable theme data exposed to descendants by `Theme`.
///
/// Core Material-ish color roles only. Additive: new fields don't break
/// dependents.
#[derive(Clone, PartialEq, Debug)]
pub struct ThemeData {
    pub primary: Color,
    pub on_primary: Color,
    pub background: Color,
    pub on_background: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub error: Color,
    pub on_error: Color,
}

impl ThemeData {
    /// Light preset. Used as the fallback when no `Theme` ancestor exists.
    pub fn light() -> Self {
        Self {
            primary: Color::rgb(0x6p, 0x75, 0xFF),
            on_primary: Color::WHITE,
            background: Color::WHITE,
            on_background: Color::BLACK,
            surface: Color::rgb(0xFF, 0xFF, 0xFF),
            on_surface: Color::rgb(0x1C, 0x1B, 0x1F),
            error: Color::rgb(0xB3, 0x26, 0x1E),
            on_error: Color::WHITE,
        }
    }

    /// Dark preset.
    pub fn dark() -> Self {
        Self {
            primary: Color::rgb(0x12, 0x14, 0x34),
            on_primary: Color::WHITE,
            background: Color::rgb(0x1C, 0x1B, 0x1F),
            on_background: Color::WHITE,
            surface: Color::rgb(0x2B, 0x29, 0x30),
            on_surface: Color::WHITE,
            error: Color::rgb(0xF2, 0xB8, 0xB5),
            on_error: Color::BLACK,
        }
    }
}

impl Default for ThemeData {
    fn default() -> Self {
        Self::light()
    }
}

/// An `InheritedWidget` that exposes `ThemeData` to its subtree.
pub struct Theme {
    data: ThemeData,
    child: Box<dyn Widget>,
    key: Option<WidgetKey>,
}

impl Theme {
    /// Create a `Theme` that exposes `data` to `child`'s subtree.
    pub fn new(data: ThemeData, child: impl Widget + 'static) -> Self {
        Self {
            data,
            child: Box::new(child),
            key: None,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Read the nearest ancestor `Theme`. Establishes a dependency:
    /// caller rebuilds when the theme data changes.
    ///
    /// Falls back to `ThemeData::light()` when no `Theme` ancestor exists,
    /// so tests and small demos that don't wrap a `Theme` get sensible colors.
    pub fn of(ctx: &mut RenderContext) -> ThemeData {
        ctx.depend_on_inherited_widget::<ThemeData>()
            .unwrap_or_else(ThemeData::light)
    }
}

impl Clone for Theme {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            child: self.child.clone_boxed(),
            key: self.key.clone(),
        }
    }
}

impl InheritedWidget for Theme {
    type Value = ThemeData;

    fn value(&self) -> &ThemeData {
        &self.data
    }

    fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }
}
```

**Fix the typo in `light()`:** `Color::rgb(0x6p, ...)` is invalid hex — use `0x67`:
```rust
            primary: Color::rgb(0x67, 0x75, 0xFF),
```

- [ ] **Step 2: Register the module and re-export**

In `vexo/src/widgets/mod.rs`, add after `mod text_edit_content;` (around line 22):
```rust
mod theme;
```

And in the re-export section (near `pub use text::Text;`), add:
```rust
pub use theme::{Theme, ThemeData};
```

- [ ] **Step 3: Re-export from `lib.rs`**

In `vexo/src/lib.rs`, find the existing widget re-exports and add. The current pattern uses `pub use` from `widgets`. Check if `Theme`/`ThemeData` need top-level re-export by looking at how other widgets are re-exported. If `Text` is re-exported at crate root, do the same for `Theme`:

```rust
pub use widgets::{Theme, ThemeData};
```

(Search for `pub use widgets::` in `lib.rs` to find the right spot.)

- [ ] **Step 4: Write unit tests**

Append to `vexo/src/widgets/theme.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn theme_data_light_and_dark_differ() {
        assert_ne!(ThemeData::light(), ThemeData::dark());
    }

    #[test]
    fn theme_data_default_is_light() {
        assert_eq!(ThemeData::default(), ThemeData::light());
    }

    #[test]
    fn theme_inherited_widget_value() {
        let t = Theme::new(ThemeData::dark(), Text::new("hi"));
        assert_eq!(t.value(), &ThemeData::dark());
    }

    #[test]
    fn theme_inherited_widget_child() {
        let t = Theme::new(ThemeData::dark(), Text::new("hi"));
        assert!(t.child().as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn theme_clone_preserves_data_and_child() {
        let t = Theme::new(ThemeData::dark(), Text::new("hi")).with_key("thm");
        let cloned = t.clone();
        assert_eq!(cloned.value(), t.value());
        assert!(cloned.child().as_any().downcast_ref::<Text>().is_some());
        assert_eq!(cloned.key(), t.key());
    }

    #[test]
    fn theme_update_should_notify_default() {
        let t1 = Theme::new(ThemeData::light(), Text::new("a"));
        let t2_same = Theme::new(ThemeData::light(), Text::new("b"));
        let t3_diff = Theme::new(ThemeData::dark(), Text::new("c"));
        // Default impl compares value() — child changes don't notify.
        assert!(!t1.update_should_notify(&t1, &t2_same));
        assert!(t1.update_should_notify(&t1, &t3_diff));
    }
}
```

- [ ] **Step 5: Build**

Run: `cargo build -p vexo`
Expected: PASS.

- [ ] **Step 6: Run unit tests**

Run: `cargo test -p vexo widgets::theme::tests`
Expected: PASS (6 tests).

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/theme.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat(theme): add Theme InheritedWidget and ThemeData"
```

---

### Task 8: Element-level integration tests

**Files:**
- Create: `vexo/src/inherited_integration_test.rs`
- Modify: `vexo/src/lib.rs` (add `#[cfg(test)] mod inherited_integration_test;`)

**Interfaces:**
- Consumes: All previous tasks. Uses `ThreeTreePipeline::reconcile`, `BuildOwner::is_dirty`, `Theme`, `ThemeData`, `Component`, `RenderContext`, `Signal`.

- [ ] **Step 1: Write the first integration test (provider value read)**

Create `vexo/src/inherited_integration_test.rs`:

```rust
//! Integration tests for InheritedWidget via the ThreeTreePipeline.
//!
//! Mirrors the harness in `stateful_integration_test.rs` — drive
//! `pipeline.reconcile()` with widget trees, assert on observed state.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::animation::AnimationTicker;
    use crate::reactive::Signal;
    use crate::stateful_widget::RenderContext;
    use crate::widgets::{Text, Theme, ThemeData};
    use crate::{Component, ComponentState, ThreeTreePipeline, Widget};

    #[derive(Clone)]
    struct ThemeReader;

    #[derive(Default)]
    struct ThemeReaderState {
        last_read: Signal<ThemeData>,
    }

    impl ComponentState for ThemeReaderState {}

    impl Component for ThemeReader {
        type State = ThemeReaderState;

        fn render(&self, state: &mut ThemeReaderState, ctx: &mut RenderContext) -> Box<dyn Widget> {
            let data = Theme::of(ctx);
            state.last_read.set(data);
            Box::new(Text::new("reader"))
        }
    }

    #[test]
    fn theme_provider_value_reaches_descendant() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Theme(dark) → ThemeReader
        let tree = Theme::new(ThemeData::dark(), ThemeReader);
        pipeline.reconcile(Box::new(tree));

        // After reconcile, the ThemeReader's render() ran and read the dark theme.
        // We can't directly inspect Signal state from outside; instead, this test
        // just verifies no panic and the tree mounted. A stronger assertion is in
        // the next test using a global counter.
        assert!(!pipeline.element_registry().is_empty());
    }

    // ... more tests below
}
```

- [ ] **Step 2: Register the test module**

In `vexo/src/lib.rs`, after the `#[cfg(test)] mod stateful_integration_test;` line, add:
```rust
#[cfg(test)]
mod inherited_integration_test;
```

- [ ] **Step 3: Run the first test to verify it passes**

Run: `cargo test -p vexo inherited_integration_test::tests::theme_provider_value_reaches_descendant`
Expected: PASS.

- [ ] **Step 4: Add the dependency-triggered rebuild test**

This is the key correctness test. Use a global atomic to record what `ThemeReader` saw, then update the `Theme` and verify the reader rebuilt with the new value.

Append to `vexo/src/inherited_integration_test.rs` `mod tests`:

```rust
    use std::sync::atomic::{AtomicU32, Ordering};

    static LAST_SEEN_PRIMARY: AtomicU32 = AtomicU32::new(0);

    #[derive(Clone)]
    struct ThemeReaderRecord;

    #[derive(Default)]
    struct ThemeReaderRecordState;

    impl ComponentState for ThemeReaderRecordState {}

    impl Component for ThemeReaderRecord {
        type State = ThemeReaderRecordState;

        fn render(&self, _state: &mut ThemeReaderRecordState, ctx: &mut RenderContext) -> Box<dyn Widget> {
            let data = Theme::of(ctx);
            // Record the primary color's red channel as a u32 sentinel.
            let r = match data.primary {
                crate::Color::Rgb { r, g: _, b: _, a: _ } => r as u32,
                _ => 0,
            };
            LAST_SEEN_PRIMARY.store(r, Ordering::SeqCst);
            Box::new(Text::new("reader"))
        }
    }

    #[derive(Clone)]
    struct App {
        data: ThemeData,
    }

    #[derive(Default)]
    struct AppState;

    impl ComponentState for AppState {}

    impl Component for App {
        type State = AppState;

        fn render(&self, _state: &mut AppState, _ctx: &mut RenderContext) -> Box<dyn Widget> {
            Box::new(Theme::new(self.data.clone(), ThemeReaderRecord))
        }
    }
```

**Color variant check:** Inspect `crate::core::Color` to get the exact variant names. Run `rg "pub enum Color" vexo/src/core/color.rs -A 10` and adjust the `match` arms to match the actual definition.

- [ ] **Step 5: Write the rebuild test**

```rust
    #[test]
    fn theme_update_rebuilds_dependents() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Initial: light theme.
        let light = App { data: ThemeData::light() };
        pipeline.reconcile(Box::new(light));
        pipeline.perform_rebuilds(); // ensure initial render ran

        let light_r = LAST_SEEN_PRIMARY.load(Ordering::SeqCst);
        assert_ne!(light_r, 0, "reader should have read a non-zero primary");

        // Update: dark theme.
        let dark = App { data: ThemeData::dark() };
        pipeline.reconcile(Box::new(dark));
        pipeline.perform_rebuilds();

        let dark_r = LAST_SEEN_PRIMARY.load(Ordering::SeqCst);
        assert_ne!(
            dark_r, light_r,
            "reader should have rebuilt with the dark theme's primary"
        );
    }
```

**Pipeline method check:** Verify `pipeline.perform_rebuilds()` is the correct public/pub(crate) method name. Run `rg "fn perform_rebuilds|fn drain_dirty|fn rebuild" vexo/src/pipeline.rs` and adjust. If rebuilds happen automatically inside `reconcile()`, drop the explicit `perform_rebuilds()` call.

- [ ] **Step 6: Add the no-provider fallback test**

```rust
    #[derive(Clone)]
    struct NoThemeReader;

    #[derive(Default)]
    struct NoThemeReaderState;

    impl ComponentState for NoThemeReaderState {}

    impl Component for NoThemeReader {
        type State = NoThemeReaderState;

        fn render(&self, _state: &mut NoThemeReaderState, ctx: &mut RenderContext) -> Box<dyn Widget> {
            let data = Theme::of(ctx);
            // Should be the light fallback.
            let r = match data.primary {
                crate::Color::Rgb { r, .. } => r as u32,
                _ => 0,
            };
            LAST_SEEN_PRIMARY.store(r, Ordering::SeqCst);
            Box::new(Text::new("no-theme"))
        }
    }

    #[test]
    fn theme_of_returns_light_fallback_without_provider() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        LAST_SEEN_PRIMARY.store(0, Ordering::SeqCst);

        pipeline.reconcile(Box::new(NoThemeReader));
        pipeline.perform_rebuilds();

        let r = LAST_SEEN_PRIMARY.load(Ordering::SeqCst);
        let light_r = match ThemeData::light().primary {
            crate::Color::Rgb { r, .. } => r as u32,
            _ => 0,
        };
        assert_eq!(r, light_r, "Theme::of should fall back to light()");
    }
```

- [ ] **Step 7: Add the nested-providers test**

```rust
    #[derive(Clone)]
    struct NestedReader {
        inner: bool, // true = read inside inner Theme
    }

    #[derive(Default)]
    struct NestedReaderState;

    impl ComponentState for NestedReaderState {}

    impl Component for NestedReader {
        type State = NestedReaderState;

        fn render(&self, _state: &mut NestedReaderState, ctx: &mut RenderContext) -> Box<dyn Widget> {
            let data = Theme::of(ctx);
            let r = match data.primary {
                crate::Color::Rgb { r, .. } => r as u32,
                _ => 0,
            };
            LAST_SEEN_PRIMARY.store(r, Ordering::SeqCst);
            Box::new(Text::new("nested"))
        }
    }

    #[test]
    fn nested_themes_nearest_wins() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        LAST_SEEN_PRIMARY.store(0, Ordering::SeqCst);

        // Theme(dark) → Theme(light) → NestedReader
        let tree = Theme::new(
            ThemeData::dark(),
            Theme::new(ThemeData::light(), NestedReader { inner: true }),
        );
        pipeline.reconcile(Box::new(tree));
        pipeline.perform_rebuilds();

        let r = LAST_SEEN_PRIMARY.load(Ordering::SeqCst);
        let light_r = match ThemeData::light().primary {
            crate::Color::Rgb { r, .. } => r as u32,
            _ => 0,
        };
        assert_eq!(r, light_r, "nearest (inner) Theme should win");
    }
```

- [ ] **Step 8: Run all integration tests**

Run: `cargo test -p vexo inherited_integration_test`
Expected: PASS (4 tests).

- [ ] **Step 9: Run the full test suite to catch regressions**

Run: `cargo test -p vexo`
Expected: PASS (all pre-existing + new tests).

- [ ] **Step 10: Commit**

```bash
git add vexo/src/inherited_integration_test.rs vexo/src/lib.rs
git commit -m "test(inherited): element-level integration tests for Theme provider"
```

---

### Task 9: Update CLAUDE.md docs

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add `InheritedWidget` to the module structure and web-developer API mapping**

In `CLAUDE.md`, find the "Module Structure" section and add after the `stateful_widget.rs` entry:

```
├── inherited_registry.rs       # InheritedRegistry, InheritedMap
├── inherited_widget.rs         # InheritedWidget trait, InheritedElement
```

Find the "Web Developer API Mapping" table and add:

```
| `InheritedWidget` trait | React Context Provider / Vue provide() |
| `RenderContext::depend_on_inherited_widget::<V>()` | React `useContext()` / Vue `inject()` |
| `Theme` / `ThemeData` | CSS custom properties / Tailwind theme |
```

- [ ] **Step 2: Add key file locations**

Find "Key File Locations" and add:

```
- InheritedWidget trait: `vexo/src/inherited_widget.rs`
- InheritedRegistry: `vexo/src/inherited_registry.rs`
- Theme widget: `vexo/src/widgets/theme.rs`
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: document InheritedWidget and Theme in CLAUDE.md"
```

---

## Self-Review Checklist

After implementing all tasks, run this checklist:

1. **Spec coverage:**
   - InheritedWidget trait (separate from Component) → Task 4 ✓
   - InheritedElement (mount/update/unmount) → Task 4 ✓
   - InheritedRegistry (values + dependents, RefCell) → Task 2 ✓
   - InheritedMap (per-element cache, Arc COW) → Task 1 ✓
   - Blanket impl Widget for InheritedWidget → Task 4 ✓
   - RenderContext::depend_on_inherited_widget → Task 6 ✓
   - ThemeData + Theme widget + Theme::of(ctx) → Task 7 ✓
   - Pipeline ownership + reconciler wiring → Task 5 ✓
   - Integration tests (mount, update→rebuild, fallback, nested) → Task 8 ✓
   - No migration of safe_area_source / focused_element → confirmed out of scope ✓
   - No aspect-based (InheritedModel) → confirmed out of scope ✓

2. **Placeholder scan:** Search the plan for `TBD`, `TODO`, `fill in`, `similar to`. The only intentional elisions are in Task 5 Step 1 ("find every call site") and Task 8 Step 5 ("verify method name") — these are verification steps, not placeholders.

3. **Type consistency:**
   - `InheritedMap::get(TypeId) -> Option<ElementKey>` — used consistently in Tasks 1, 6.
   - `InheritedRegistry::value_clone::<V>(ElementKey) -> Option<V>` — used in Task 6. Task 2 ships only this form (clone-inside-borrow).
   - `RenderContext::depend_on_inherited_widget::<V>() -> Option<V>` — used in Task 7 (`Theme::of`).
   - `InheritedWidget::Value: Clone + PartialEq` — consistent across Tasks 4, 7.

4. **Ambiguity check:** The plan originally had an ambiguity between `Ref`-returning and clone-returning `InheritedRegistry::value`. Resolved: ship only `value_clone` (simpler, no `RefCell::Ref` lifetime issues).

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-12-inherited-widget.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
