# Unify Layout Ownership — Remove GestureDetector's Layout Field

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove `GestureDetector`'s `layout` field and convert it to a pass-through proxy, so the rule becomes clean: every widget that owns a `layout` field exposes `.with_layout()`; no widget that doesn't own one exposes it.

**Architecture:** `GestureDetectorRenderObject` is converted from a container-node owner to a pass-through proxy (the `DecoratedBoxRenderObject` model): `layout()` returns the child's Taffy node, `apply_layout()` adopts the child's computed bounds, `is_pass_through()` returns `true`. The 3 production call sites that chained `.with_layout(L)` on the detector are migrated to wrap the content in `WithLayout::new(content, L)` instead, preserving hit-testing (the pass-through detector adopts the `WithLayout` child's bounds). `WithLayout` gains an inherent `.with_layout()` for API parity with `MultiChild`/`Stack`/`Grid`.

**Tech Stack:** Rust workspace (`vexo`, `vexo_uikit`, `shared_app` crates). Standard `cargo build` / `cargo test --workspace` workflow per `CLAUDE.md`.

## Global Constraints

- Honor the 2026-07-20 ADR: **no trait-default `.with_layout()` on `Widget`.** `WithLayout::new` injects `Column + Stretch` defaults the caller can't see; a trait method that silently wraps would reintroduce that footgun.
- Do NOT run `cargo run -p desktop_demo` (per `CLAUDE.md` — agent cannot interact with GUI). GUI validation is user-run only.
- `cargo build --workspace` and `cargo test --workspace` must pass at every commit.
- No deprecation period — internal codebase, no external consumers. Remove outright.
- Spec: `docs/superpowers/specs/2026-07-31-unify-layout-ownership-design.md`

---

### Task 1: Add inherent `.with_layout()` to `WithLayout`

This is a pure additive change, independent of the GestureDetector cleanup. It brings `WithLayout` into API parity with `MultiChild`/`Stack`/`Grid`, all of which expose an inherent replace-layout method. Lands first so the unified rule is established before the structural cleanup.

**Files:**
- Modify: `vexo/src/widgets/with_layout.rs` (add inherent method after `with_key` at line 277; add test in the `#[cfg(test)] mod tests` block)
- Test: `vexo/src/widgets/with_layout.rs` (same file)

**Interfaces:**
- Consumes: `Layout` (from `crate::layout`, already imported)
- Produces: `WithLayout::with_layout(self, layout: Layout) -> Self` — replaces the layout field wholesale (same semantics as `MultiChild::with_layout` at `multi_child.rs:64-67`)

- [ ] **Step 1: Write the failing test**

Add this test to the `#[cfg(test)] mod tests` block in `vexo/src/widgets/with_layout.rs`, after the existing `test_with_layout_gap_preserves_padding` test (line 465):

```rust
    #[test]
    fn test_with_layout_inherent_replace() {
        let w = WithLayout::new(Text::new("Hello"), Layout::default().padding(10.0))
            .with_layout(Layout::default().padding(20.0));
        assert_eq!(
            w.layout_ref().padding,
            Some(crate::layout::EdgeInsets::all(20.0)),
            "with_layout must replace the layout wholesale"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_with_layout_inherent_replace -- --nocapture`
Expected: COMPILE ERROR — method `with_layout` not found on `WithLayout`.

- [ ] **Step 3: Add the inherent method**

In `vexo/src/widgets/with_layout.rs`, add this method to the `impl WithLayout` block, immediately after the existing `with_key` method (after line 277):

```rust
    /// Replace the layout.
    ///
    /// Mirrors `MultiChild::with_layout`: replaces the layout field
    /// wholesale. Does NOT re-apply the `Column + Stretch` default
    /// injection that `WithLayout::new` performs — callers who want
    /// those defaults should use `WithLayout::new`.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_with_layout_inherent_replace -- --nocapture`
Expected: PASS

- [ ] **Step 5: Build the vexo crate**

Run: `cargo build -p vexo`
Expected: PASS (no warnings about unused code)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/with_layout.rs
git commit -m "feat: add inherent .with_layout() to WithLayout for parity with MultiChild"
```

---

### Task 2: Convert GestureDetector to pass-through + migrate call sites

This is the structural cleanup. It must land as a single commit because removing `GestureDetector::with_layout()` breaks 5 call sites (3 production, 2 test) immediately — the workspace won't compile until all migrations are done.

The conversion model is `DecoratedBoxRenderObject` (`vexo/src/render_objects/decorated_box.rs:66-103`): `layout()` returns the child's node, `apply_layout()` adopts the child's computed bounds, `is_pass_through()` returns `true`.

**Files:**
- Modify: `vexo/src/widgets/gesture_detector.rs` (widget struct, render object, 4 tests, imports)
- Modify: `vexo_uikit/src/tab_bar.rs:184` (production call site)
- Modify: `shared_app/src/desktop_shell.rs:133` (production call site)
- Modify: `shared_app/src/me/profile_screen.rs:376` (production call site)
- Test: `vexo/src/widgets/gesture_detector.rs` (same file)

**Interfaces:**
- Consumes: `WithLayout::new(child, layout)` from Task 1's crate (already public)
- Produces: `GestureDetector` with no `layout` field, no `.with_layout()` inherent method; `GestureDetectorRenderObject` as a pass-through proxy (`is_pass_through() == true`)

**Spec reconciliation note:** The spec says "Update the two `with_layout` call-site tests (lines 744, 765)." On inspection, all four layout-related tests (lines 739, 753, 761, 772) assert on the removed `gd.layout` / `ro.layout` fields or call `new_with_layout` — they cannot be "updated," only deleted. The new `test_gesture_detector_render_object_is_pass_through` test replaces them as the regression guard. The compile-test (`cargo build --workspace`) enforces that no `gd.with_layout()` call sites survive.

- [ ] **Step 1: Remove the `layout` field and inherent `.with_layout()` from `GestureDetector`**

In `vexo/src/widgets/gesture_detector.rs`, edit the `GestureDetector` struct (lines 62-73). Remove the `layout: Layout,` field:

```rust
pub struct GestureDetector {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    /// Callback invoked when pointer is pressed inside the child bounds.
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
    /// Callback invoked when pointer is released inside the child bounds.
    on_release: Option<Rc<RefCell<dyn FnMut()>>>,
    /// Callback invoked when a tap is recognized (pointer up, having won the
    /// arena). Arena-mediated — does NOT fire if a drag wins instead.
    on_tap: Option<Rc<RefCell<dyn FnMut()>>>,
}
```

Edit `GestureDetector::new` (lines 77-88). Remove the default layout:

```rust
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            on_press: None,
            on_release: None,
            on_tap: None,
        }
    }
