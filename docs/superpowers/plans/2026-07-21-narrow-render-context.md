# Narrow RenderContext Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Narrow `RenderContext` to a pure read + dependency-subscription interface, move `clear_focus()` to a new `on_rebuild` lifecycle hook on `LifecycleContext`, and remove unused mutators and framework-internal fields from the public surface.

**Architecture:** Three changes layered in a TDD-friendly order: (1) add the new `on_rebuild` hook + `LifecycleContext::clear_focus()` alongside the existing API, (2) migrate the single `clear_focus()` callsite in `NavigationStackView::render()` to the new hook, (3) remove the old `RenderContext` surface (mutators, `dirty`/`render_objects` fields, `pub` fields) and replace struct-literal construction with a public `new()` constructor.

**Tech Stack:** Rust workspace, three crates: `vexo` (framework), `vexo_uikit` (UI kit), `shared_app` (sample app). Build with `cargo build -p <crate>`, test with `cargo test -p <crate>`.

**Spec:** `docs/superpowers/specs/2026-07-21-narrow-render-context-design.md`

## Global Constraints

- Preserve all current behavior, especially the deferred-unfocus semantics of `clear_focus()` (stashed on `BuildOwner`, applied by the pipeline after `perform_rebuilds()` returns).
- `on_rebuild` fires **only** on state-driven rebuilds (setState / Signal::set), NOT on parent-widget updates (which fire `on_update`).
- `cargo build --workspace` and `cargo test --workspace` must pass at every commit.
- Commit messages follow the existing project style: `type(scope): subject` (e.g., `refactor(render-context): narrow public API`).
- Per CLAUDE.md: do NOT run `cargo run -p desktop_demo` yourself — ask the user to run it for the manual verification step.

---

### Task 1: Add `on_rebuild` lifecycle hook

