# NavigationStackView Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `NavigationStackView` component — a stack navigator with a caller-owned `NavigationController<Dest>` handle — to `vexo_uikit`.

**Architecture:** A `Component` that renders a NavBar plus either the caller-supplied root (when the path is empty) or the top-of-stack page built by a `destination` closure (when the path is non-empty). The path lives in `Rc<RefCell<Vec<Dest>>>` on the controller; the dirty callback lives in `Rc<RefCell<Option<Arc<...>>>>`. Lifecycle hooks in `ComponentState` wire/unwire the callback by reading the controller off `ctx.widget()`, exactly mirroring `TextEditingController`.

**Tech Stack:** Rust, vexo framework (`Component`, `ComponentState`, `Flex`, `Button`, `Text`, `Widget`), stdlib `Rc<RefCell<...>>`. Tests use the same `RenderContext` harness as `vexo_uikit/tests/navigation_render_tests.rs`.

**Spec:** `docs/superpowers/specs/2026-07-03-navigationstackview-design.md`

## Global Constraints

- All types live in `vexo_uikit/src/navigation.rs` (alongside `NavigationSplitView`).
- Re-export new public types from `vexo_uikit/src/lib.rs`.
- Controller storage is `Rc<RefCell<Vec<Dest>>>` + `Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>` (NOT `Signal` — see spec §"Why `Rc<RefCell<...>>` and not `Signal`").
- `NavigationStackViewState` has NO controller field; lifecycle hooks read the controller off `ctx.widget()` (matches `TextEditState`).
- Reuse existing `tokens::navigation::MOBILE_*` and `BACK_*` constants — no new theme tokens.
- Identical rendering on all platforms; `.platform()` is a no-op builder (forward-compat only).
- No `shared_app` demo change in this plan.
- No `vexo/` framework changes — built only from existing public primitives.
- Run `cargo build -p vexo_uikit` after edits, `cargo test -p vexo_uikit` after implementing features. Never assume tests pass.
- TDD: write failing test, run to confirm it fails, implement, run to confirm it passes, commit.

---

## File Structure

- **Modify** `vexo_uikit/src/navigation.rs` — add `NavigationController<Dest>`, `NavigationStackView<Dest>`, `NavigationStackViewState<Dest>` after the existing `NavigationSplitView` code.
- **Modify** `vexo_uikit/src/lib.rs` — add `NavigationController`, `NavigationStackView` to the `pub use navigation::{...}` re-export.
- **Create** `vexo_uikit/tests/navigation_stack_tests.rs` — controller unit tests + render tests.

Each task below is self-contained: write test → fail → implement → pass → commit.

---

## Task 1: `NavigationController` skeleton + `new` + `path` + `depth`

**Files:**
- Modify: `vexo_uikit/src/navigation.rs` (append after existing `NavigationSplitView` impl block, before the closing of the file)
- Test: `vexo_uikit/tests/navigation_stack_tests.rs` (new file)

**Interfaces:**
- Produces: `NavigationController<Dest: Hash + Eq + Clone + 'static>` with `new()`, `path() -> Vec<Dest>`, `depth() -> usize`, `Clone`, `Default`.

- [ ] **Step 1: Write the failing test (new file)**

Create `vexo_uikit/tests/navigation_stack_tests.rs`:

```rust
use vexo_uikit::NavigationController;

#[test]
fn controller_default_path_is_empty() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    assert!(controller.path().is_empty(), "new controller path must be empty");
    assert_eq!(controller.depth(), 0, "new controller depth must be 0");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: compile error — `NavigationController` not found / not exported.

- [ ] **Step 3: Add `NavigationController` to `vexo_uikit/src/navigation.rs`**

Append at the end of `vexo_uikit/src/navigation.rs` (after the existing `NavigationSplitView` `impl Component` block):

```rust
// ============================================================================
// NAVIGATION STACK VIEW
// ============================================================================

/// External controller that owns the navigation path for a NavigationStackView.
///
/// Inspired by SwiftUI's `NavigationPath` + Flutter's `TextEditingController`:
/// the caller creates and owns this controller, passing it into
/// NavigationStackView. The controller holds the LIFO stack of pushed
/// destinations; mutating methods (`push`, `pop`, etc.) fire a dirty callback
/// wired by the framework during mount, triggering a rebuild.
///
/// The path and dirty callback are shared via `Rc<RefCell<...>>` so that
/// clones captured in closures *before* wiring still observe mutations and
/// fire the callback once wired. This mirrors `TextEditingController`.
pub struct NavigationController<Dest: Hash + Eq + Clone + 'static> {
    path: Rc<RefCell<Vec<Dest>>>,
    dirty_callback: Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationController<Dest> {
    /// Create a new controller with an empty path (at root).
    pub fn new() -> Self {
        Self {
            path: Rc::new(RefCell::new(Vec::new())),
            dirty_callback: Rc::new(RefCell::new(None)),
        }
    }

    /// Snapshot the current path for inspection.
    pub fn path(&self) -> Vec<Dest> {
        self.path.borrow().clone()
    }

    /// Current stack depth (path length). `0` means at root.
    pub fn depth(&self) -> usize {
        self.path.borrow().len()
    }
}

