# NavigationStackView Design Spec

## Problem

Vexo's `NavigationSplitView` provides a two-column (sidebar + detail) navigation layout, but
there is no general-purpose stack navigator for drill-down flows: a root screen from which the
user pushes deeper levels and pops back. SwiftUI's `NavigationStack` fills this role; Vexo needs
an equivalent.

## Scope

### In scope

- A standalone `NavigationStackView` `Component` that renders a root page plus a LIFO stack of
  pushed pages.
- A caller-owned `NavigationController<Dest>` handle exposing `push`, `pop`, `pop_to_root`,
  `replace`, `path`, `depth`.
- A NavBar chrome (title + optional back button) rendered above the current page.
- Identical rendering on all platforms (single code path).

### Explicit non-goals

- State preservation of off-screen (popped) pages. Pushed pages are rebuilt from scratch on each
  push; callers hoist persistent state into the controller or app state.
- Keep-alive / caching of stacked layers. Only the top-of-stack page is built each frame.
- Replacing or refactoring `NavigationSplitView`. That component keeps its current behavior,
  including its mobile 2-level boolean toggle.
- Platform-specific adaptation. `.platform()` is accepted but currently a no-op, reserved for a
  future desktop breadcrumb treatment.
- `NavigationLink`-style implicit-push widgets. Pushing is explicit via the controller handle.
- A `shared_app` demo swap. The existing `NavigationSplitView` demo is left untouched; a
  NavigationStackView demo can be a follow-up.

## Approach

A `NavigationController<Dest>` holds the path in `Rc<RefCell<Vec<Dest>>>` and a dirty callback
in `Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>`. The controller is created by the caller,
passed to `NavigationStackView::new`, and cloned into page-builder closures so any push/pop
mutates the shared path and fires the shared dirty callback. The framework wires the dirty
callback during the element's `on_mount` lifecycle hook (and re-wires on `on_update` if the
widget's controller instance changes, unwires on `on_unmount`), reading the controller off
`ctx.widget()`. When the callback fires it marks the element dirty in the `BuildOwner`,
triggering a rebuild with the new path.

This mirrors the existing `TextEditingController` precedent exactly: a caller-owned, cheaply-
clonable handle whose internal state is shared via `Rc<RefCell<...>>` so that clones taken
before wiring still observe and fire the callback once wired.

### Why `Rc<RefCell<...>>` and not `Signal<Vec<Dest>>`

The framework's `Signal::set_dirty_callback` takes `&mut self`, and `Signal::clone` copies the
`on_change: Option<Arc<...>>` field at clone time. The caller creates the controller, captures
clones in page-builder closures, *then* builds the widget — so wiring happens *after* those
clones are captured, and the captured clones would never receive the dirty callback. Storing
the callback in a shared `Rc<RefCell<Option<Arc<...>>>>` (as `TextEditingController` does)
sidesteps this: all clones share the same callback cell, which `on_mount` populates through any
one of them.

### Why a controller handle (not a raw shared `Vec`)

- `push` / `pop` / `pop_to_root` / `replace` are ergonomic named operations instead of manual
  `borrow_mut` → mutate → `notify`.
- Matches the `TextEditingController` pattern already in the codebase.
- Still fully externally controllable: callers can call any controller method from anywhere.

## Public API

All types live in `vexo_uikit/src/navigation.rs` alongside `NavigationSplitView`, and are
re-exported from `vexo_uikit/src/lib.rs`.

### `NavigationController<Dest>`