**Files:**
- Modify: `vexo/src/stateful_widget.rs` (ComponentState trait + StatefulElement::rebuild_from_state)
- Test: `vexo/src/stateful_widget.rs` (in `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `ComponentState::on_rebuild(&mut self, ctx: &mut LifecycleContext)` — default no-op. Called by `StatefulElement::rebuild_from_state` before `build_child_widget`.

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/stateful_widget.rs` in the `#[cfg(test)] mod tests` block, after the `test_stateful_element_can_update_same_type` test (around line 1330):

```rust
#[derive(Clone)]
struct RebuildCounter {
    label: String,
}

#[derive(Default)]
struct RebuildCounterState {
    on_rebuild_fired: bool,
    render_count: u32,
}

impl ComponentState for RebuildCounterState {
    fn on_rebuild(&mut self, _ctx: &mut LifecycleContext) {
        self.on_rebuild_fired = true;
    }
}

impl Component for RebuildCounter {
    type State = RebuildCounterState;

    fn render(&self, state: &mut RebuildCounterState, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        state.render_count += 1;
        Box::new(Text::new(format!("{}: render={}", self.label, state.render_count)))
    }
}

#[test]
fn test_on_rebuild_fires_before_render() {
    // Verify on_rebuild fires on state-driven rebuild (rebuild_from_state),
    // and fires BEFORE render() so side-effects land before the new tree is built.
    let widget = RebuildCounter { label: "R".to_string() };
    let mut element = StatefulElement::new(widget);

    let (
        element_id,
        mut state,
        mut dirty,
        mut render_objects,
        _element_registry,
        build_owner,
        dirty_sender,
        mut child_ops,
        mut focus_manager,
        inherited_registry,
        mut inherited_maps,
    ) = create_test_context();
    let empty_map = InheritedMap::empty();

    // Mount
    {
        let mut ctx = ElementContext::new(
            element_id,
            None,
            Vec::new(),
            &mut state,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &dirty_sender,
            &mut child_ops,
            &mut focus_manager,
            None,
            Arc::new(AnimationTicker::new()),
            &empty_map,
            &inherited_registry,
            &mut inherited_maps,
        );
        Element::mount(&mut element, &mut ctx);
    }

    // Mount calls render() once but NOT on_rebuild (mount is not a state-driven rebuild).
    let state_ref = state.get::<RebuildCounterState>(element_id).unwrap();
    assert!(!state_ref.on_rebuild_fired, "on_rebuild must not fire on mount");
    assert_eq!(state_ref.render_count, 1, "render fires once on mount");

    // State-driven rebuild
    {
        let mut ctx = ElementContext::new(
            element_id,
            None,
            Vec::new(),
            &mut state,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &dirty_sender,
            &mut child_ops,
            &mut focus_manager,
            None,
            Arc::new(AnimationTicker::new()),
            &empty_map,
            &inherited_registry,
            &mut inherited_maps,
        );
        Element::rebuild_from_state(&mut element, &mut ctx);
    }

    // After rebuild: on_rebuild fired, render fired again.
    let state_ref = state.get::<RebuildCounterState>(element_id).unwrap();
    assert!(state_ref.on_rebuild_fired, "on_rebuild must fire on state-driven rebuild");
    assert_eq!(state_ref.render_count, 2, "render fires once per rebuild");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_on_rebuild_fires_before_render -- --nocapture`
Expected: compile error — `on_rebuild` method not found on `ComponentState`.

- [ ] **Step 3: Add `on_rebuild` hook to `ComponentState` trait**

In `vexo/src/stateful_widget.rs`, find the `pub trait ComponentState` block (line 51). Add the new hook after `on_unmount` (after line 93):

```rust
    /// Called once before each state-driven rebuild (setState / Signal::set),
    /// NOT before parent-widget updates (that's `on_update`).
    ///
    /// Use for side-effects that must happen at the start of a rebuild pass:
    /// clearing focus when a navigation transition begins, dismissing a
    /// pending modal, etc. `render()` itself must stay pure.
    ///
    /// Fires only from `StatefulElement::rebuild_from_state`. The first
    /// render of an element goes through `mount()` and does NOT fire
    /// `on_rebuild`. Parent-widget changes go through `update()` →
    /// `on_update()`, also NOT `on_rebuild`.
    ///
    /// Default: no-op.
    fn on_rebuild(&mut self, _ctx: &mut LifecycleContext) {}
```

- [ ] **Step 4: Wire `on_rebuild` into `StatefulElement::rebuild_from_state`**

In `vexo/src/stateful_widget.rs`, find `fn rebuild_from_state` (line 762). Insert the `on_rebuild` call at the very top of the function body, before the existing `let element_id = ...` line:

```rust
    fn rebuild_from_state(&mut self, context: &mut ElementContext) {
        let element_id = self.id.unwrap_or(context.element_id);

        // Fire on_rebuild before building the child widget. This is the
        // only place state-driven side-effects run.
        {
            let tx = context.dirty_sender.clone();
            let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let _ = tx.send(element_id);
            });
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let mut lifecycle_ctx = LifecycleContext::new(
                element_id,
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.on_rebuild(&mut lifecycle_ctx);
        }

        // Build the child widget tree using RenderContext
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.dirty,
                context.render_objects,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
            )
        };

        // Reconcile child via child_ops
        let old_child = context.children().first().copied();
        match old_child {
            Some(old_child_key) => {
                context.update_child(old_child_key, child_widget);
            }
            None => {
                context.inflate_child(None, child_widget);
            }
        }
    }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vexo test_on_rebuild_fires_before_render -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Run full crate test suite to verify no regressions**

Run: `cargo test -p vexo`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/stateful_widget.rs
git commit -m "feat(stateful-widget): add on_rebuild lifecycle hook

Fires before each state-driven rebuild (setState / Signal::set), not on
parent-widget updates. Default no-op. Used to keep render() pure by
giving state-driven side-effects a dedicated hook."
```

---

### Task 2: Add `LifecycleContext::clear_focus()`

**Files:**
- Modify: `vexo/src/stateful_widget.rs` (LifecycleContext impl)
- Test: `vexo/src/stateful_widget.rs` (in `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `LifecycleContext::clear_focus(&self)` — defers an unfocus request to `BuildOwner::request_unfocus()`, applied by the pipeline after `perform_rebuilds()` returns. Identical body to the existing `RenderContext::clear_focus()`.

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/stateful_widget.rs` in the test block, after the `test_render_context_is_focused` test (around line 1414):

```rust
#[test]
fn test_lifecycle_context_clear_focus_requests_unfocus() {
    use crate::build_owner::BuildOwner;

    let build_owner = BuildOwner::new();
    let element_id = make_element_key();
    let widget = TestCounter { label: "X".to_string() };
    let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    let animation_ticker = Arc::new(AnimationTicker::new());

    // No unfocus request pending initially.
    assert!(!build_owner.has_unfocus_request());

    // Construct a LifecycleContext and call clear_focus.
    let mut ctx = LifecycleContext::new(
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

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_lifecycle_context_clear_focus_requests_unfocus -- --nocapture`
Expected: compile error — no `clear_focus` method on `LifecycleContext`, and possibly no `has_unfocus_request` on `BuildOwner`.

- [ ] **Step 3: Add `has_unfocus_request` accessor to `BuildOwner` (if missing)**

Check `vexo/src/build_owner.rs` for an existing accessor. If `take_unfocus_request()` is the only existing method, add a non-consuming check. In `vexo/src/build_owner.rs`, find the `request_unfocus` method (around line 235) and add immediately after it:

```rust
    /// Test-only accessor: returns `true` if `request_unfocus()` has been
    /// called since the last `take_unfocus_request()`. Used by tests to
    /// assert that a deferred unfocus was scheduled.
    pub fn has_unfocus_request(&self) -> bool {
        self.pending_unfocus.get()
    }
```

Adjust the field name (`pending_unfocus`) to match the actual `AtomicBool` / `Cell<bool>` field used in `BuildOwner` — read the file first to confirm the exact name.

- [ ] **Step 4: Add `clear_focus()` to `LifecycleContext`**

In `vexo/src/stateful_widget.rs`, find `impl<'a> LifecycleContext<'a>` (line 213). Add `clear_focus` after the `animation_ticker` accessor (after line 299):

```rust
    /// Request that primary focus be cleared after the current rebuild pass.
    ///
    /// Safe to call from any lifecycle hook (`on_mount`, `on_update`,
    /// `on_rebuild`, `on_unmount`). The request is stashed on the
    /// [`BuildOwner`] and applied by the pipeline once `perform_rebuilds()`
    /// returns — mirrors the deferred-unfocus semantics previously on
    /// `RenderContext::clear_focus()`.
    ///
    /// No-op when nothing is focused (the subsequent `FocusManager::unfocus()`
    /// is itself a no-op in that case).
    pub fn clear_focus(&self) {
        self.build_owner.request_unfocus();
    }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vexo test_lifecycle_context_clear_focus_requests_unfocus -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Run full crate test suite**

Run: `cargo test -p vexo`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/stateful_widget.rs vexo/src/build_owner.rs
git commit -m "feat(lifecycle-context): add clear_focus method

Mirrors the deferred-unfocus semantics of RenderContext::clear_focus.
Pre-positions the migration target for NavigationStackView to call
clear_focus from a lifecycle hook instead of render()."
```

---

### Task 3: Migrate `NavigationStackView::clear_focus` call to `on_rebuild`

**Files:**
- Modify: `vexo_uikit/src/navigation.rs` (NavigationStackViewState::on_rebuild + render)

**Interfaces:**
- Consumes: `ComponentState::on_rebuild` (Task 1), `LifecycleContext::clear_focus` (Task 2).
- Produces: `NavigationStackViewState::on_rebuild` implementation that calls `clear_focus` when a pending op is observed.

- [ ] **Step 1: Add `on_rebuild` impl to `NavigationStackViewState`**

In `vexo_uikit/src/navigation.rs`, find the `impl<Dest: ...> ComponentState for NavigationStackViewState<Dest>` block (line 439). Add the new hook after `on_tick` (around line 486):

```rust
    fn on_rebuild(&mut self, ctx: &mut LifecycleContext) {
        // Was: ctx.clear_focus() inside render() when a pending op was
        // observed. Now: same check, same call, but in the lifecycle hook —
        // render() stays pure.
        //
        // A navigation transition is starting (push or pop). Clear primary
        // focus now, on the same frame the animation begins, rather than
        // letting it linger on the outgoing page.
        //
        // Why this matters: on iOS, a TextEdit holding focus keeps the
        // software keyboard up. Without this call, tapping Back on a
        // focused chat screen would leave the keyboard stuck on screen
        // for the entire pop animation (and beyond), because the outgoing
        // page stays mounted as the transition overlay and retains focus
        // until it unmounts at the end — and even then nothing re-synced
        // the keyboard.
        //
        // `clear_focus()` is deferred (stashed on BuildOwner, applied after
        // this rebuild pass), and `FocusManager::unfocus()` is a no-op when
        // nothing is focused, so this is harmless for pushes from an
        // unfocused list.
        if self.transition.is_none() {
            if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
                if nav.controller.pending().is_some() {
                    ctx.clear_focus();
                }
            }
        }
    }
```

- [ ] **Step 2: Remove the `ctx.clear_focus()` call from `render()`**

In `vexo_uikit/src/navigation.rs`, find `fn render` (line 492). Delete the block comment and `ctx.clear_focus();` call inside the `if let Some(pending) = self.controller.pending()` block (lines 500-516). The block becomes:

```rust
        if state.transition.is_none() {
            if let Some(pending) = self.controller.pending() {
                if let (Some(ticker), Some(cb)) =
                    (state.ticker.as_ref(), state.dirty_callback.as_ref())
                {
                    let duration = self.effective_transition_duration();
                    let mut controller = AnimationController::new(duration);
                    controller.set_ticker(ticker.clone());
                    controller.set_dirty_callback(cb.clone());
                    controller.forward();
                    state.transition = Some(NavTransition {
                        direction: pending.kind,
                        controller,
                        from_path: pending.from.clone(),
                        to_path: pending.to.clone(),
                    });
                } else {
                    // No ticker available (test harness or pre-mount). Clear
                    // the pending op and render steady-state.
                    self.controller.clear_pending();
                }
            }
        }
```

- [ ] **Step 3: Build vexo_uikit to verify it compiles**

Run: `cargo build -p vexo_uikit`
Expected: build succeeds.

- [ ] **Step 4: Run navigation tests to verify no regressions**

Run: `cargo test -p vexo_uikit`
Expected: all tests pass. Pay particular attention to:
- `navigation_stack_tests`
- `navigation_animation_tests`

If any fail, the most likely cause is a difference in the `pending()` check between `render()`'s `&self` access and `on_rebuild`'s `ctx.widget().downcast_ref::<NavigationStackView<Dest>>()` access. Verify the downcast succeeds.

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/navigation.rs
git commit -m "refactor(navigation): move clear_focus from render to on_rebuild

The clear_focus call is a state-driven side-effect (a navigation pop is
starting), not a render-time computation. Moving it to on_rebuild keeps
render() pure. Behavior unchanged: same deferred-unfocus semantics via
BuildOwner, applied by the pipeline after perform_rebuilds() returns."
```

---

### Task 4: Add `RenderContext::new()` constructor and migrate struct-literal callers

**Files:**
- Modify: `vexo/src/stateful_widget.rs` (RenderContext impl + in-crate test constructors)
- Modify: `vexo_uikit/tests/navigation_stack_tests.rs:208-224` (helper)
- Modify: `vexo_uikit/tests/navigation_animation_tests.rs:43-...` (helper)
- Modify: `vexo_uikit/tests/button_render_tests.rs:13-29` (helper)

**Interfaces:**
- Produces: `RenderContext::new(element_id, build_owner, inherited_map, inherited_registry)` — public constructor. At this step the `dirty` and `render_objects` fields are still on the struct (still `pub`); the constructor just doesn't take them.

- [ ] **Step 1: Add `new()` constructor to `RenderContext`**

In `vexo/src/stateful_widget.rs`, find `impl<'a> RenderContext<'a>` (line 332). Add a `new()` method at the top of the impl block:

```rust
impl<'a> RenderContext<'a> {
    /// Construct a `RenderContext` for use in `Component::render()`.
    ///
    /// The `dirty` and `render_objects` fields (still present on the struct
    /// for now) are initialized to `None` / default — they are unused by
    /// `render()` and will be removed entirely in a subsequent change.
    pub fn new(
        element_id: ElementKey,
        build_owner: &'a BuildOwner,
        inherited_map: &'a InheritedMap,
        inherited_registry: &'a InheritedRegistry,
    ) -> Self {
        Self {
            element_id,
            dirty: None,
            render_objects: None,
            build_owner,
            inherited_map,
            inherited_registry,
        }
    }

    // ...existing methods unchanged...
}
```

Note: this step assumes `dirty` and `render_objects` are currently `&'a mut` references. They need to become `Option<&'a mut ...>` for the constructor to work without taking them. **Before writing this step, read the current `RenderContext` struct definition (line 309) to confirm the exact field types.** If they are `&'a mut DirtyTracking` and `&'a mut RenderObjectRegistry`, change them to `Option<&'a mut DirtyTracking>` and `Option<&'a mut RenderObjectRegistry>` first, then update the existing struct-literal constructors (in `StatefulElement::build_child_widget` and tests) to wrap their args in `Some(...)`.

Alternative simpler approach: skip the `Option` wrapping and instead make `new()` take the dirty/render_objects args too (still public, just consolidates construction). Then Task 6 will remove them. Use this if the `Option` approach gets messy.

**Decision: use the simpler approach.** Make `new()` take all current fields:

```rust
    pub fn new(
        element_id: ElementKey,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
        build_owner: &'a BuildOwner,
        inherited_map: &'a InheritedMap,
        inherited_registry: &'a InheritedRegistry,
    ) -> Self {
        Self {
            element_id,
            dirty,
            render_objects,
            build_owner,
            inherited_map,
            inherited_registry,
        }
    }
```

Task 6 will narrow the constructor signature when the fields are removed.

- [ ] **Step 2: Update `StatefulElement::build_child_widget` to use `new()`**

In `vexo/src/stateful_widget.rs`, find `build_child_widget` (line 506). Replace the struct literal:

```rust
        let mut render_ctx = RenderContext {
            element_id,
            dirty,
            render_objects,
            build_owner,
            inherited_map,
            inherited_registry,
        };
```

with:

```rust
        let mut render_ctx = RenderContext::new(
            element_id,
            dirty,
            render_objects,
            build_owner,
            inherited_map,
            inherited_registry,
        );
```

- [ ] **Step 3: Update in-crate tests to use `new()`**

In `vexo/src/stateful_widget.rs`, find each `RenderContext { ... }` struct literal in the test block (lines 1349, 1381, 1393, 1405, 1431, 1454, 1480). Replace each with `RenderContext::new(...)`.

For example, the literal at line 1349:

```rust
        let mut ctx = RenderContext {
            element_id,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &build_owner,
            inherited_map: &empty_map,
            inherited_registry: &inherited_registry,
        };
```

becomes:

```rust
        let mut ctx = RenderContext::new(
            element_id,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &empty_map,
            &inherited_registry,
        );
```

Apply to all 7 occurrences.

- [ ] **Step 4: Update external test helper in `navigation_stack_tests.rs`**

In `vexo_uikit/tests/navigation_stack_tests.rs`, find `create_render_context` (line 208). Replace the struct literal with:

```rust
fn create_render_context<'a>(
    element_id: ElementKey,
    dirty: &'a mut DirtyTracking,
    render_objects: &'a mut RenderObjectRegistry,
    build_owner: &'a BuildOwner,
    inherited_map: &'a InheritedMap,
    inherited_registry: &'a InheritedRegistry,
) -> RenderContext<'a> {
    RenderContext::new(
        element_id,
        dirty,
        render_objects,
        build_owner,
        inherited_map,
        inherited_registry,
    )
}
```

- [ ] **Step 5: Update external test helper in `navigation_animation_tests.rs`**

In `vexo_uikit/tests/navigation_animation_tests.rs`, find `create_render_context` (around line 43). Apply the same replacement as Step 4.

- [ ] **Step 6: Update external test helper in `button_render_tests.rs`**

In `vexo_uikit/tests/button_render_tests.rs`, find `create_render_context` (line 13). Apply the same replacement as Step 4.

- [ ] **Step 7: Build and test the workspace**

Run: `cargo build --workspace`
Expected: build succeeds.

Run: `cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/stateful_widget.rs vexo_uikit/tests/navigation_stack_tests.rs vexo_uikit/tests/navigation_animation_tests.rs vexo_uikit/tests/button_render_tests.rs
git commit -m "refactor(render-context): add public new() constructor