impl<Dest: Hash + Eq + Clone + 'static> Clone for NavigationController<Dest> {
    fn clone(&self) -> Self {
        Self {
            path: Rc::clone(&self.path),
            dirty_callback: Rc::clone(&self.dirty_callback),
        }
    }
}

impl<Dest: Hash + Eq + Clone + 'static> Default for NavigationController<Dest> {
    fn default() -> Self {
        Self::new()
    }
}
```

Ensure these imports already exist at the top of the file (they do for `NavigationSplitView`):
`use std::cell::RefCell;`, `use std::hash::Hash;`, `use std::rc::Rc;`, `use std::sync::Arc;`.
If any are missing, add them.

- [ ] **Step 4: Add `NavigationController` to the re-export in `vexo_uikit/src/lib.rs`**

Modify the `pub use navigation::{...};` line in `vexo_uikit/src/lib.rs` to add `NavigationController`:

```rust
pub use navigation::{
    NavigationController, NavigationItem, NavigationSplitView, NavigationSplitViewState,
};
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (1 test).

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/navigation.rs vexo_uikit/src/lib.rs vexo_uikit/tests/navigation_stack_tests.rs
git commit -m "feat(navigation): add NavigationController skeleton with path/depth"
```

---

## Task 2: `NavigationController` mutators — `push`, `pop`, `pop_to_root`, `replace`, plus wiring helpers

**Files:**
- Modify: `vexo_uikit/src/navigation.rs` (extend the `impl NavigationController<Dest>` block)
- Test: `vexo_uikit/tests/navigation_stack_tests.rs` (append tests)

**Interfaces:**
- Consumes: `NavigationController` from Task 1.
- Produces: `push(&self, Dest)`, `pop(&self) -> Option<Dest>`, `pop_to_root(&self)`, `replace(&self, Dest)`, `set_dirty_callback(&self, Arc<dyn Fn() + Send + Sync>)`, `clear_dirty_callback(&self)`, private `notify(&self)`.

- [ ] **Step 1: Write the failing tests (append to `vexo_uikit/tests/navigation_stack_tests.rs`)**

Append:

```rust
#[test]
fn controller_push_pop_round_trip() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    assert_eq!(controller.path(), vec!["a"]);
    assert_eq!(controller.depth(), 1);
    controller.push("b");
    assert_eq!(controller.path(), vec!["a", "b"]);
    assert_eq!(controller.depth(), 2);

    assert_eq!(controller.pop(), Some("b"));
    assert_eq!(controller.path(), vec!["a"]);
    assert_eq!(controller.pop(), Some("a"));
    assert_eq!(controller.path(), Vec::<&str>::new());
    assert_eq!(controller.depth(), 0);
}

#[test]
fn controller_pop_to_root_clears_path() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.push("b");
    controller.push("c");
    controller.pop_to_root();
    assert!(controller.path().is_empty());
    assert_eq!(controller.depth(), 0);
}

#[test]
fn controller_replace_swaps_top() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.replace("b");
    assert_eq!(controller.path(), vec!["b"]);
    assert_eq!(controller.depth(), 1);
}

#[test]
fn controller_replace_at_root_behaves_as_push() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.replace("only");
    assert_eq!(controller.path(), vec!["only"]);
    assert_eq!(controller.depth(), 1);
}

#[test]
fn controller_pop_at_root_is_noop() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    assert_eq!(controller.pop(), None);
    assert!(controller.path().is_empty());
    assert_eq!(controller.depth(), 0);
}

