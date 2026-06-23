# Web Developer Concept Mapping Design

Date: 2026-06-23

## Problem

Vexo's architecture is sound (Flutter's three-tree model), but its public API speaks Flutter's language, not the web developer's. A React/Vue/Svelte developer encountering Vexo sees:

- `StatefulWidget` + `State` — Flutter jargon, not "component"
- `StatefulMutable<T>` + manual `set_dirty_callback()` — no web framework makes you wire re-render triggers by hand
- `build()` — Flutter's name; web devs say "render"
- `BuildContext` — Flutter-specific; web devs expect "render context" or "setup context"
- `.push().push().boxed()` — imperative chain, not declarative children
- Internal types (`DecoratedContainer`, `WithLayout`, `GestureDetector`) exposed in public API — web devs don't need to see the framework's innards

The goal: **map every public concept to something a web developer already knows**, without changing the three-tree engine.

## Approach

Rename, restructure, and ergonomically improve the public API surface. The engine stays the same. Old names remain as deprecated aliases during transition.

Target audience: React/Vue/Svelte developers who think in components, props, state, and declarative rendering.

## Section 1: Concept Renaming Map

| Current Vexo | Proposed | Web analog | Rationale |
|---|---|---|---|
| `StatefulWidget` trait | `Component` trait | React function component / Vue component | "StatefulWidget" is Flutter jargon. "Component" is universal. |
| `State` trait | `ComponentState` trait | React hooks state / Vue reactive state | `State` alone is too generic, collides with common names. Keeps association to Component. |
| `SimpleState<T>` | Keep, add derive | — | Useful pattern, just needs better ergonomics via derive |
| `StatefulMutable<T>` | `Signal<T>` | Vue `ref()` / Solid signal | "StatefulMutable" is verbose. `Signal` is the emerging cross-framework term. |
| `set_dirty_callback()` | Auto-wired (removed from public API) | React auto-re-renders | Web devs never manually wire re-render triggers. |
| `State::init()` | `ComponentState::on_mount()` | React `useEffect([])` / Vue `onMounted()` | "init" is ambiguous. "on_mount" maps to every framework. |
| `State::did_update_widget()` | `ComponentState::on_update()` | React `useEffect([deps])` / Vue `onUpdated()` | Same pattern, clearer name. |
| `State::dispose()` | `ComponentState::on_unmount()` | React cleanup / Vue `onUnmounted()` | Matches the mount/unmount lifecycle pair. |
| `State::animate()` | `ComponentState::on_tick()` | `requestAnimationFrame` callback | "animate" is too specific — ticks are general. |
| `BuildContext` | `RenderContext` | React render arg / Vue setup return | "Build" is Flutter jargon. "Render" is universal. |
| `StateContext` | `LifecycleContext` | React effect context | Makes clear it's for lifecycle hooks, not rendering. |
| `Widget::build()` (on StatefulWidget) | `Component::render()` | React render / Vue template | "build" is Flutter. "render" is universal. |
| `Widget` | Keep as `Widget` | React element / Vue VNode | "Widget" is intuitive enough. |
| `Flex::column()` / `Flex::row()` | Keep + add `Column::new()` / `Row::new()` aliases | `<div flex-direction>` | `Flex::column()` is fine but `Column`/`Row` are the web dev's mental default. |

## Section 2: Auto-wire Dirty Callbacks

### Current pain

Every `ComponentState` with `Signal<T>` fields must manually implement `set_dirty_callback()`:

```rust
struct CounterState {
    count: Signal<u32>,
}

impl State for CounterState {
    fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.count.set_dirty_callback(callback);
    }
}
```

### Solution: `#[derive(ComponentState)]` macro

```rust
#[derive(ComponentState)]
struct CounterState {
    count: Signal<u32>,
    // more fields auto-wired
}
```

The derive macro generates the `set_dirty_callback` implementation automatically — it iterates every `Signal<T>` field and calls `.set_dirty_callback(callback.clone())` on each.

### Behavior

- For each field of type `Signal<T>`: generates `self.field_name.set_dirty_callback(callback.clone());`
- For non-`Signal` fields: skipped
- For `Option<Signal<T>>` fields: wired if `Some`
- Nested structs containing `Signal` fields: derive only wires top-level fields. Nested structs must derive `ComponentState` themselves and be wired manually in a custom `set_dirty_callback` override.
- Custom `set_dirty_callback` impl: if the user provides their own, it takes precedence over the derive. This should be rare.