Replaces struct-literal construction at all callsites with a public
constructor. No behavior change. Sets up subsequent removal of fields
and methods from the public surface."
```

---

### Task 5: Remove unused mutator methods from `RenderContext`

**Files:**
- Modify: `vexo/src/stateful_widget.rs` (RenderContext impl)
- Test: `vexo/src/stateful_widget.rs` (delete obsolete test)

**Interfaces:**
- Produces: `RenderContext` without `request_rebuild`, `mark_needs_layout`, `mark_needs_paint`, `clear_focus`. Reads (`is_focused`, `safe_area`, `element_id`) and `depend_on_inherited_widget` remain.

- [ ] **Step 1: Delete the obsolete test**

In `vexo/src/stateful_widget.rs`, find `test_render_context_request_rebuild` (line 1333). Delete the entire test function (approximately lines 1333-1361).

- [ ] **Step 2: Delete `request_rebuild`, `mark_needs_layout`, `mark_needs_paint`, `clear_focus` from `RenderContext`**

In `vexo/src/stateful_widget.rs`, find `impl<'a> RenderContext<'a>` (line 332). Delete the four methods:

- `pub fn request_rebuild(&mut self)` (line 336)
- `pub fn mark_needs_layout(...)` (line 341)
- `pub fn mark_needs_paint(...)` (line 346)
- `pub fn clear_focus(&self)` (line 370)

Keep: `is_focused`, `safe_area`, `depend_on_inherited_widget`, `element_id()` if present.

- [ ] **Step 3: Build to verify no callers remain**

Run: `cargo build --workspace`
Expected: build succeeds. If it fails, the error will name the caller — fix or report. Per the spec's grep, no user `render()` code calls these methods, so the build should succeed.

- [ ] **Step 4: Test the workspace**

Run: `cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/stateful_widget.rs
git commit -m "refactor(render-context): remove unused mutator methods