#[test]
fn controller_depth_tracks_path_length() {
    let controller: NavigationController<u32> = NavigationController::new();
    assert_eq!(controller.depth(), 0);
    controller.push(1);
    assert_eq!(controller.depth(), 1);
    controller.push(2);
    assert_eq!(controller.depth(), 2);
    controller.pop();
    assert_eq!(controller.depth(), 1);
    controller.pop_to_root();
    assert_eq!(controller.depth(), 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: compile error — `push`, `pop`, etc. not found on `NavigationController`.

- [ ] **Step 3: Implement the mutators and wiring helpers**

In `vexo_uikit/src/navigation.rs`, extend the `impl<Dest: Hash + Eq + Clone + 'static> NavigationController<Dest>` block (add these methods; keep `new`/`path`/`depth` from Task 1):

```rust
    /// Push a new destination onto the stack. The next render shows its page.
    pub fn push(&self, dest: Dest) {
        self.path.borrow_mut().push(dest);
        self.notify();
    }

    /// Pop the top destination. No-op at root (returns `None`).
    /// Returns the popped value when the path was non-empty.
    pub fn pop(&self) -> Option<Dest> {
        let popped = self.path.borrow_mut().pop();
        if popped.is_some() {
            self.notify();
        }
        popped
    }

    /// Clear the entire path, returning to root. Idempotent: a no-op (and no
    /// dirty fire) if the path is already empty.
    pub fn pop_to_root(&self) {
        let mut p = self.path.borrow_mut();
        if p.is_empty() {
            return;
        }
        p.clear();
        drop(p);
        self.notify();
    }

    /// Replace the top of the stack with `dest`. At root (empty path), behaves
    /// as `push(dest)` — documented, not an error.
    pub fn replace(&self, dest: Dest) {
        let mut p = self.path.borrow_mut();
        if let Some(top) = p.last_mut() {
            *top = dest;
        } else {
            p.push(dest);
        }
        drop(p);
        self.notify();
    }

    // --- Framework wiring (called by NavigationStackViewState lifecycle) ---

    /// Wire the dirty callback. Called from `ComponentState::on_mount` (and
    /// `on_update` when the widget's controller instance changes), reading the
    /// controller off `ctx.widget()`. Takes `&self` because the callback cell
    /// is a `RefCell`.
    pub fn set_dirty_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.dirty_callback.borrow_mut() = Some(callback);
    }

    /// Clear the dirty callback. Called from `ComponentState::on_unmount`.
    pub fn clear_dirty_callback(&self) {
        *self.dirty_callback.borrow_mut() = None;
    }

    /// Fire the dirty callback if set. Called by `push`/`pop`/etc. after
    /// mutating the path.
    fn notify(&self) {
        if let Some(cb) = self.dirty_callback.borrow().as_ref() {
            cb();
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/navigation.rs vexo_uikit/tests/navigation_stack_tests.rs
git commit -m "feat(navigation): add NavigationController push/pop/replace + wiring helpers"
```

---

## Task 3: `NavigationController` dirty-callback wiring test

**Files:**
- Test: `vexo_uikit/tests/navigation_stack_tests.rs` (append)

**Interfaces:**
- Consumes: `set_dirty_callback`, `clear_dirty_callback`, `push` from Task 2.
- Produces: confidence that the dirty callback fires on mutation and is silenced after `clear_dirty_callback`.

- [ ] **Step 1: Write the failing test (append)**

Append:

```rust
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

#[test]
fn controller_notify_fires_dirty_callback_on_push() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.push("a");
    controller.push("b");
    assert_eq!(counter.load(Ordering::SeqCst), 2, "push must fire dirty callback");
}

#[test]
fn controller_notify_fires_dirty_callback_on_pop_only_when_nonempty() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.push("a");
    controller.pop();
    assert_eq!(counter.load(Ordering::SeqCst), 2);
    controller.pop(); // at root — no fire
    assert_eq!(counter.load(Ordering::SeqCst), 2, "pop at root must NOT fire");
}

#[test]
fn controller_pop_to_root_does_not_fire_when_already_at_root() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.pop_to_root();
    assert_eq!(counter.load(Ordering::SeqCst), 0, "pop_to_root at root must NOT fire");
}

#[test]
fn controller_clear_dirty_callback_silences_notify() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.clear_dirty_callback();
    controller.push("a");
    assert_eq!(counter.load(Ordering::SeqCst), 0, "after clear, push must NOT fire");
}

#[test]
fn controller_clone_shares_path_and_callback() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    let clone = controller.clone();
    clone.push("a"); // mutate via clone
    assert_eq!(controller.path(), vec!["a"], "clone must share path storage");
    assert_eq!(counter.load(Ordering::SeqCst), 1, "clone must fire shared callback");
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (12 tests). (These exercise already-implemented methods, so they should pass immediately; if any fail, the implementation in Task 2 is wrong — fix before continuing.)

- [ ] **Step 3: Commit**

```bash
git add vexo_uikit/tests/navigation_stack_tests.rs
git commit -m "test(navigation): cover NavigationController dirty-callback wiring"
```

---

## Task 4: `NavigationStackView` struct + builder API (no `render` yet)

**Files:**
- Modify: `vexo_uikit/src/navigation.rs` (append after the `NavigationController` block)
- Modify: `vexo_uikit/src/lib.rs` (re-export)

**Interfaces:**
- Consumes: `NavigationController` from Task 1-2.
- Produces: `NavigationStackView<Dest>` struct with `new`, `destination`, `title`, `root_title`, `platform`, `Clone`. No `Component` impl yet.

- [ ] **Step 1: Write the failing test (append to `vexo_uikit/tests/navigation_stack_tests.rs`)**

Append (note: this test only checks construction — no render yet):

```rust
use vexo::{Text, Widget};
use vexo_uikit::NavigationStackView;

#[test]
fn stack_view_can_be_constructed_with_builder_methods() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root"))
        .root_title("Home")
        .title(|d| format!("{}", d))
        .destination(|d| Text::new(format!("Page: {}", d)).boxed())
        .platform(vexo_uikit::Platform::Mobile);
    // No assertion on render yet — just that construction compiles and does not panic.
    let _ = view;
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: compile error — `NavigationStackView` not found.

- [ ] **Step 3: Add the `NavigationStackView` struct + builder methods to `vexo_uikit/src/navigation.rs`**

Append after the `NavigationController` impl block. Ensure `use vexo::Widget;` is in the existing import block at the top of the file (it already is, via `NavigationSplitView`'s imports — verify).

```rust
/// A stack navigation component: a root page plus a LIFO stack of pushed pages.
///
/// Modeled on SwiftUI's `NavigationStack`. The caller owns a
/// `NavigationController<Dest>` and mutates the path via `push`/`pop`/etc.;
/// the controller's dirty callback (wired during mount) triggers rebuilds so
/// the view always reflects the current top-of-stack.
///
/// The component renders a NavBar (title + optional back button) above either
/// the root widget (empty path) or the destination closure's output (non-empty
/// path). No `ScrollView`, padding, or background is applied to the page —
/// callers wrap their page content as desired.
pub struct NavigationStackView<Dest: Hash + Eq + Clone + 'static> {
    controller: NavigationController<Dest>,
    root: Box<dyn Widget>,
    destination: Rc<dyn Fn(&Dest) -> Box<dyn Widget>>,
    title: Rc<dyn Fn(&Dest) -> String>,
    root_title: Option<String>,
    platform: Option<Platform>,
}

impl<Dest: Hash + Eq + Clone + 'static> Clone for NavigationStackView<Dest> {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
            root: self.root.clone_boxed(),
            destination: self.destination.clone(),
            title: self.title.clone(),
            root_title: self.root_title.clone(),
            platform: self.platform,
        }
    }
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationStackView<Dest> {
    /// Create a stack view with the given controller and root page widget.
    pub fn new(controller: NavigationController<Dest>, root: impl Widget + 'static) -> Self {
        Self {
            controller,
            root: Box::new(root),
            destination: Rc::new(|_| Text::new("").boxed()),
            title: Rc::new(|_| String::new()),
            root_title: None,
            platform: None,
        }
    }

    /// Provide a closure that builds the page widget for a pushed destination.
    /// Called at most once per rebuild, with `path.last()`.
    pub fn destination<F: Fn(&Dest) -> Box<dyn Widget> + 'static>(mut self, f: F) -> Self {
        self.destination = Rc::new(f);
        self
    }

    /// Provide a closure returning the NavBar title for a pushed destination.
    /// Default: returns an empty string.
    pub fn title<F: Fn(&Dest) -> String + 'static>(mut self, f: F) -> Self {
        self.title = Rc::new(f);
        self
    }

    /// Set the NavBar title shown when at root. Default: `None` (empty title).
    pub fn root_title(mut self, title: impl Into<String>) -> Self {
        self.root_title = Some(title.into());
        self
    }

    /// Override the platform. Currently a no-op (rendering is identical on all
    /// platforms); reserved for future desktop adaptation.
    pub fn platform(mut self, p: Platform) -> Self {
        self.platform = Some(p);
        self
    }
}
```

- [ ] **Step 4: Add `NavigationStackView` to the re-export in `vexo_uikit/src/lib.rs`**

Modify the `pub use navigation::{...};` line:

```rust
pub use navigation::{
    NavigationController, NavigationItem, NavigationSplitView, NavigationSplitViewState,
    NavigationStackView,
};
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (13 tests).

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/navigation.rs vexo_uikit/src/lib.rs vexo_uikit/tests/navigation_stack_tests.rs
git commit -m "feat(navigation): add NavigationStackView struct + builder API"
```

---

## Task 5: `NavigationStackViewState` + lifecycle wiring (no `render` yet)

**Files:**
- Modify: `vexo_uikit/src/navigation.rs` (append after the `NavigationStackView` builder impl)

**Interfaces:**
- Consumes: `NavigationController::{set_dirty_callback, clear_dirty_callback}` from Task 2; `NavigationStackView` from Task 4.
- Produces: `NavigationStackViewState<Dest>` implementing `ComponentState` with `on_mount`/`on_update`/`on_unmount`. No `Component` impl on `NavigationStackView` yet.

- [ ] **Step 1: Write the failing test (append)**

This test verifies the state's `on_mount` wires the controller's dirty callback. It needs a `LifecycleContext` — which is non-trivial to build standalone. Instead, test wiring indirectly via a render-context harness in Task 6. For this task, write a compile-only test that confirms the state type exists and `Default` works:

```rust
#[test]
fn stack_view_state_default_compiles() {
    fn assert_default<T: Default>() {}
    assert_default::<vexo_uikit::navigation::NavigationStackViewState<&'static str>>();
}
```

Note: `NavigationStackViewState` must be reachable. We'll expose it via `pub` in the module but NOT re-export from `lib.rs` (it's an internal state type, like `NavigationSplitViewState` is exported but consumers rarely need it — match that precedent by also re-exporting it; see Step 3).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: compile error — `NavigationStackViewState` not found.

- [ ] **Step 3: Add `NavigationStackViewState` to `vexo_uikit/src/navigation.rs`**

Append after the `NavigationStackView` builder impl. The imports `use std::any::Any;`, `use std::marker::PhantomData;` may need adding — check the top of the file. `Arc` is already imported.

```rust
use std::marker::PhantomData;

/// State for the NavigationStackView component.
///
/// Has NO controller field. The controller lives on the widget (caller-supplied).
/// Lifecycle hooks read it off `ctx.widget()` and wire/unwire its dirty callback
/// — exactly like `TextEditState`. The state exists only to host the lifecycle
/// hooks; `set_dirty_callback` is a no-op (no state-owned Signals).
pub struct NavigationStackViewState<Dest: Hash + Eq + Clone + 'static> {
    _marker: PhantomData<Dest>,
}

impl<Dest: Hash + Eq + Clone + 'static> Default for NavigationStackViewState<Dest> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<Dest: Hash + Eq + Clone + 'static> ComponentState for NavigationStackViewState<Dest> {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
            nav.controller.set_dirty_callback(ctx.dirty_callback());
        }
    }

    fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut LifecycleContext) {
        let old = old_widget.downcast_ref::<NavigationStackView<Dest>>();
        let new = ctx.widget().downcast_ref::<NavigationStackView<Dest>>();
        if let (Some(old), Some(new)) = (old, new) {
            // Re-wire only if the controller instance changed. Identity is
            // determined by Rc::ptr_eq on the shared path cell.
            if !Rc::ptr_eq(&old.controller.path, &new.controller.path) {
                old.controller.clear_dirty_callback();
                new.controller.set_dirty_callback(ctx.dirty_callback());
            }
        }
    }

    fn on_unmount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
            nav.controller.clear_dirty_callback();
        }
    }
}
```

Add the necessary imports at the top of `vexo_uikit/src/navigation.rs` if not already present. The existing `NavigationSplitView` imports already bring in `Component`, `ComponentState`, `RenderContext`, `Widget` from `vexo`. Add `LifecycleContext` to that import:

```rust
use vexo::{
    AlignItems, Component, ComponentState, DecoratedContainer, LifecycleContext, RenderContext,
    ScrollView, Signal, Text, Widget,
};
```

(The existing import line in `navigation.rs` already has most of these; just add `LifecycleContext`.)

- [ ] **Step 4: Add `NavigationStackViewState` to the re-export in `vexo_uikit/src/lib.rs`**

```rust
pub use navigation::{
    NavigationController, NavigationItem, NavigationSplitView, NavigationSplitViewState,
    NavigationStackView, NavigationStackViewState,
};
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (14 tests). Also run `cargo build -p vexo_uikit` to confirm `ComponentState` impl compiles.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/navigation.rs vexo_uikit/src/lib.rs vexo_uikit/tests/navigation_stack_tests.rs
git commit -m "feat(navigation): add NavigationStackViewState with controller lifecycle wiring"
```

---

## Task 6: `Component::render` for `NavigationStackView` — root case

**Files:**
- Modify: `vexo_uikit/src/navigation.rs` (add `impl Component` block)

**Interfaces:**
- Consumes: `NavigationStackView` builder (Task 4), `NavigationStackViewState` (Task 5), `tokens::navigation::MOBILE_*` and `BACK_*`, `Button`/`ButtonVariant`, `Flex`, `Text`.
- Produces: a working `render` that draws the NavBar + root when path is empty.

- [ ] **Step 1: Write the failing tests (append)**

These need the render harness. Append (the harness mirrors `navigation_render_tests.rs`):

```rust
use vexo::{
    BuildOwner, DirtyTracking, ElementKey, Flex, RenderContext, RenderObjectRegistry,
};

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn create_render_context<'a>(
    element_id: ElementKey,
    dirty: &'a mut DirtyTracking,
    render_objects: &'a mut RenderObjectRegistry,
    build_owner: &'a BuildOwner,
) -> RenderContext<'a> {
    RenderContext {
        element_id,
        dirty,
        render_objects,
        build_owner,
    }
}

fn render_stack<Dest: std::hash::Hash + Eq + Clone + 'static>(
    view: NavigationStackView<Dest>,
    state: &mut vexo_uikit::NavigationStackViewState<Dest>,
) -> Box<dyn Widget> {
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();
    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    view.render(state, &mut ctx)
}

fn collect_text(w: &dyn Widget, out: &mut Vec<String>) {
    if let Some(t) = w.as_any().downcast_ref::<Text>() {
        out.push(t.content().to_string());
    }
    if let Some(child) = w.child() {
        collect_text(child, out);
    }
    for child in w.children() {
        collect_text(child.as_ref(), out);
    }
}

fn all_text(w: &dyn Widget) -> Vec<String> {
    let mut out = Vec::new();
    collect_text(w, &mut out);
    out
}

#[test]
fn stack_render_root_does_not_panic() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page"))
        .root_title("Home");
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let _tree = render_stack(view, &mut state);
}

#[test]
fn stack_root_top_level_is_flex_column_with_two_children() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page"))
        .root_title("Home");
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    let flex = tree
        .as_any()
        .downcast_ref::<Flex>()
        .expect("top-level widget should be a Flex");
    assert_eq!(
        flex.children().len(),
        2,
        "root layout must have NavBar + root = 2 children"
    );
}

#[test]
fn stack_root_has_no_back_button() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page"))
        .root_title("Home");
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    let texts = all_text(tree.as_ref());
    assert!(
        !texts.iter().any(|t| t.contains("Back")),
        "root must NOT show a back button, got: {:?}",
        texts
    );
}

#[test]
fn stack_navbar_title_uses_root_title_at_root() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page"))
        .root_title("Home");
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    let texts = all_text(tree.as_ref());
    assert!(
        texts.iter().any(|t| t == "Home"),
        "root NavBar must show root_title 'Home', got: {:?}",
        texts
    );
}

#[test]
fn stack_navbar_title_is_empty_when_root_title_unset() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page"));
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    // Should not panic and should still have 2 children (NavBar with empty title + root).
    let flex = tree
        .as_any()
        .downcast_ref::<Flex>()
        .expect("top-level should be Flex");
    assert_eq!(flex.children().len(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: compile error — `Component` not implemented for `NavigationStackView`, so `view.render(...)` doesn't exist.

- [ ] **Step 3: Implement `Component::render` (root case) + `build_nav_bar` helper**

Append to `vexo_uikit/src/navigation.rs` (after the `NavigationStackViewState` impl). The `tokens::navigation` module already provides `MOBILE_HEADER_BG`, `MOBILE_HEADER_HEIGHT`, `MOBILE_HEADER_PADDING`, `MOBILE_TITLE_FONT_SIZE`, `MOBILE_TITLE_COLOR`, `BACK_CHEVRON`, `BACK_LABEL`.

```rust
impl<Dest: Hash + Eq + Clone + 'static> Component for NavigationStackView<Dest> {
    type State = NavigationStackViewState<Dest>;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let path = self.controller.path();
        let (title, can_pop) = if let Some(top) = path.last() {
            ((self.title)(top), true)
        } else {
            (
                self.root_title.clone().unwrap_or_default(),
                false,
            )
        };

        let nav_bar = self.build_nav_bar(&title, can_pop);

        let page: Box<dyn Widget> = if let Some(top) = path.last() {
            (self.destination)(top)
        } else {
            self.root.clone_boxed()
        };

        Flex::column()
            .push(nav_bar)
            .push(page)
            .boxed()
    }
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationStackView<Dest> {
    /// Build the NavBar chrome: title text + optional back button.
    ///
    /// `can_pop == false` (at root) → no back button, title occupies the row.
    /// `can_pop == true` → back button on the left, title after it.
    fn build_nav_bar(&self, title: &str, can_pop: bool) -> Box<dyn Widget> {
        let mut row = Flex::row()
            .align(AlignItems::Center)
            .gap(8.0)
            .padding(tokens::navigation::MOBILE_HEADER_PADDING)
            .background(tokens::navigation::MOBILE_HEADER_BG)
            .height(tokens::navigation::MOBILE_HEADER_HEIGHT)
            .flex_shrink(0.0);

        if can_pop {
            let controller = self.controller.clone();
            let back_label = format!(
                "{} {}",
                tokens::navigation::BACK_CHEVRON,
                tokens::navigation::BACK_LABEL
            );
            let back_button = Button::new(back_label)
                .variant(ButtonVariant::Ghost)
                .on_press(move || {
                    controller.pop();
                })
                .boxed();
            row = row.push(back_button);
        }

        let title_text = Text::new(title)
            .with_font_size(tokens::navigation::MOBILE_TITLE_FONT_SIZE)
            .with_color(tokens::navigation::MOBILE_TITLE_COLOR);
        row = row.push(title_text);

        row.boxed()
    }
}
```

Note: `state` is unused in `render` itself (the controller is read off `self`), but the parameter must exist to satisfy the `Component` trait. Prefix with `_state` to silence the unused warning — actually, the trait signature names it `state`, so just leave the signature as `state: &mut Self::State` and the lint will accept it (it's a trait method). If a warning appears, rename to `_state` — but trait methods can use any parameter name, so `state` is fine.

Wait — the trait signature is `fn render(&self, state: &mut Self::State, ctx: &mut RenderContext)`. The parameter name in the impl can differ. Use `state` and `_ctx` (ctx is genuinely unused here since the controller is on `self`). The existing `NavigationSplitView::render` uses `_ctx` for the same reason — match that.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (19 tests).

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/navigation.rs vexo_uikit/tests/navigation_stack_tests.rs
git commit -m "feat(navigation): implement NavigationStackView render (root case + NavBar)"
```

---

## Task 7: `Component::render` pushed-page case + destination-builder tests

**Files:**
- Modify: `vexo_uikit/src/navigation.rs` (the `render` impl from Task 6 already handles both cases — this task adds tests for the pushed case)

**Interfaces:**
- Consumes: `render` from Task 6 (already handles non-empty path).

- [ ] **Step 1: Write the failing tests (append)**

```rust
#[test]
fn stack_render_pushed_page_does_not_panic() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");
    let view = NavigationStackView::new(controller, Text::new("Root page"))
        .root_title("Home")
        .title(|d| format!("Title-{}", d))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let _tree = render_stack(view, &mut state);
}

#[test]
fn stack_pushed_page_has_back_button() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");
    let view = NavigationStackView::new(controller, Text::new("Root page"))
        .root_title("Home")
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    let texts = all_text(tree.as_ref());
    assert!(
        texts.iter().any(|t| t.contains(tokens::navigation::BACK_LABEL)),
        "pushed page NavBar must contain back label '{}', got: {:?}",
        tokens::navigation::BACK_LABEL,
        texts
    );
}
```

Note: `tokens::navigation::BACK_LABEL` is `pub const`, so this is reachable. If the test file cannot import `vexo_uikit::theme::tokens::navigation`, fall back to the literal string `"Back"` (the value of `BACK_LABEL`). Prefer the constant import; add `use vexo_uikit::theme::tokens;` at the top of the test file if not present.

- [ ] **Step 2: Run tests — they should already pass**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (21 tests). The `render` impl from Task 6 already handles the non-empty path. If these fail, the Task 6 impl is wrong — fix it.

- [ ] **Step 3: Add destination-builder invocation-count tests (append)**

```rust
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

fn make_counted_destination<T: std::fmt::Display>() -> (Arc<AtomicU32>, impl Fn(&T) -> Box<dyn Widget>)
{
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    let closure = move |d: &T| {
        c.fetch_add(1, Ordering::SeqCst);
        Text::new(format!("Body: {}", d)).boxed()
    };
    (counter, closure)
}

#[test]
fn stack_destination_not_invoked_at_root() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let (counter, dest) = make_counted_destination::<&'static str>();
    let view = NavigationStackView::new(controller, Text::new("Root"))
        .destination(dest);
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let _tree = render_stack(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "destination builder must NOT run at root"
    );
}

#[test]
fn stack_destination_invoked_once_per_render_when_pushed() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    let (counter, dest) = make_counted_destination::<&'static str>();
    let view = NavigationStackView::new(controller, Text::new("Root"))
        .destination(dest);
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let _tree = render_stack(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "destination builder must run exactly once when pushed"
    );
}

#[test]
fn stack_navbar_title_uses_destination_title_when_pushed() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");
    let view = NavigationStackView::new(controller, Text::new("Root"))
        .root_title("Home")
        .title(|d| format!("Title-{}", d))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    let texts = all_text(tree.as_ref());
    assert!(
        texts.iter().any(|t| t == "Title-detail"),
        "pushed NavBar must use destination title 'Title-detail', got: {:?}",
        texts
    );
    assert!(
        !texts.iter().any(|t| t == "Home"),
        "pushed NavBar must NOT show root_title 'Home', got: {:?}",
        texts
    );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (24 tests).

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/tests/navigation_stack_tests.rs
git commit -m "test(navigation): cover NavigationStackView pushed-page rendering"
```

---

## Task 8: Controller-driven re-render round-trip test

**Files:**
- Test: `vexo_uikit/tests/navigation_stack_tests.rs` (append)

**Interfaces:**
- Consumes: full `NavigationStackView` + `NavigationController` from Tasks 1-7.

- [ ] **Step 1: Write the test (append)**

This test simulates the rebuild loop manually: render at root, push via controller, render again, assert the pushed page is now visible. (A real `BuildOwner` integration test would require mounting the element tree — out of scope; the `RenderContext` harness re-renders on demand.)

```rust
#[test]
fn stack_pop_via_controller_round_trip_shows_root_again() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");

    let view = NavigationStackView::new(controller.clone(), Text::new("Root page"))
        .root_title("Home")
        .title(|d| format!("Title-{}", d))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();

    // First render: pushed page visible.
    let tree = render_stack(view.clone(), &mut state);
    let texts = all_text(tree.as_ref());
    assert!(
        texts.iter().any(|t| t == "Body-detail"),
        "first render must show pushed page body, got: {:?}",
        texts
    );

    // Simulate the rebuild triggered by the controller's dirty callback:
    // pop via the controller, then re-render.
    controller.pop();

    let tree2 = render_stack(view, &mut state);
    let texts2 = all_text(tree2.as_ref());
    assert!(
        texts2.iter().any(|t| t == "Root page"),
        "after pop, root must be visible again, got: {:?}",
        texts2
    );
    assert!(
        !texts2.iter().any(|t| t == "Body-detail"),
        "after pop, pushed body must NOT be visible, got: {:?}",
        texts2
    );
}

#[test]
fn stack_push_then_pop_to_root_round_trip() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.push("b");
    controller.push("c");

    let view = NavigationStackView::new(controller.clone(), Text::new("Root"))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();

    // Top is "c".
    let tree = render_stack(view.clone(), &mut state);
    let texts = all_text(tree.as_ref());
    assert!(texts.iter().any(|t| t == "Body-c"));

    controller.pop_to_root();
    let tree2 = render_stack(view, &mut state);
    let texts2 = all_text(tree2.as_ref());
    assert!(
        texts2.iter().any(|t| t == "Root"),
        "after pop_to_root, root must be visible, got: {:?}",
        texts2
    );
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (26 tests).

- [ ] **Step 3: Commit**

```bash
git add vexo_uikit/tests/navigation_stack_tests.rs
git commit -m "test(navigation): cover controller-driven push/pop re-render round trips"
```

---

## Task 9: Final verification — full crate build + test + clippy

**Files:**
- None modified.

- [ ] **Step 1: Build the whole workspace**

Run: `cargo build`
Expected: compiles cleanly with no warnings.

- [ ] **Step 2: Run all vexo_uikit tests**

Run: `cargo test -p vexo_uikit`
Expected: all tests pass (existing `navigation_render_tests.rs` + new `navigation_stack_tests.rs`).

- [ ] **Step 3: Run clippy on vexo_uikit**

Run: `cargo clippy -p vexo_uikit -- -D warnings`
Expected: no warnings. If warnings appear, fix them (common ones: unused imports, needless clone).

- [ ] **Step 4: Verify the existing NavigationSplitView demo still works**

Run: `cargo build -p shared_app`
Expected: compiles. (Do NOT run the GUI — per CLAUDE.md, never run `cargo run -p desktop_demo` yourself; the build check is sufficient to confirm no regression.)

- [ ] **Step 5: Final commit if any fixes were made**

If Steps 1-3 required any code changes, commit them:

```bash
git add -A
git commit -m "chore(navigation): fix clippy/build warnings from NavigationStackView"
```

If no changes were needed, no commit — the feature is complete.

---

## Self-Review Notes

**Spec coverage check:**
- ✅ `NavigationController` with `push`/`pop`/`pop_to_root`/`replace`/`path`/`depth`/`set_dirty_callback`/`clear_dirty_callback`/`notify`/`Clone`/`Default` — Tasks 1-2.
- ✅ `NavigationStackView` with `new`/`destination`/`title`/`root_title`/`platform`/`Clone` — Task 4.
- ✅ `NavigationStackViewState` with `Default` + `ComponentState` (`on_mount`/`on_update`/`on_unmount`) — Task 5.
- ✅ `Component::render` — root case + pushed case — Tasks 6-7.
- ✅ NavBar with back button + title, reusing `tokens::navigation::MOBILE_*`/`BACK_*` — Task 6.
- ✅ Edge cases: pop at root, replace at root, pop_to_root at root, empty title — Tasks 2-3, 6.
- ✅ All 14 tests listed in spec §Testing — Tasks 1, 2, 3, 6, 7, 8 (controller unit tests + render tests + round-trip).
- ✅ Re-export from `vexo_uikit/src/lib.rs` — Tasks 1, 4, 5.
- ✅ No `shared_app` demo change, no new theme tokens, no `vexo/` framework changes — respected throughout.

**Placeholder scan:** None. Every step has complete code.

**Type consistency:** `NavigationController<Dest>` signature is identical across all tasks. `NavigationStackViewState<Dest>` is consistent. `render_stack` helper uses the same generic bound (`Hash + Eq + Clone + 'static`) everywhere. `path() -> Vec<Dest>` return type is consistent.

**Scope check:** Single component, single crate, focused. No decomposition needed.