```rust
pub struct NavigationController<Dest: Hash + Eq + Clone + 'static> {
    // Shared path storage. Rc<RefCell<...>> so all clones observe and mutate
    // the same stack (matches TextEditingController's `editor` field).
    path: Rc<RefCell<Vec<Dest>>>,
    // Shared dirty-callback cell. Rc<RefCell<Option<...>>> so clones captured
    // before on_mount-wiring still fire the callback once wired (matches
    // TextEditingController's `dirty_callback` field). Settable through &self.
    dirty_callback: Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationController<Dest> {
    /// Creates a controller with an empty path (at root).
    pub fn new() -> Self;

    /// Push a new destination onto the stack. The next render shows its page.
    pub fn push(&self, dest: Dest);

    /// Pop the top destination. No-op at root (returns `None`).
    /// Returns the popped value when the path was non-empty.
    pub fn pop(&self) -> Option<Dest>;

    /// Clear the entire path, returning to root. Idempotent: a no-op (and no
    /// dirty fire) if the path is already empty.
    pub fn pop_to_root(&self);

    /// Replace the top of the stack with `dest`. At root (empty path), behaves
    /// as `push(dest)` — documented, not an error.
    pub fn replace(&self, dest: Dest);

    /// Snapshot the current path for inspection.
    pub fn path(&self) -> Vec<Dest>;

    /// Current stack depth (path length). `0` means at root.
    pub fn depth(&self) -> usize;

    // --- Framework wiring (called by NavigationStackViewState lifecycle) ---

    /// Wire the dirty callback. Called from `ComponentState::on_mount` (and
    /// `on_update` when the widget's controller instance changes), reading the
    /// controller off `ctx.widget()`. Takes `&self` because the callback cell
    /// is a `RefCell`.
    pub fn set_dirty_callback(&self, callback: Arc<dyn Fn() + Send + Sync>);

    /// Clear the dirty callback. Called from `ComponentState::on_unmount`.
    pub fn clear_dirty_callback(&self);

    /// Fire the dirty callback if set. Called by `push`/`pop`/etc. after
    /// mutating the path.
    fn notify(&self);
}

impl<Dest: Hash + Eq + Clone + 'static> Clone for NavigationController<Dest> {
    // Shallow Rc clone — both `path` and `dirty_callback` are shared, so clones
    // observe and mutate the same stack and fire the same callback. Matches
    // TextEditingController's clone semantics.
}

impl<Dest: Hash + Eq + Clone + 'static> Default for NavigationController<Dest> {}
```

#### Push/pop implementation sketch

```rust
pub fn push(&self, dest: Dest) {
    self.path.borrow_mut().push(dest);
    self.notify();
}

pub fn pop(&self) -> Option<Dest> {
    let popped = self.path.borrow_mut().pop();
    if popped.is_some() {
        self.notify();
    }
    popped
}

pub fn pop_to_root(&self) {
    let mut p = self.path.borrow_mut();
    if p.is_empty() {
        return; // idempotent — no notify
    }
    p.clear();
    drop(p);
    self.notify();
}

pub fn replace(&self, dest: Dest) {
    let mut p = self.path.borrow_mut();
    if let Some(top) = p.last_mut() {
        *top = dest;
    } else {
        p.push(dest); // at root → behaves as push
    }
    drop(p);
    self.notify();
}

fn notify(&self) {
    if let Some(cb) = self.dirty_callback.borrow().as_ref() {
        cb();
    }
}
```

### `NavigationStackView<Dest>`

```rust
pub struct NavigationStackView<Dest: Hash + Eq + Clone + 'static> {
    controller: NavigationController<Dest>,
    root: Box<dyn Widget>,
    destination: Rc<dyn Fn(&Dest) -> Box<dyn Widget>>,
    title: Rc<dyn Fn(&Dest) -> String>,
    root_title: Option<String>,
    platform: Option<Platform>,
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationStackView<Dest> {
    /// Create a stack view with the given controller and root page widget.
    pub fn new(controller: NavigationController<Dest>, root: impl Widget + 'static) -> Self;

    /// Provide a closure that builds the page widget for a pushed destination.
    /// Called at most once per rebuild, with `path.last()`.
    pub fn destination<F: Fn(&Dest) -> Box<dyn Widget> + 'static>(self, f: F) -> Self;

    /// Provide a closure returning the NavBar title for a pushed destination.
    /// Default: returns an empty string.
    pub fn title<F: Fn(&Dest) -> String + 'static>(self, f: F) -> Self;

    /// Set the NavBar title shown when at root. Default: `None` (empty title).
    pub fn root_title(self, title: impl Into<String>) -> Self;

    /// Override the platform. Currently a no-op (rendering is identical on all
    /// platforms); reserved for future desktop adaptation.
    pub fn platform(self, p: Platform) -> Self;
}

impl<Dest: Hash + Eq + Clone + 'static> Clone for NavigationStackView<Dest> {}

impl<Dest: Hash + Eq + Clone + 'static> Component for NavigationStackView<Dest> {
    type State = NavigationStackViewState<Dest>;
    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget>;
}
```