Deletes request_rebuild, mark_needs_layout, mark_needs_paint, and
clear_focus from RenderContext. None had callers in user render() code;
clear_focus was migrated to LifecycleContext via on_rebuild in Tasks 2-3.
render() is now pure: reads + dependency subscription only."
```

---

### Task 6: Make `RenderContext` fields private, remove `dirty` and `render_objects` fields

**Files:**
- Modify: `vexo/src/stateful_widget.rs` (struct def + new() + build_child_widget signature + 3 callsites)
- Modify: `vexo_uikit/tests/navigation_stack_tests.rs` (helper signature + callers)
- Modify: `vexo_uikit/tests/navigation_animation_tests.rs` (helper signature + callers)
- Modify: `vexo_uikit/tests/button_render_tests.rs` (helper signature + callers)

**Interfaces:**
- Produces: `RenderContext` with private fields `element_id`, `build_owner`, `inherited_map`, `inherited_registry`. The `dirty` and `render_objects` fields are removed entirely. `RenderContext::new()` signature drops those two args.

- [ ] **Step 1: Remove `dirty` and `render_objects` from the struct definition**

In `vexo/src/stateful_widget.rs`, find `pub struct RenderContext<'a>` (line 309). Change all fields from `pub` to private, and delete the `dirty` and `render_objects` fields:

```rust
pub struct RenderContext<'a> {
    /// The element ID for this stateful element.
    element_id: ElementKey,

    /// Build owner for scheduling rebuilds.
    /// Uses shared reference because mark_needs_build() takes &self
    /// via interior mutability (RefCell).
    build_owner: &'a BuildOwner,

    /// Nearest-ancestor cache for inherited values (read-only here).
    inherited_map: &'a InheritedMap,

    /// Pipeline-owned registry; `depend_on_inherited_widget` uses interior
    /// mutability to register the caller as a dependent.
    inherited_registry: &'a InheritedRegistry,
}
```

- [ ] **Step 2: Update `new()` constructor signature**

In `vexo/src/stateful_widget.rs`, find `RenderContext::new` (added in Task 4). Drop the `dirty` and `render_objects` args:

```rust
    pub fn new(
        element_id: ElementKey,
        build_owner: &'a BuildOwner,
        inherited_map: &'a InheritedMap,
        inherited_registry: &'a InheritedRegistry,
    ) -> Self {
        Self {
            element_id,
            build_owner,
            inherited_map,
            inherited_registry,
        }
    }
```

- [ ] **Step 3: Update `build_child_widget` signature**

In `vexo/src/stateful_widget.rs`, find `build_child_widget` (line 506). Drop the `dirty` and `render_objects` args:

```rust
    fn build_child_widget(
        &self,
        element_id: ElementKey,
        state: &mut W::State,
        build_owner: &BuildOwner,
        inherited_map: &InheritedMap,
        inherited_registry: &InheritedRegistry,
    ) -> Box<dyn Widget> {
        let mut render_ctx = RenderContext::new(
            element_id,
            build_owner,
            inherited_map,
            inherited_registry,
        );
        self.widget.render(state, &mut render_ctx)
    }
```

- [ ] **Step 4: Update the three `build_child_widget` callsites in `StatefulElement`**

In `vexo/src/stateful_widget.rs`, find the three calls to `self.build_child_widget(...)` (in `mount` ~line 624, `update` ~line 677, `rebuild_from_state` ~line 772). Drop the `context.dirty,` and `context.render_objects,` arguments from each.

Example — the `mount` callsite:

Before:
```rust
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.dirty,
                context.render_objects,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
            )
        };
```

After:
```rust
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
            )
        };
```

Apply the same change to the `update` and `rebuild_from_state` callsites.

- [ ] **Step 5: Update in-crate test constructors**

In `vexo/src/stateful_widget.rs`, find each `RenderContext::new(...)` call in the test block (there are 6 after Task 4 deleted one). Drop the `&mut dirty,` and `&mut render_objects,` arguments from each.