```

Delete the entire `with_layout` method (lines 96-108):

```rust
    /// Set the layout for this GestureDetector.
    ///
    /// Overrides the default `Column + Stretch` layout. Use this when the
    /// detector needs to participate in flex sizing (e.g. `flex_grow` to fill
    /// a slot) or center its content (`justify(Center)`).
    ///
    /// The layout is applied at mount time. Changing it on rebuild requires
    /// a new element (different widget type or key); the render object's
    /// layout is not hot-updated.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
```

Edit `impl Clone for GestureDetector` (lines 136-147). Remove the `layout` line:

```rust
impl Clone for GestureDetector {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            on_press: self.on_press.clone(),
            on_release: self.on_release.clone(),
            on_tap: self.on_tap.clone(),
        }
    }
}
```

Edit `create_render_object` in the `Widget` impl (lines 160-164). Use `new()` instead of `new_with_layout`:

```rust
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(GestureDetectorRenderObject::new())
    }
```

- [ ] **Step 2: Convert `GestureDetectorRenderObject` to pass-through**

In `vexo/src/widgets/gesture_detector.rs`, edit the struct (lines 458-463). Remove the `layout` field:

```rust
pub struct GestureDetectorRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

Edit the constructors (lines 465-484). Replace `new_with_layout` with a single `new()`:

```rust
impl GestureDetectorRenderObject {
    /// Create a new pass-through GestureDetector render object.
    ///
    /// Pass-through: delegates layout to its child, generates no paint
    /// commands, hit tests using the child's computed bounds (adopted
    /// via `apply_layout`). Mirrors `DecoratedBoxRenderObject`.
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }
}
```

Edit the `layout` method (lines 493-512). Replace the container-creating logic with pass-through logic that returns the child's node directly (model: `decorated_box.rs:67-91`):

```rust
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Pass-through: return the child's node directly. No intervening
        // container — the grandparent links the grandchild's Taffy node.
        // Mirrors `DecoratedBoxRenderObject::layout`.
        match child_nodes.first() {
            Some(&child_node) => {
                self.layout_node = Some(child_node);
                LayoutResult {
                    node: child_node,
                    size: Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }
```

Edit `apply_layout` (lines 514-520). No change needed — it already reads `self.layout_node` and stores `computed_bounds`. The only difference is that `layout_node` now holds the child's node (adopted) instead of a self-owned container. The existing code is correct as-is:

```rust
    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }
```

Add `is_pass_through` override. Insert immediately after `apply_layout` (after line 520), before `paint`:

```rust
    fn is_pass_through(&self) -> bool {
        true
    }
```

- [ ] **Step 3: Clean up imports**

In `vexo/src/widgets/gesture_detector.rs`, edit line 39. After removing the `layout` field, `Layout` is still needed (used by the pass-through `layout()` defensive branch: `ctx.engine().create_leaf(&Layout::default())`), but `AlignItems` and `FlexDirection` are no longer used:

Before:
```rust
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};
```

After:
```rust
use crate::layout::{Layout, LayoutNodeKey};
```

- [ ] **Step 4: Delete the 4 layout-field tests and add the pass-through test**

In `vexo/src/widgets/gesture_detector.rs`, delete these 4 tests entirely (lines 738-783):

- `test_gesture_detector_with_custom_layout_stores_layout` (738-750)
- `test_gesture_detector_default_layout_is_column_stretch` (752-758)
- `test_gesture_detector_clone_preserves_custom_layout` (760-769)
- `test_gesture_detector_render_object_uses_custom_layout` (771-783)

These all assert on the removed `gd.layout` / `ro.layout` fields or call `new_with_layout`. They cannot be salvaged — the fields are gone.

Add this new test in their place (before the closing `}` of the `mod tests` block):

```rust
    #[test]
    fn test_gesture_detector_render_object_is_pass_through() {
        let widget = GestureDetector::new(Text::new("Hello"));
        let ro = widget.create_render_object();
        assert!(
            ro.is_pass_through(),
            "GestureDetector's render object must be pass-through"
        );
    }
```

- [ ] **Step 5: Migrate `vexo_uikit/src/tab_bar.rs:184`**

In `vexo_uikit/src/tab_bar.rs`, find the `GestureDetector::new(content)` chain at line 184. Move `.with_layout(L)` from the detector onto the content by wrapping `content` in `WithLayout::new(content, L)`.

Before (lines 184-193):
```rust
            let item = GestureDetector::new(content)
                .on_press(move || ctrl.switch_to(tab_clone.clone()))
                .with_layout(
                    Layout::default()
                        .flex_direction(FlexDirection::Column)
                        .align(AlignItems::Stretch)
                        .flex_grow(1.0)
                        .justify(JustifyContent::Center),
                )
                .boxed();
```

After:
```rust
            let item = GestureDetector::new(
                WithLayout::new(
                    content,
                    Layout::default()
                        .flex_direction(FlexDirection::Column)
                        .align(AlignItems::Stretch)
                        .flex_grow(1.0)
                        .justify(JustifyContent::Center),
                )
                .boxed(),
            )
            .on_press(move || ctrl.switch_to(tab_clone.clone()))
            .boxed();
```