### `NavigationStackViewState<Dest>`

```rust
pub struct NavigationStackViewState<Dest: Hash + Eq + Clone + 'static> {
    // No fields. The controller lives on the widget (caller-supplied), not on
    // the state. Lifecycle hooks read it off `ctx.widget()` and wire/unwire
    // its dirty callback — exactly like TextEditState.
    _marker: PhantomData<Dest>,
}

impl<Dest: Hash + Eq + Clone + 'static> Default for NavigationStackViewState<Dest> {}

impl<Dest: Hash + Eq + Clone + 'static> ComponentState for NavigationStackViewState<Dest> {
    // set_dirty_callback is a no-op: there are no state-owned Signals to wire.
    // The controller's dirty callback is wired in on_mount (below), reading
    // the controller off ctx.widget() — the TextEdit pattern.

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

The controller is the same instance the caller holds (shared `Rc`), so external mutations
flow in through the wired dirty callback. The state exists only to host the lifecycle hooks.

## Rendering & Data Flow

`Component::render` reads `controller.path()` each rebuild. Two cases:

### Path empty (root visible)

```
Flex::column
├── NavBar(root_title, can_pop=false)
└── root                       // caller-provided widget tree, rendered as-is
```

### Path non-empty (top-of-stack visible)

```
Flex::column
├── NavBar(title(path.last()), can_pop=true → controller.pop())
└── destination(path.last())   // caller-provided widget tree, as-is
```

The `root` widget and the `destination` closure return the caller's full page widget tree. The
component adds only the NavBar above it — no `ScrollView`, no padding, no background. Callers
who want scrolling wrap their page in `ScrollView` themselves.

### NavBar

Built by a private helper `build_nav_bar(&self, title: &str, can_pop: bool, controller: &NavigationController<Dest>) -> Box<dyn Widget>`:

```
Flex::row [
    height=MOBILE_HEADER_HEIGHT,
    padding=MOBILE_HEADER_PADDING,
    background=MOBILE_HEADER_BG,
    flex_shrink=0.0,
]
├── if can_pop:
│     Button(format!("{} {}", BACK_CHEVRON, BACK_LABEL))
│         .variant(ButtonVariant::Ghost)
│         .on_press(clone controller → move || controller.pop())
│ else:
│     (no back button rendered — title occupies the row)
└── Text(title)
        .with_font_size(MOBILE_TITLE_FONT_SIZE)
        .with_color(MOBILE_TITLE_COLOR)