### Alternative considered

Make `Signal<T>` auto-detect its owner element via thread-local or registration. Rejected — too implicit, breaks with concurrency, conflicts with Rust's ownership model.

## Section 3: Component Trait Redesign

### Current

```rust
#[derive(Clone)]
struct Counter { label: String }

struct CounterState { count: Signal<u32> }

impl StatefulWidget for Counter {
    type State = CounterState;
    fn build(&self, state: &mut Self::State, ctx: &mut BuildContext) -> Box<dyn Widget>;
}
```

### Proposed

```rust
#[derive(Clone)]
struct Counter { label: String }

#[derive(ComponentState)]
struct CounterState { count: Signal<u32> }

impl Component for Counter {
    type State = CounterState;
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget>;
}
```

### Lifecycle mapping

| Web dev expectation | Vexo `ComponentState` method | When it fires |
|---|---|---|
| Mount / `onMounted()` | `on_mount(&mut self, ctx: &mut LifecycleContext)` | First time element enters tree |
| Update / `onUpdated()` | `on_update(&mut self, old: &dyn Any, ctx: &mut LifecycleContext)` | Parent rebuilt with new props |
| Unmount / `onUnmounted()` | `on_unmount(&mut self, ctx: &mut LifecycleContext)` | Element removed from tree |
| Animation frame / `rAF` | `on_tick(&mut self, now: Instant)` | Every frame, before render |
| Event handler | `on_event(&mut self, widget: &dyn Any, event: &InputEvent, ctx: &mut EventContext)` | Input event hits this element |

All methods have default no-op implementations — override only what you need.

### What stays the same

- `Component::State` must implement `Default` (state initialized lazily)
- `Component` types must implement `Clone` (widgets are immutable snapshots)
- Engine still creates `StatefulElement` under the hood — no architecture change

## Section 4: Declarative Child Composition

### 4a. `Column` and `Row` type aliases

```rust
// Today
Flex::column()
Flex::row()

// Also available
Column::new()   // = Flex::column()
Row::new()      // = Flex::row()
```

Type aliases (`pub type Column = Flex;`), not newtypes. Zero behavioral change. Newtypes would break the `.push()` chain since `push()` is on `Flex`.

### 4b. `children![]` macro

```rust
Column::new()
    .gap(16.0)
    .children![
        Text::new("Title").padding(8.0),
        Text::new("Body"),
        Row::new().children![
            Text::new("A"),
            Text::new("B"),
        ],
    ]
```

Expands to chained `.push()` calls with automatic boxing. Each element is implicitly `.boxed()` if needed.

### Features

- Each child expression produces a `Widget` — the macro boxes it automatically
- Conditional children: `if show_extra { Text::new("Extra") }` — macro filters `Option<Box<dyn Widget>>` (None = skip)
- Spread (`..items`): not supported initially (adds complexity, marginal value)

### Why a macro, not a method

A `.children(Vec<Box<dyn Widget>>)` method requires collecting into a Vec (allocation). The macro expands to chained `.push()` calls — zero overhead. Also handles mixed types without explicit boxing.

### Before/after

```rust
// Before
Flex::column()
    .gap(16.0)
    .push(Text::new("Title").padding(8.0))
    .push(Text::new("Body"))
    .push(Flex::row().push(Text::new("A")).push(Text::new("B")).boxed())
    .boxed()

// After
Column::new()
    .gap(16.0)
    .children![
        Text::new("Title").padding(8.0),
        Text::new("Body"),
        Row::new().children![
            Text::new("A"),
            Text::new("B"),
        ],
    ]
```

## Section 5: Reduce `.boxed()` Friction

### 5a. `.push()` accepts `impl Widget` directly

Today `push()` takes `Box<dyn Widget>`, forcing `.boxed()` at every call site. Change `push()` to accept `impl Into<Box<dyn Widget>>` or be generic over `impl Widget`, boxing internally:

```rust
// Today
.push(Text::new("Hello").padding(8.0).boxed())

// After — no .boxed() needed
.push(Text::new("Hello").padding(8.0))
```

### 5b. `children![]` macro auto-boxes

From Section 4b — the macro automatically boxes each child.

### 5c. Component render return type

`Component::render()` still returns `Box<dyn Widget>` — unavoidable in Rust for polymorphic returns. But it's only one `.boxed()` at the top-level return, not scattered throughout. Acceptable.

### Net effect