Note: `WithLayout` is already imported at line 18 (`use vexo::{..., WithLayout, ...}`). The `.boxed()` on `WithLayout::new(...)` is needed because `GestureDetector::new` takes `impl Widget + 'static`, and the chained `.on_press()` returns `Box<dyn Widget>`.

- [ ] **Step 6: Migrate `shared_app/src/desktop_shell.rs:133`**

In `shared_app/src/desktop_shell.rs`, find the `GestureDetector::new(content)` chain at line 133.

Before (lines 133-143):
```rust
        let item = GestureDetector::new(content)
            .on_press(move || ctrl.switch_to(tab_clone.clone()))
            .with_layout(
                Layout::default()
                    .width_percent(1.0)
                    .height(48.0)
                    .flex_shrink(0.0)
                    .align(AlignItems::Center)
                    .justify(JustifyContent::Center),
            )
            .boxed();
```

After:
```rust
        let item = GestureDetector::new(
            WithLayout::new(
                content,
                Layout::default()
                    .width_percent(1.0)
                    .height(48.0)
                    .flex_shrink(0.0)
                    .align(AlignItems::Center)
                    .justify(JustifyContent::Center),
            )
            .boxed(),
        )
        .on_press(move || ctrl.switch_to(tab_clone.clone()))
        .boxed();
```

Note: `WithLayout` is already imported at line 19.

- [ ] **Step 7: Migrate `shared_app/src/me/profile_screen.rs:376`**

In `shared_app/src/me/profile_screen.rs`, find the `GestureDetector::new(content)` chain at line 376.

Before (lines 376-381):
```rust
    GestureDetector::new(content)
        .on_tap(move || {
            is_dark.set(set_value);
        })
        .with_layout(Layout::default().flex_shrink(0.0))
        .boxed()
```

After:
```rust
    GestureDetector::new(WithLayout::new(content, Layout::default().flex_shrink(0.0)).boxed())
        .on_tap(move || {
            is_dark.set(set_value);
        })
        .boxed()
```

Note: `WithLayout` is already imported (used at lines 159, 168, 205, etc.).

- [ ] **Step 8: Build the workspace**

Run: `cargo build --workspace`
Expected: PASS with no errors. Any missed migration site or leftover `gd.layout` / `new_with_layout` reference produces a compile error here — fix before proceeding.

- [ ] **Step 9: Run the full test suite**

Run: `cargo test --workspace`
Expected: PASS. Key regression guards:
- `test_gesture_detector_render_object_is_pass_through` — new test, must pass.
- `test_tab_bar_items_are_equal_width_full_height_slots` (`vexo_uikit/src/tab_bar.rs:425`) — downcasts to `GestureDetectorRenderObject`, inspects `computed_bounds`. Post-migration, the detector adopts the `WithLayout` child's bounds (equal-width slots). Bounds assertions must still hold.
- `test_appearance_picker_renders_two_tappable_cells` (`shared_app/src/me/profile_screen.rs:507`) — counts `GestureDetectorRenderObject` instances (expects 2). Type unchanged. Must still pass.
- All `with_layout.rs` tests — `WithLayout::new` behavior unchanged.

- [ ] **Step 10: Commit**

```bash
git add vexo/src/widgets/gesture_detector.rs vexo_uikit/src/tab_bar.rs shared_app/src/desktop_shell.rs shared_app/src/me/profile_screen.rs
git commit -m "refactor: convert GestureDetector to pass-through, remove layout field

GestureDetector no longer owns a layout field or Taffy container node.
It is now a true pass-through proxy (consistent with DecoratedBox,
Opacity, Transform) — layout() returns the child's node, apply_layout()
adopts the child's computed bounds, is_pass_through() returns true.

The 3 production call sites that chained .with_layout(L) on the detector
are migrated to wrap content in WithLayout::new(content, L) instead,
preserving hit-testing (the pass-through detector adopts the WithLayout
child's bounds = the slot/row).

This finishes the symmetry left open by the 2026-07-20 ADR: single-child
widgets never own a Layout — use WithLayout to add one."
```