```

All visual constants come from existing `tokens::navigation::MOBILE_*` and `BACK_*` — no new
theme tokens for v1.

### Key mechanics

1. **Single-page rendering.** Only the top-of-stack page is built each frame. No off-screen
   layers are kept alive. The `destination` closure is invoked at most once per rebuild, with
   `path.last()`.

2. **State preservation across pop is a non-goal.** Pushed pages are rebuilt from scratch on
   each push. Stateful sub-components (e.g. a `TextEdit` inside a pushed page) lose internal
   state when the page is popped and re-pushed. Callers wanting persistence hoist state into
   the controller or app state.

3. **External mutation → rebuild.** When any closure calls `controller.push(...)` / `pop()` /
   etc., the controller mutates its shared `Rc<RefCell<Vec<Dest>>>` path and calls `notify()`,
   which fires the dirty callback wired during `on_mount`. That callback marks this element
   dirty in the `BuildOwner`; the next frame's `render` runs with the new path. No special
   plumbing beyond the existing controller-wiring machinery.

4. **Back button at root.** When `path.is_empty()`, `can_pop = false`: no back button is
   rendered. The root's own navigation is the caller's responsibility via the `root` widget
   tree.

## Edge Cases

| Case | Behavior |
| --- | --- |
| `pop()` at root (`path.is_empty()`) | Returns `None`, no-op. No dirty fire, no panic. NavBar renders without back button. |
| `replace()` at root | Behaves as `push(dest)`. Documented, not an error. |
| `pop_to_root()` at root | No-op (idempotent). Does not fire dirty if path was already empty. |
| Empty title (`title()` returns `""` or `root_title` unset) | NavBar renders an empty `Text`, height unchanged. No layout collapse. |
| `destination` closure panics | Propagates like any widget builder. Same contract as `NavigationSplitView::detail`. |

## Testing

Tests live in `vexo_uikit/tests/navigation_stack_tests.rs`, mirroring the structure of
`navigation_render_tests.rs`. They use the same `RenderContext` harness — no GPU, no window.

### Controller unit tests

- `controller_default_path_is_empty`
- `controller_push_pop_round_trip` (push A, push B, pop → A, pop → None at root)
- `controller_pop_to_root_clears_path`
- `controller_replace_swaps_top` (push A, replace B → `path == [B]`)
- `controller_replace_at_root_behaves_as_push`
- `controller_pop_at_root_is_noop`
- `controller_depth_tracks_path_length`

### Render tests

- `stack_render_root_does_not_panic` (empty path, root shown)
- `stack_render_pushed_page_does_not_panic` (path non-empty)
- `stack_root_has_no_back_button` (collect text, assert no `BACK_LABEL`)
- `stack_pushed_page_has_back_button` (assert `BACK_LABEL` present)
- `stack_navbar_title_uses_root_title_at_root`
- `stack_navbar_title_uses_destination_title_when_pushed`
- `stack_destination_builder_invoked_once_per_render` (counter, like the split-view test)
- `stack_destination_not_invoked_at_root`
- `stack_pop_via_controller_round_trip` (clone controller, call `pop()` directly, re-render,
  assert root visible)

## Integration

- **Module placement:** lives in `vexo_uikit/src/navigation.rs` alongside
  `NavigationSplitView`. Re-exported from `vexo_uikit/src/lib.rs`:
  ```rust
  pub use navigation::{
      NavigationController, NavigationItem, NavigationSplitView,
      NavigationSplitViewState, NavigationStackView,
  };
  ```

- **No `shared_app` demo change.** The existing `NavigationSplitView` demo is left untouched
  to keep the diff focused. A demo swap can be a follow-up.

- **No new theme tokens.** All visuals come from existing
  `tokens::navigation::MOBILE_*` / `BACK_*` constants.

- **No `vexo/` framework changes.** Built entirely from existing primitives (`Component`,
  `Flex`, `Button`, `Text`) plus stdlib `Rc<RefCell<...>>` for the controller's shared state —
  no new Element or RenderObject, matching the `NavigationSplitView` precedent.

- **`.platform()` is a no-op in v1.** Accepted by the builder for forward compatibility,
  documented as currently ignored.

## Usage Example

```rust
use vexo::{Column, Text, Widget};
use vexo_uikit::{Button, ButtonVariant, NavigationController, NavigationStackView};

let controller = NavigationController::<&'static str>::new();
let root = Column::new()
    .push(Text::new("Root"))
    .push(
        Button::new("Push detail")
            .variant(ButtonVariant::Primary)
            .on_press({
                let c = controller.clone();
                move || c.push("detail")
            }),
    );

NavigationStackView::new(controller.clone(), root)
    .root_title("Home")
    .title(|d| format!("{}", d))
    .destination(|d| {
        let c = controller.clone();
        Column::new()
            .push(Text::new(format!("Page: {}", d)))
            .push(
                Button::new("Push deeper")
                    .on_press(move || c.push("deeper")),
            )
            .boxed()
    })
    .boxed()
```
