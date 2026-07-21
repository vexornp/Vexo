# Narrow LifecycleContext Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the dead `setState`, `request_rebuild`, and `element_id()` methods (plus the now-unused `element_id` field) from `LifecycleContext`, and reword docstrings/comments that reference the removed `setState` to describe the actual rebuild path (`Signal::set` / dirty callback).

**Architecture:** Single-phase surface narrowing in one file (`vexo/src/stateful_widget.rs`) plus two comment rewrites in `vexo/src/pipeline.rs` and `vexo/src/reconciler.rs`. No behavior change — the removed methods are dead (zero callers in user lifecycle hooks). The `element_id` field is consumed only by the removed methods and goes with them. The four remaining methods (`widget`, `dirty_callback`, `animation_ticker`, `clear_focus`) and the four internal `LifecycleContext::new` callsites are unchanged except for dropping the `element_id` arg.

**Tech Stack:** Rust workspace (`vexo`, `vexo_uikit`, `shared_app` crates). Standard `cargo build` / `cargo test --workspace` workflow per `CLAUDE.md`.

## Global Constraints

- All `LifecycleContext` fields remain private (they already are).
- `LifecycleContext::new()` stays module-private (`fn new`, not `pub fn new`). Do not widen visibility.
- No changes to `RenderContext`, `EventContext`, `ElementContext`, `BuildOwner`, or `ComponentState` trait method signatures.
- No changes to user-visible behavior. The removed methods have zero callers in lifecycle hooks (verified by grep).
- The migration is single-phase: one commit. No additive-then-remove staging needed because there is no live callsite to migrate.
- The two `setState` references in `vexo/src/build_owner.rs:30` and `vexo/src/pipeline.rs:692` are Flutter-model comparisons, NOT references to Vexo's removed method. Leave them alone.
- Run `cargo build -p vexo`, `cargo build -p vexo_uikit`, `cargo build -p shared_app`, then `cargo test --workspace` after the edits. Do not run `cargo run -p desktop_demo` (per CLAUDE.md).

---

### Task 1: Narrow `LifecycleContext` struct and constructor

**Files:**
- Modify: `vexo/src/stateful_widget.rs:193-243` (struct + constructor + struct docstring)

**Interfaces:**
- Consumes: `BuildOwner`, `AnimationTicker`, `Arc<dyn Fn() + Send + Sync>` (all unchanged).
- Produces: `LifecycleContext::new(build_owner, widget, dirty_callback, animation_ticker)` — module-private constructor with the `element_id` arg dropped. The four internal callsites in Task 2 will be updated to match.

- [ ] **Step 1: Replace the struct docstring**

In `vexo/src/stateful_widget.rs`, find the docstring starting at line 193:

```rust
/// Context provided to `ComponentState` lifecycle methods.
///
/// Maps to React's effect context or Vue's lifecycle hook context.
/// The key method is `setState()`, which mutates state and marks the
/// element dirty for rebuild.
///
/// Unlike Flutter's `State.widget` getter, Vexo provides widget access
/// through `LifecycleContext::widget()` since Rust's trait objects cannot
/// be generic over the widget type. Downcast to the concrete type:
/// ```ignore
/// let text_edit = ctx.widget().downcast_ref::<TextEdit>().unwrap();
/// ```
```

Replace with:

```rust
/// Context provided to `ComponentState` lifecycle methods.
///
/// Maps to React's effect context or Vue's lifecycle hook context. State
/// mutations trigger rebuilds through `Signal` (auto-wired by
/// `#[derive(ComponentState)]`) or by calling the `dirty_callback()`
/// exposed here — both end up at `BuildOwner::mark_needs_build()`.
///
/// Unlike Flutter's `State.widget` getter, Vexo provides widget access
/// through `LifecycleContext::widget()` since Rust's trait objects cannot
/// be generic over the widget type. Downcast to the concrete type:
/// ```ignore
/// let text_edit = ctx.widget().downcast_ref::<TextEdit>().unwrap();
/// ```
```

- [ ] **Step 2: Drop the `element_id` field from the struct**

Find the struct definition (currently lines 205-225):

```rust
pub struct LifecycleContext<'a> {
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
    animation_ticker: Arc<AnimationTicker>,
}
```

Replace with (note: `element_id` field and its doc comment removed):

```rust
pub struct LifecycleContext<'a> {
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
    animation_ticker: Arc<AnimationTicker>,
}
```

- [ ] **Step 3: Drop `element_id` from the constructor signature and body**

Find the `new` function (currently lines 227-243):

```rust
impl<'a> LifecycleContext<'a> {
    /// Create a new LifecycleContext. Only called by StatefulElement.
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
```

Replace with:

```rust
impl<'a> LifecycleContext<'a> {
    /// Create a new LifecycleContext. Only called by StatefulElement.
    fn new(
        build_owner: &'a BuildOwner,
        widget: &'a dyn Any,
        dirty_callback: Arc<dyn Fn() + Send + Sync>,
        animation_ticker: Arc<AnimationTicker>,
    ) -> Self {
        Self {
            build_owner,
            widget,
            dirty_callback,
            animation_ticker,
        }
    }
```

- [ ] **Step 4: Do NOT build yet — Task 2 updates the callsites**

The four internal callsites of `LifecycleContext::new` and one test callsite still pass `element_id` as the first arg. They will fail to compile until Task 2 finishes. That is expected — proceed to Task 2 before running `cargo build`.

---

### Task 2: Update the four internal `LifecycleContext::new` callsites

**Files:**
- Modify: `vexo/src/stateful_widget.rs:592-598` (`mount`)
- Modify: `vexo/src/stateful_widget.rs:658-664` (`update`)
- Modify: `vexo/src/stateful_widget.rs:703-709` (`unmount`)
- Modify: `vexo/src/stateful_widget.rs:769-775` (`rebuild_from_state`)

**Interfaces:**
- Consumes: Task 1's new `LifecycleContext::new(build_owner, widget, dirty_callback, animation_ticker)` signature.
- Produces: All four `LifecycleContext::new` callsites match the narrowed signature. The crate compiles after this task.

- [ ] **Step 1: Update the `mount` callsite (line 592)**

Find this block inside `fn mount`:

```rust
        let mut lifecycle_ctx = LifecycleContext::new(
            element_id,
            context.build_owner,
            &self.widget as &dyn Any,
            dirty_callback,
            context.animation_ticker.clone(),
        );
        state.on_mount(&mut lifecycle_ctx);
```

Replace with:

```rust
        let mut lifecycle_ctx = LifecycleContext::new(
            context.build_owner,
            &self.widget as &dyn Any,
            dirty_callback,
            context.animation_ticker.clone(),
        );
        state.on_mount(&mut lifecycle_ctx);
```

- [ ] **Step 2: Update the `update` callsite (line 658)**

Find this block inside `fn update`:

```rust
            let mut lifecycle_ctx = LifecycleContext::new(
                element_id,
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.on_update(&old_widget as &dyn Any, &mut lifecycle_ctx);
```

Replace with:

```rust
            let mut lifecycle_ctx = LifecycleContext::new(
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.on_update(&old_widget as &dyn Any, &mut lifecycle_ctx);
```

- [ ] **Step 3: Update the `unmount` callsite (line 703)**

Find this block inside `fn unmount`:

```rust
                let mut lifecycle_ctx = LifecycleContext::new(
                    id,
                    context.build_owner,
                    &self.widget as &dyn Any,
                    dirty_callback,
                    context.animation_ticker.clone(),
                );
                state.on_unmount(&mut lifecycle_ctx);
```

Replace with:

```rust
                let mut lifecycle_ctx = LifecycleContext::new(
                    context.build_owner,
                    &self.widget as &dyn Any,
                    dirty_callback,
                    context.animation_ticker.clone(),
                );
                state.on_unmount(&mut lifecycle_ctx);
```

- [ ] **Step 4: Update the `rebuild_from_state` callsite (line 769)**

Find this block inside `fn rebuild_from_state`:

```rust
            let mut lifecycle_ctx = LifecycleContext::new(
                element_id,
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.on_rebuild(&mut lifecycle_ctx);
```

Replace with:

```rust
            let mut lifecycle_ctx = LifecycleContext::new(
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.on_rebuild(&mut lifecycle_ctx);
```

- [ ] **Step 5: Verify the crate compiles**

Run: `cargo build -p vexo`
Expected: builds cleanly. If it fails with "expected `element_id`" or "extra argument", re-check that all four callsites were updated.

---

### Task 3: Remove the three dead methods

**Files:**
- Modify: `vexo/src/stateful_widget.rs:245-277` (remove `setState`, `request_rebuild`, `element_id()`)

**Interfaces:**
- Consumes: nothing (the methods are dead — zero callers).
- Produces: `LifecycleContext` public surface is now exactly: `widget()`, `dirty_callback()`, `animation_ticker()`, `clear_focus()`.

- [ ] **Step 1: Remove `setState`**

Find this block (currently lines 245-264):

```rust
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

```

Delete it entirely (including the trailing blank line that separates it from `request_rebuild`).

- [ ] **Step 2: Remove `request_rebuild`**

Find this block (currently lines 266-272):

```rust
    /// Mark this element as needing rebuild without mutating state.
    ///
    /// Useful when an external event requires a rebuild but no state
    /// mutation is needed (e.g., a reactive signal changed).
    pub fn request_rebuild(&self) {
        self.build_owner.mark_needs_build(self.element_id);
    }

```

Delete it entirely (including the trailing blank line).

- [ ] **Step 3: Remove `element_id()`**

Find this block (currently lines 274-277):

```rust
    /// Get the element ID of the owning StatefulElement.
    pub fn element_id(&self) -> ElementKey {
        self.element_id
    }

```

Delete it entirely (including the trailing blank line).

- [ ] **Step 4: Verify the crate compiles**

Run: `cargo build -p vexo`
Expected: builds cleanly. If it fails with "unused import" for `ElementKey`, leave the import alone — `ElementKey` is still used elsewhere in the file (e.g., `StatefulElement.id: Option<ElementKey>`). Only remove the import if `cargo build` explicitly flags it as unused.

---

### Task 4: Update the in-module test callsite

**Files:**
- Modify: `vexo/src/stateful_widget.rs:1503-1527` (`test_lifecycle_context_clear_focus_requests_unfocus`)

**Interfaces:**
- Consumes: Task 1's new `LifecycleContext::new` signature.
- Produces: The single test that constructs `LifecycleContext` directly matches the narrowed signature.

- [ ] **Step 1: Drop the `element_id` arg and its binding in the test**

Find this test (currently lines 1502-1527):

```rust
    #[test]
    fn test_lifecycle_context_clear_focus_requests_unfocus() {
        let build_owner = BuildOwner::new();
        let element_id = make_element_key();
        let widget = TestCounter {
            label: "X".to_string(),
        };
        let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let animation_ticker = Arc::new(AnimationTicker::new());

        // No unfocus request pending initially.
        assert!(!build_owner.has_unfocus_request());

        // Construct a LifecycleContext and call clear_focus.
        let ctx = LifecycleContext::new(
            element_id,
            &build_owner,
            &widget as &dyn Any,
            dirty_callback,
            animation_ticker,
        );
        ctx.clear_focus();

        // BuildOwner should now have a pending unfocus request.
        assert!(build_owner.has_unfocus_request());
    }
```

Replace with:

```rust
    #[test]
    fn test_lifecycle_context_clear_focus_requests_unfocus() {
        let build_owner = BuildOwner::new();
        let widget = TestCounter {
            label: "X".to_string(),
        };
        let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let animation_ticker = Arc::new(AnimationTicker::new());

        // No unfocus request pending initially.
        assert!(!build_owner.has_unfocus_request());

        // Construct a LifecycleContext and call clear_focus.
        let ctx = LifecycleContext::new(
            &build_owner,
            &widget as &dyn Any,
            dirty_callback,
            animation_ticker,
        );
        ctx.clear_focus();

        // BuildOwner should now have a pending unfocus request.
        assert!(build_owner.has_unfocus_request());
    }
```

- [ ] **Step 2: Verify the test compiles and passes**

Run: `cargo test -p vexo test_lifecycle_context_clear_focus_requests_unfocus`
Expected: PASS.

If `make_element_key` is now unused anywhere in the test module, the compiler will warn. Leave the helper alone (it is used by other tests in the same module — verify with `rg "make_element_key" vexo/src/stateful_widget.rs` if unsure).

---

### Task 5: Reword docstrings/comments that reference the removed `setState`

**Files:**
- Modify: `vexo/src/stateful_widget.rs:94` (`on_rebuild` hook doc)
- Modify: `vexo/src/stateful_widget.rs:756` (comment in `rebuild_from_state`)
- Modify: `vexo/src/pipeline.rs:320` (comment on `mark_needs_build`)
- Modify: `vexo/src/reconciler.rs:191` (comment in reconcile function)

**Interfaces:**
- Consumes: nothing — comment-only changes.
- Produces: All docstring/comment references to the removed `setState` either describe the actual rebuild path or are left as Flutter-model comparisons (the two `build_owner.rs:30` and `pipeline.rs:692` references).

- [ ] **Step 1: Reword the `on_rebuild` hook docstring**

In `vexo/src/stateful_widget.rs`, find line 94:

```rust
    /// Called once before each state-driven rebuild (setState / Signal::set),
    /// NOT before parent-widget updates (that's `on_update`).
```

Replace with:

```rust
    /// Called once before each state-driven rebuild (triggered by `Signal::set`
    /// or the dirty callback), NOT before parent-widget updates (that's
    /// `on_update`).
```

- [ ] **Step 2: Reword the `rebuild_from_state` comment**

In `vexo/src/stateful_widget.rs`, find lines 755-757:

```rust
        // Rebuild using the CURRENT widget + updated state.
        // This is called by perform_rebuilds() when setState() or
        // Signal::set() marked this element dirty.
```

Replace with:

```rust
        // Rebuild using the CURRENT widget + updated state.
        // This is called by perform_rebuilds() when a `Signal::set` or
        // dirty-callback invocation marked this element dirty.
```

- [ ] **Step 3: Reword the `pipeline.rs` comment**

In `vexo/src/pipeline.rs`, find line 320:

```rust
    /// Elements call this when their state changes (e.g., setState equivalent).
```

Replace with:

```rust
    /// Elements call this when their state changes (via `Signal::set` or the
    /// dirty callback).
```

- [ ] **Step 4: Reword the `reconciler.rs` comment**

In `vexo/src/reconciler.rs`, find line 191:

```rust
        // First, perform any pending state-driven rebuilds (from setState)
```

Replace with:

```rust
        // First, perform any pending state-driven rebuilds (from
        // `Signal::set` / dirty-callback invocations)
```

- [ ] **Step 5: Verify no stray `setState` references to Vexo's removed method remain**

Run: `rg "setState" vexo/src/stateful_widget.rs vexo/src/pipeline.rs vexo/src/reconciler.rs vexo/src/build_owner.rs`
Expected: Only two hits should remain — `vexo/src/build_owner.rs:30` (Flutter comparison) and `vexo/src/pipeline.rs:692` (Flutter comparison). These are correct; do not modify them.

Any other `setState` hit in those files is a missed reference — fix it to say "`Signal::set` / dirty callback".

---

### Task 6: Final build, test, and commit

**Files:**
- None (verification + commit only).

**Interfaces:**
- Consumes: Tasks 1-5.
- Produces: A single commit that narrows `LifecycleContext` and rewords the docstrings.

- [ ] **Step 1: Build all three crates**

Run: `cargo build -p vexo && cargo build -p vexo_uikit && cargo build -p shared_app`
Expected: All three build cleanly. If `vexo_uikit` or `shared_app` fails with a `setState`/`request_rebuild`/`element_id()` reference on a `LifecycleContext`, that's a hidden callsite the spec's grep missed — re-add the method or update the callsite per the spec's risk mitigation. (The spec's audit found zero such callsites, so this should not happen.)

- [ ] **Step 2: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: All tests pass.

If any test fails with a reference to the removed methods, the spec's audit was wrong. Do NOT silently re-add the methods — investigate the failure, then either update the callsite or document the gap and ask the user.

- [ ] **Step 3: Verify no docstring mentions `setState` as a Vexo method**

Run: `rg "setState" vexo/src/ --type rust`
Expected: Only the two Flutter-model comparisons in `build_owner.rs:30` and `pipeline.rs:692`. No other hits.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/stateful_widget.rs vexo/src/pipeline.rs vexo/src/reconciler.rs
git commit -m "refactor(lifecycle-context): narrow LifecycleContext to used surface only

Remove dead setState/request_rebuild/element_id() methods and the
now-unused element_id field they alone consumed. Reword docstrings
that called setState 'the key method' to describe the actual rebuild
path (Signal::set + dirty_callback → BuildOwner::mark_needs_build).

No behavior change — the removed methods had zero callers in user
lifecycle hooks. Mirrors the RenderContext narrowing applied earlier
today."
```

- [ ] **Step 5: Sanity-check the commit**

Run: `git show --stat HEAD`
Expected: exactly three files modified (`vexo/src/stateful_widget.rs`, `vexo/src/pipeline.rs`, `vexo/src/reconciler.rs`), no others.

---

## Self-Review

**1. Spec coverage:**
- Spec §"Narrowed `LifecycleContext` public API": struct field removal → Task 1 Step 2; method removals → Task 3 Steps 1-3; constructor signature → Task 1 Step 3. ✓
- Spec §"Docstring rewrites" items 1-5: item 1 (struct doc) → Task 1 Step 1; item 2 (`on_rebuild`) → Task 5 Step 1; item 3 (`rebuild_from_state` comment) → Task 5 Step 2; item 4 (`pipeline.rs:320`) → Task 5 Step 3; item 5 (`reconciler.rs:191`) → Task 5 Step 4. ✓
- Spec §"Test impact": the one in-module test → Task 4. The "no tests exist for removed methods" and "no external tests construct LifecycleContext" claims are verified by Task 6 Step 2. ✓
- Spec §"Migration Plan": single-phase, single commit → Task 6 Step 4. `cargo build -p vexo/uikit/shared_app` + `cargo test --workspace` → Task 6 Steps 1-2. ✓
- Spec §"Out of Scope" / "Non-Goals": no task touches `RenderContext`, `EventContext`, `ElementContext`, `BuildOwner`, or `ComponentState` trait signatures. ✓

**2. Placeholder scan:** No TBD/TODO. Every step shows the exact code or command. ✓

**3. Type consistency:** `LifecycleContext::new(build_owner, widget, dirty_callback, animation_ticker)` — same signature used in Task 1 Step 3 (definition) and Tasks 2 & 4 (callsites). Field names match. ✓

No issues found.