`.boxed()` disappears from `.push()` calls and `children![]` bodies. It only appears at the top-level return of `render()`.

## Section 6: Public API Boundary

### Public API (what web devs import and use)

| Category | Types | Web analog |
|---|---|---|
| Components | `Component`, `ComponentState`, `#[derive(ComponentState)]` | React component, Vue setup |
| Reactive state | `Signal<T>`, `Signal::new()`, `.get()`, `.set()` | `useState`, `ref()` |
| Widgets | `Text`, `Column`, `Row`, `Grid`, `Image`, `ScrollView`, `Focus` | HTML elements |
| Styling | `.background()`, `.border()`, `.corner_radius()`, `.clip()`, `Style` | CSS properties |
| Layout | `.padding()`, `.margin()`, `.width()`, `.height()`, `.gap()`, `.flex_grow()`, `.absolute()`, `Layout` | CSS box model |
| Events | `.on_press()`, `.on_release()` | `onClick`, `onMouseUp` |
| Mouse | `.cursor()`, `.on_enter()`, `.on_exit()` | CSS cursor, `onMouseEnter/Leave` |
| Transform | `.translate()`, `.rotate()`, `.scale()` | CSS transform |
| Animation | `AnimationController`, `Tween`, `ColorTween`, `FloatTween` | Web Animations API |
| Focus | `Focus`, `FocusManager` | Tab focus, `focus()` |
| Input | `InputEvent`, `Key`, `NamedKey` | Keyboard/mouse events |
| Composition | `children![]`, `.push()` | JSX children |
| App entry | `Application`, `run_desktop_demo()` | `createRoot().render()` |

### Internal (not re-exported)

| Category | Types | Why internal |
|---|---|---|
| Element tree | `Element`, `ElementRegistry`, `ElementContext`, all element types | Like DOM internals — managed by framework |
| Render objects | `RenderObject`, `RenderObjectRegistry`, all render object types | Like browser layout/paint — managed by framework |
| Reconciliation | `Reconciler`, `update_child()`, `ChildOps` | Like React's diffing — automatic |
| Pipeline | `ThreeTreePipeline`, `BuildOwner`, `DirtyTracking` | Like React's scheduler — automatic |
| Decorator widgets | `DecoratedContainer`, `WithLayout`, `GestureDetector`, `MouseRegion`, `Transform` | Created by modifier methods — devs use methods, not types |
| Keys | `ElementKey`, `RenderObjectKey` | Internal IDs |
| State storage | `StateStorage` | Internal |

### Key change

`DecoratedContainer`, `WithLayout`, `GestureDetector`, `MouseRegion`, and `Transform` become `pub(crate)`. Web devs interact through modifier methods (`.background()`, `.on_press()`, etc.), not by constructing these types directly.

## Section 7: Migration Strategy

### 7a. Dual-export transition period

Both old and new names available simultaneously:

```rust
// New names (primary)
pub use component::Component;
pub use component_state::{ComponentState, Signal};
pub use render_context::RenderContext;
pub use lifecycle_context::LifecycleContext;

// Old names (deprecated aliases)
#[deprecated(since = "0.x", note = "Use `Component` instead")]
pub use component::Component as StatefulWidget;

#[deprecated(since = "0.x", note = "Use `ComponentState` instead")]
pub use component_state::ComponentState as State;

#[deprecated(since = "0.x", note = "Use `Signal` instead")]
pub use component_state::Signal as StatefulMutable;

#[deprecated(since = "0.x", note = "Use `RenderContext` instead")]
pub use render_context::RenderContext as BuildContext;

#[deprecated(since = "0.x", note = "Use `LifecycleContext` instead")]
pub use lifecycle_context::LifecycleContext as StateContext;
```

Existing code compiles with deprecation warnings, not errors. New code uses new names.

### 7b. Internal code migration

Internal engine (elements, render objects, pipeline) continues using old names internally — they're not public API. Only rename public-facing types and traits. Minimizes engine churn while fixing the surface.

### 7c. Phased rollout

1. **Phase 1:** Add new names as aliases. Add `Column`/`Row` aliases. Add `children![]` macro. Add `#[derive(ComponentState)]` macro. Make `.push()` accept `impl Widget`. Hide internal types from public API. Zero breaking changes.
2. **Phase 2:** Update `shared_app` and demos to use new names. Validate the API feels right.
3. **Phase 3:** Remove deprecated aliases. Make new names the only names.