---

### Task 3: Final verification + GUI validation handoff

Verify the full workspace builds and tests pass, then hand off GUI validation to the user (hit-testing is the one behavioral risk — per `CLAUDE.md`, the agent does not run `cargo run -p desktop_demo`).

**Files:**
- None modified (verification only)

- [ ] **Step 1: Clean build from scratch**

Run: `cargo build --workspace 2>&1 | tail -5`
Expected: `Finished` with no warnings about unused imports or dead code.

- [ ] **Step 2: Full test sweep**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: All tests pass, 0 failures. Note any `ignored` count (iOS-gated tests are expected to be ignored on macOS).

- [ ] **Step 3: Verify no lingering `.with_layout` on GestureDetector**

Run: `rg "GestureDetector.*\.with_layout" --type rust`
Expected: No matches. (If any match appears, it's a missed migration site — fix it.)

- [ ] **Step 4: Verify no lingering `new_with_layout` references**

Run: `rg "new_with_layout" --type rust`
Expected: No matches in `vexo/src/widgets/gesture_detector.rs`. (Other files like `mouse_region.rs` may have their own `new_with_layout` — those are out of scope and should be ignored.)

- [ ] **Step 5: GUI validation handoff**

Per `CLAUDE.md`, the agent does not run `cargo run -p desktop_demo`. Hit-testing is the one behavioral risk. Report to the user:

> Implementation complete. Please run `cargo run -p desktop_demo` and verify:
> 1. **Tab bar** — tap anywhere in an equal-width tab slot → tab switches. (Exercises `tab_bar.rs:184` migration.)
> 2. **Desktop sidebar** — tap anywhere in a 48px sidebar row → switches view. (Exercises `desktop_shell.rs:133` migration.)
> 3. **Profile appearance picker** — tap either cell → toggles theme. (Exercises `profile_screen.rs:376` migration.)
>
> If any of these regress, the symptom is "tap only registers on the centered content, not the full slot/row." Per the `debugging-gui-with-logs` workflow, add `log::debug!` in `GestureDetectorRenderObject::hit_test` printing `computed_bounds`, run with `RUST_LOG=debug | grep hit_test`, and inspect the bounds.

- [ ] **Step 6: Mark complete**

No commit needed — this task is verification only. If the user reports a hit-testing regression, diagnose using the `debugging-gui-with-logs` workflow before declaring done.

---

## Spec Coverage Check

| Spec section | Task |
|---|---|
| §"GestureDetector widget" — remove layout field, with_layout, default, Clone line | Task 2 Step 1 |
| §"GestureDetectorRenderObject" — pass-through conversion | Task 2 Step 2 |
| §"WithLayout inherent .with_layout()" | Task 1 |
| §"Call-site migration" — 3 production + 2 test sites | Task 2 Steps 5-7 (production), Step 4 (tests deleted — see reconciliation note) |
| §"Testing" — `test_gesture_detector_render_object_is_pass_through` | Task 2 Step 4 |
| §"Testing" — `test_with_layout_inherent_replace` | Task 1 Step 1 |
| §"Testing" — regression guards (tab_bar, profile_screen downcast tests) | Task 2 Step 9 |
| §"GUI validation" | Task 3 Step 5 |

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Missed `.with_layout` call site on GestureDetector | Very low | Compile error | `rg` audit in Task 3 Step 3; compiler catches in Task 2 Step 8 |
| Hit-testing regression (tap only on centered content, not full slot) | Low | High — broken tab bar/sidebar | GUI validation in Task 3 Step 5; `debugging-gui-with-logs` workflow if symptomatic |
| `MouseRegionRenderObject` has same shape but is NOT converted | Intentional | None | Out of scope per spec §"Non-Goals"; no `.with_layout()` inherent on MouseRegion, so no violation |
| Unused import warnings after removing `AlignItems`/`FlexDirection` | Medium | Build warning | Task 2 Step 3 cleans imports; Task 3 Step 1 verifies zero warnings |