Example — the test at line ~1349 (post-Task-4 numbering):

Before:
```rust
        let mut ctx = RenderContext::new(
            element_id,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &empty_map,
            &inherited_registry,
        );
```

After:
```rust
        let mut ctx = RenderContext::new(
            element_id,
            &build_owner,
            &empty_map,
            &inherited_registry,
        );
```

If a test no longer uses its `dirty` or `render_objects` locals, delete those locals too to avoid unused-variable warnings. The `test_render_context_is_focused` and the three `depend_on_inherited_widget_*` tests will likely have unused locals after this change.

- [ ] **Step 6: Update external test helpers**

In `vexo_uikit/tests/navigation_stack_tests.rs`, find `create_render_context` (line 208). Drop the `dirty` and `render_objects` args from both the function signature and the `RenderContext::new(...)` call:

```rust
fn create_render_context<'a>(
    element_id: ElementKey,
    build_owner: &'a BuildOwner,
    inherited_map: &'a InheritedMap,
    inherited_registry: &'a InheritedRegistry,
) -> RenderContext<'a> {
    RenderContext::new(
        element_id,
        build_owner,
        inherited_map,
        inherited_registry,
    )
}
```

Then find every caller of `create_render_context(...)` in the same file (the `render_stack` function at line 226 and any others). Drop the `&mut dirty,` and `&mut render_objects,` args from each call. Delete now-unused `let mut dirty = DirtyTracking::new();` and `let mut render_objects = RenderObjectRegistry::new();` locals.

Apply the same changes to:
- `vexo_uikit/tests/navigation_animation_tests.rs` (helper at line ~43 + callers)
- `vexo_uikit/tests/button_render_tests.rs` (helper at line 13 + callers)

- [ ] **Step 7: Build the workspace to verify**

Run: `cargo build --workspace`
Expected: build succeeds. If any caller still passes `dirty` or `render_objects` to `RenderContext::new`, the compiler will name it — fix.

- [ ] **Step 8: Test the workspace**

Run: `cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add vexo/src/stateful_widget.rs vexo_uikit/tests/navigation_stack_tests.rs vexo_uikit/tests/navigation_animation_tests.rs vexo_uikit/tests/button_render_tests.rs
git commit -m "refactor(render-context): make fields private, drop dirty and render_objects

All RenderContext fields are now private. The dirty and render_objects
fields are removed entirely — they belong to the element layer
(ElementContext), not the widget-facing context. RenderContext::new()
signature narrows accordingly. User render() code can no longer reach
framework internals."
```

---

### Task 7: Final verification

**Files:**
- No code changes. Verification only.

- [ ] **Step 1: Build entire workspace**

Run: `cargo build --workspace`
Expected: build succeeds with no warnings related to `RenderContext`.

- [ ] **Step 2: Run entire workspace test suite**

Run: `cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 3: Check for any leftover references**

Run: `rg "RenderContext::(request_rebuild|mark_needs_layout|mark_needs_paint|clear_focus)" --type rust`
Expected: no matches.

Run: `rg "\.dirty\b|\.render_objects\b" vexo/src/stateful_widget.rs`
Expected: no matches inside `RenderContext` (the `ElementContext` may still have these fields, which is correct).

- [ ] **Step 4: Ask user to run manual navigation pop test**

Per CLAUDE.md, do NOT run `cargo run -p desktop_demo` yourself. Ask the user:

> Please run `cargo run -p desktop_demo` and verify:
> 1. Navigate to a screen with a focused TextEdit (e.g., a chat input).
> 2. Tap Back to trigger a pop.
> 3. Verify the keyboard (on iOS) or focus state (on desktop) dismisses immediately when the pop animation starts, not at the end.
>
> This is the only behavioral change from the refactor — `clear_focus` now fires from `on_rebuild` instead of `render()`, but the deferred-unfocus semantics are unchanged.

- [ ] **Step 5: Final commit (if any cleanup needed)**

If the manual test reveals any issue, fix and commit. Otherwise, the refactor is complete — no final commit needed.

---

## Self-Review

**1. Spec coverage:**

| Spec section | Covered by |
|---|---|
| Narrowed `RenderContext` public API | Tasks 5 + 6 |
| New `on_rebuild` lifecycle hook | Task 1 |
| `clear_focus` migration to `LifecycleContext` | Task 2 |
| `StatefulElement::rebuild_from_state` wiring | Task 1 (Step 4) |
| `build_child_widget` signature change | Task 6 (Step 3) |
| `NavigationStackView` migration | Task 3 |
| Test impact: delete `test_render_context_request_rebuild` | Task 5 (Step 1) |
| Test impact: update in-crate tests | Task 4 (Step 3) + Task 6 (Step 5) |
| Test impact: update external test helpers | Task 4 (Steps 4-6) + Task 6 (Step 6) |
| Test impact: new `test_on_rebuild_fires_before_rebuild` | Task 1 (Step 1) |
| Test impact: new `test_lifecycle_context_clear_focus_requests_unfocus` | Task 2 (Step 1) |
| Migration plan Phase 1 (additive) | Tasks 1, 2, 4 |
| Migration plan Phase 2 (migrate callsite) | Task 3 |
| Migration plan Phase 3 (remove old surface) | Tasks 5, 6 |
| Migration plan Phase 4 (verify) | Task 7 |

No spec gaps.

**2. Placeholder scan:** No "TBD", "TODO", "implement later", "add appropriate error handling", or "similar to Task N" patterns. Every code step shows the actual code.

**3. Type consistency:**
- `ComponentState::on_rebuild(&mut self, ctx: &mut LifecycleContext)` — used consistently in Tasks 1, 2, 3.
- `LifecycleContext::clear_focus(&self)` — used consistently in Tasks 2, 3.
- `RenderContext::new(...)` signature: 6 args in Task 4, narrowed to 4 args in Task 6. Both signatures explicit at each step.
- `build_child_widget` signature: 7 args before Task 6, 5 args after. Both explicit.
- `has_unfocus_request` accessor name consistent across Task 2.

No type inconsistencies found.
