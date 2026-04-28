# Vexo Component System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a component system to Vexo that enables reusable components with local state, message isolation, and auto-scoped WidgetIds.

**Architecture:** Introduce a `Component` trait that mirrors the `Application` trait pattern but for widget subtrees. State is stored externally in `ComponentStateStorage` (type-erased HashMap). WidgetIds are auto-scoped using `KeyPath` for hierarchical namespacing. Message isolation is achieved via a `map_message()` function that converts internal messages to parent messages.

**Tech Stack:** Rust, std::any::Any for type erasure, existing Vexo Widget trait

---

## File Structure

```
vexo/src/
├── component/
│   ├── mod.rs           # Module exports, Component trait
│   ├── context.rs       # ComponentContext, KeyPath
│   ├── storage.rs       # ComponentStateStorage
│   └── widget.rs        # ComponentWidget bridge
├── state/
│   └── registry.rs      # Extended with component_storage field
└── lib.rs               # Add `pub mod component;`
```

---

## Task 1: Create Component Module Structure

**Files:**
- Create: `vexo/src/component/mod.rs`

- [ ] **Step 1: Create the component module directory and mod.rs**

```rust
//! Component system for reusable UI building blocks.
//!
//! Components provide:
//! - Local state that persists across view tree rebuilds
//! - Message isolation (each component has its own message type)
//! - Auto-scoped WidgetIds to prevent collisions
//!
//! # Example
//!
//! ```
//! use vexo::component::{Component, ComponentContext, ComponentWidget};
//! use vexo::widgets::Widget;
//!
//! struct MyComponent;
//!
//! impl Component for MyComponent {
//!     type Message = MyMessage;
//!     type Output = MyOutput;
//!     type State = MyState;
//!     
//!     fn update(state: &mut Self::State, message: Self::Message) {
//!         // Handle internal messages
//!     }
//!     
//!     fn view(state: &Self::State, ctx: &mut ComponentContext<'_, Self::Message>) -> Box<dyn Widget<Self::Output>> {
//!         // Render widget tree
//! #       unimplemented!()
//!     }
//!     
//!     fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output> {
//!         // Map internal messages to output
//! #       None
//!     }
//! }
//! ```

mod context;
mod storage;
mod widget;

pub use context::{ComponentContext, KeyPath};
pub use storage::ComponentStateStorage;
pub use widget::ComponentWidget;

// Component trait defined here (depends on all above)
use crate::widgets::Widget;

/// A reusable component with local state and message isolation.
///
/// Components encapsulate:
/// - Local state (stored externally in ComponentStateStorage)
/// - Internal message handling
/// - Message mapping to parent
///
/// This trait mirrors the `Application` trait pattern but for widget subtrees.
pub trait Component: Sized + 'static {
    /// Messages this component handles internally.
    type Message: Clone + std::fmt::Debug + Send;

    /// Messages this component emits to its parent.
    type Output: Clone + std::fmt::Debug + Send;

    /// Component's local state.
    type State: Default;

    /// Create initial state. Called once when component mounts.
    fn initial_state() -> Self::State {
        Self::State::default()
    }

    /// Update state in response to internal messages.
    fn update(state: &mut Self::State, message: Self::Message);

    /// Render the component's widget tree.
    fn view(
        state: &Self::State,
        ctx: &mut ComponentContext<'_, Self::Message>,
    ) -> Box<dyn Widget<Self::Output>>;

    /// Map internal message to output message.
    /// Return None to swallow the message (internal handling only).
    fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output>;
}
```

- [ ] **Step 2: Add component module to lib.rs**

In `vexo/src/lib.rs`, add after line 33 (`pub mod widgets;`):

```rust
pub mod component;
```

- [ ] **Step 3: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Errors about missing `context`, `storage`, `widget` modules (expected - we'll create them next)

- [ ] **Step 4: Commit**

```bash
git add vexo/src/component/mod.rs vexo/src/lib.rs
git commit -m "feat(component): create component module structure"
```

---

## Task 2: Implement KeyPath for Auto-Scoping

**Files:**
- Create: `vexo/src/component/context.rs`

- [ ] **Step 1: Write the failing test for KeyPath**

In `vexo/src/component/context.rs`:

```rust
//! Component context and key path for auto-scoping.

use std::cell::Cell;

/// Hierarchical key path for automatic WidgetId scoping.
///
/// Prevents WidgetId collisions when the same component type
/// appears multiple times in the tree.
///
/// # Example
///
/// ```
/// use vexo::component::KeyPath;
///
/// let root = KeyPath::root();
/// let child = root.child("login_form");
/// 
/// assert_eq!(child.scoped("username"), "login_form/username");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPath {
    segments: Vec<String>,
}

impl KeyPath {
    /// Create a root key path (no segments).
    pub fn root() -> Self {
        Self { segments: Vec::new() }
    }

    /// Create a child key path by appending a segment.
    pub fn child(&self, segment: &str) -> Self {
        let mut segments = self.segments.clone();
        segments.push(segment.to_string());
        Self { segments }
    }

    /// Generate a scoped key string.
    ///
    /// Example: `["app", "login"]` + `"username"` → `"app/login/username"`
    pub fn scoped(&self, local_key: &str) -> String {
        let mut result = self.segments.join("/");
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str(local_key);
        result
    }
}

impl Default for KeyPath {
    fn default() -> Self {
        Self::root()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypath_root() {
        let root = KeyPath::root();
        assert_eq!(root.scoped("widget"), "widget");
    }

    #[test]
    fn test_keypath_child() {
        let root = KeyPath::root();
        let child = root.child("login");
        assert_eq!(child.scoped("username"), "login/username");
    }

    #[test]
    fn test_keypath_nested() {
        let root = KeyPath::root();
        let app = root.child("app");
        let login = app.child("login");
        assert_eq!(login.scoped("username"), "app/login/username");
    }

    #[test]
    fn test_keypath_multiple_widgets() {
        let form = KeyPath::root().child("form");
        
        // Same component, different widgets - different IDs
        let id1 = form.scoped("field1");
        let id2 = form.scoped("field2");
        
        assert_ne!(id1, id2);
        assert_eq!(id1, "form/field1");
        assert_eq!(id2, "form/field2");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo -- component::context`
Expected: All 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/component/context.rs
git commit -m "feat(component): add KeyPath for auto-scoping WidgetIds"
```

---

## Task 3: Implement ComponentStateStorage

**Files:**
- Create: `vexo/src/component/storage.rs`

- [ ] **Step 1: Write ComponentStateStorage with tests**

In `vexo/src/component/storage.rs`:

```rust
//! Type-erased storage for component state.

use std::any::Any;
use std::collections::HashMap;

/// Storage for component state, keyed by scoped string ID.
///
/// Uses type-erased storage (`Box<dyn Any>`) to support any state type.
/// Similar to SwiftUI's external state storage or React's hooks array.
///
/// # Example
///
/// ```
/// use vexo::component::ComponentStateStorage;
///
/// let mut storage = ComponentStateStorage::new();
///
/// // State is created on first access
/// let state = storage.get_or_create::<i32>("counter");
/// assert_eq!(*state, 0);
///
/// // Modify state
/// *state = 42;
///
/// // Retrieve same state
/// let state2 = storage.get_or_create::<i32>("counter");
/// assert_eq!(*state2, 42);
/// ```
pub struct ComponentStateStorage {
    states: HashMap<String, Box<dyn Any>>,
}

impl Default for ComponentStateStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentStateStorage {
    /// Create a new empty storage.
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    /// Get or create state by key.
    ///
    /// State is created lazily on first access using `Default`.
    /// Subsequent accesses return the existing state.
    ///
    /// # Panics
    ///
    /// Panics if the same key was previously used with a different type.
    pub fn get_or_create<S: Default + 'static>(&mut self, key: &str) -> &mut S {
        self.states
            .entry(key.to_string())
            .or_insert_with(|| Box::new(S::default()))
            .downcast_mut::<S>()
            .expect("State type mismatch - same key used with different types")
    }

    /// Check if state exists for a key.
    pub fn contains(&self, key: &str) -> bool {
        self.states.contains_key(key)
    }

    /// Remove state for a key (when component unmounts).
    pub fn remove(&mut self, key: &str) -> Option<Box<dyn Any>> {
        self.states.remove(key)
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.states.clear();
    }

    /// Get the number of stored states.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// Check if storage is empty.
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_create() {
        let mut storage = ComponentStateStorage::new();
        
        let state = storage.get_or_create::<i32>("counter");
        assert_eq!(*state, 0); // Default value
    }

    #[test]
    fn test_storage_persist() {
        let mut storage = ComponentStateStorage::new();
        
        // First access creates state
        *storage.get_or_create::<i32>("counter") = 42;
        
        // Second access retrieves same state
        let state = storage.get_or_create::<i32>("counter");
        assert_eq!(*state, 42);
    }

    #[test]
    fn test_storage_multiple_keys() {
        let mut storage = ComponentStateStorage::new();
        
        *storage.get_or_create::<i32>("a") = 1;
        *storage.get_or_create::<i32>("b") = 2;
        
        assert_eq!(*storage.get_or_create::<i32>("a"), 1);
        assert_eq!(*storage.get_or_create::<i32>("b"), 2);
    }

    #[test]
    fn test_storage_different_types() {
        let mut storage = ComponentStateStorage::new();
        
        *storage.get_or_create::<i32>("int") = 42;
        *storage.get_or_create::<String>("string") = "hello".to_string();
        
        assert_eq!(*storage.get_or_create::<i32>("int"), 42);
        assert_eq!(*storage.get_or_create::<String>("string"), "hello");
    }

    #[test]
    fn test_storage_remove() {
        let mut storage = ComponentStateStorage::new();
        
        *storage.get_or_create::<i32>("counter") = 42;
        assert!(storage.contains("counter"));
        
        storage.remove("counter");
        assert!(!storage.contains("counter"));
        
        // Re-creating gives fresh state
        let state = storage.get_or_create::<i32>("counter");
        assert_eq!(*state, 0);
    }

    #[test]
    fn test_storage_clear() {
        let mut storage = ComponentStateStorage::new();
        
        *storage.get_or_create::<i32>("a") = 1;
        *storage.get_or_create::<i32>("b") = 2;
        
        storage.clear();
        
        assert!(storage.is_empty());
    }

    #[derive(Default, Debug, PartialEq)]
    struct MyState {
        count: u32,
        name: String,
    }

    #[test]
    fn test_storage_custom_type() {
        let mut storage = ComponentStateStorage::new();
        
        let state = storage.get_or_create::<MyState>("my_state");
        state.count = 10;
        state.name = "test".to_string();
        
        let state = storage.get_or_create::<MyState>("my_state");
        assert_eq!(state.count, 10);
        assert_eq!(state.name, "test");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo -- component::storage`
Expected: All 7 tests PASS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/component/storage.rs
git commit -m "feat(component): add ComponentStateStorage with type-erased state"
```

---

## Task 4: Implement ComponentContext

**Files:**
- Modify: `vexo/src/component/context.rs`

- [ ] **Step 1: Add ComponentContext to context.rs**

Add to `vexo/src/component/context.rs` after the `KeyPath` implementation:

```rust
use crate::core::{Scale, WidgetId};
use crate::state::WidgetStateRegistry;
use glyphon::FontSystem;
use std::cell::Cell;

// ... existing KeyPath code ...

/// Context provided to components during `view()`.
///
/// Provides:
/// - Scoped WidgetId generation (auto-namespacing)
/// - Access to component state storage
/// - Font system and scale factor
///
/// # Example
///
/// ```
/// use vexo::component::{ComponentContext, KeyPath, ComponentStateStorage};
/// use vexo::core::WidgetId;
/// use glyphon::FontSystem;
/// use vexo::core::Scale;
///
/// let mut storage = ComponentStateStorage::new();
/// let mut font_system = FontSystem::new();
/// let mut ctx = ComponentContext::new(
///     KeyPath::root().child("login"),
///     &mut storage,
///     &mut font_system,
///     Scale::new(1.0),
/// );
///
/// // Generate scoped WidgetId
/// let id = ctx.widget_id("username");
/// assert_eq!(id, WidgetId::from_key("login/username"));
/// ```
pub struct ComponentContext<'a, M: Clone + std::fmt::Debug + Send> {
    /// Hierarchical key path for auto-scoping
    key_path: KeyPath,

    /// State storage for component instances
    state_storage: &'a mut ComponentStateStorage,

    /// Font system for text rendering
    font_system: &'a mut FontSystem,

    /// Current scale factor
    scale: Scale,

    /// Counter for auto-generating unique IDs
    auto_id_counter: Cell<u32>,

    /// Message type marker
    _marker: std::marker::PhantomData<M>,
}

impl<'a, M: Clone + std::fmt::Debug + Send> ComponentContext<'a, M> {
    /// Create a new component context.
    pub fn new(
        key_path: KeyPath,
        state_storage: &'a mut ComponentStateStorage,
        font_system: &'a mut FontSystem,
        scale: Scale,
    ) -> Self {
        Self {
            key_path,
            state_storage,
            font_system,
            scale,
            auto_id_counter: Cell::new(0),
            _marker: std::marker::PhantomData,
        }
    }

    /// Generate a scoped WidgetId from a local key.
    ///
    /// The resulting WidgetId is unique to this component's position in the tree.
    ///
    /// # Example
    ///
    /// If `key_path` is `"login_form"`, then
    /// `ctx.widget_id("username")` produces `"login_form/username"`
    pub fn widget_id(&self, local_key: &str) -> WidgetId {
        WidgetId::from_key(&self.key_path.scoped(local_key))
    }

    /// Generate an auto-incremented WidgetId.
    ///
    /// Useful when you don't care about the specific ID, just uniqueness.
    pub fn auto_id(&self) -> WidgetId {
        let n = self.auto_id_counter.get();
        self.auto_id_counter.set(n + 1);
        self.widget_id(&format!("auto_{}", n))
    }

    /// Create a child context for nested components.
    ///
    /// Automatically extends the key path for auto-scoping.
    pub fn child_context<N: Clone + std::fmt::Debug + Send>(
        &mut self,
        component_key: &str,
    ) -> ComponentContext<'_, N> {
        ComponentContext {
            key_path: self.key_path.child(component_key),
            state_storage: self.state_storage,
            font_system: self.font_system,
            scale: self.scale,
            auto_id_counter: Cell::new(0),
            _marker: std::marker::PhantomData,
        }
    }

    /// Get the key path.
    pub fn key_path(&self) -> &KeyPath {
        &self.key_path
    }

    /// Get the scale factor.
    pub fn scale(&self) -> Scale {
        self.scale
    }

    /// Get the font system.
    pub fn font_system(&mut self) -> &mut FontSystem {
        self.font_system
    }

    /// Get component state storage.
    pub fn state_storage(&mut self) -> &mut ComponentStateStorage {
        self.state_storage
    }
}
```

- [ ] **Step 2: Add imports at top of context.rs**

Update the imports at the top of `vexo/src/component/context.rs`:

```rust
//! Component context and key path for auto-scoping.

use crate::core::{Scale, WidgetId};
use crate::component::ComponentStateStorage;
use glyphon::FontSystem;
use std::cell::Cell;
```

- [ ] **Step 3: Run tests to verify they still pass**

Run: `cargo test -p vexo -- component::context`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/component/context.rs
git commit -m "feat(component): add ComponentContext with scoped WidgetId generation"
```

---

## Task 5: Extend WidgetStateRegistry

**Files:**
- Modify: `vexo/src/state/registry.rs`

- [ ] **Step 1: Add component_storage field to WidgetStateRegistry**

In `vexo/src/state/registry.rs`, add import at top:

```rust
use crate::component::ComponentStateStorage;
```

Update the struct definition (around line 34):

```rust
pub struct WidgetStateRegistry {
    editor_state: EditorState,
    focus_state: FocusState,
    component_storage: ComponentStateStorage,
}
```

- [ ] **Step 2: Update WidgetStateRegistry methods**

Update `new()` method:

```rust
pub fn new() -> Self {
    Self {
        editor_state: EditorState::new(),
        focus_state: FocusState::new(),
        component_storage: ComponentStateStorage::new(),
    }
}
```

Add component storage accessor methods after the focus management section (around line 140):

```rust
    // ========================================================================
    // Component State Management
    // ========================================================================

    /// Get the component state storage.
    pub fn component_storage(&mut self) -> &mut ComponentStateStorage {
        &mut self.component_storage
    }

    /// Get component state by key (convenience method).
    pub fn get_or_create_component_state<S: Default + 'static>(
        &mut self,
        key: &str,
    ) -> &mut S {
        self.component_storage.get_or_create(key)
    }

    /// Check if component state exists.
    pub fn has_component_state(&self, key: &str) -> bool {
        self.component_storage.contains(key)
    }

    /// Remove component state.
    pub fn remove_component_state(&mut self, key: &str) {
        self.component_storage.remove(key);
    }
```

Update `clear()` method:

```rust
pub fn clear(&mut self) {
    self.editor_state.clear();
    self.focus_state.clear_focus();
    self.component_storage.clear();
}
```

- [ ] **Step 3: Update Debug impl**

```rust
impl std::fmt::Debug for WidgetStateRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetStateRegistry")
            .field("editor_count", &self.editor_state.len())
            .field("focused_widget", &self.focus_state.focused())
            .field("component_state_count", &self.component_storage.len())
            .finish()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo -- state::registry`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/state/registry.rs
git commit -m "feat(state): add ComponentStateStorage to WidgetStateRegistry"
```

---

## Task 6: Implement ComponentWidget Bridge

**Files:**
- Create: `vexo/src/component/widget.rs`

- [ ] **Step 1: Create ComponentWidget**

In `vexo/src/component/widget.rs`:

```rust
//! Bridge between Component trait and Widget trait.

use crate::component::{Component, ComponentContext, ComponentStateStorage, KeyPath};
use crate::core::{Logical, Point, Scale, WidgetId};
use crate::input::{CursorIcon, InputEvent};
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::render::RenderCommand;
use crate::renderer::UiBatcher;
use crate::testable::{ComputedLayout, PaintContext};
use crate::widgets::{Widget, WidgetContext, WidgetResponse};
use crate::CursorBlinkState;
use glyphon::FontSystem;

/// Widget wrapper that hosts a Component.
///
/// This is the bridge between the `Widget` trait (used by the rendering
/// pipeline) and the `Component` trait (used by application code).
///
/// # Example
///
/// ```
/// use vexo::component::{ComponentWidget, LoginComponent};
/// use vexo::widgets::Widget;
///
/// // Create a component widget
/// let login = ComponentWidget::<LoginComponent>::new("login_form");
///
/// // Use in parent's view()
/// // Column::new().push(login.boxed())
/// ```
pub struct ComponentWidget<C: Component> {
    /// The component's state
    state: C::State,

    /// Key for state lookup in storage
    storage_key: String,

    /// Key path for auto-scoping
    key_path: KeyPath,

    /// Cached view widget (rebuilt each frame)
    cached_view: Option<Box<dyn Widget<C::Output>>>,

    /// Computed layout (received after layout phase)
    computed_layout: Option<ComputedLayout>,
}

impl<C: Component> ComponentWidget<C> {
    /// Create a new component widget.
    ///
    /// The `storage_key` is used for state persistence and WidgetId scoping.
    pub fn new(storage_key: impl Into<String>) -> Self {
        let storage_key = storage_key.into();
        let key_path = KeyPath::root().child(&storage_key);
        Self {
            state: C::initial_state(),
            storage_key,
            key_path,
            cached_view: None,
            computed_layout: None,
        }
    }

    /// Create with a custom initial state.
    pub fn with_state(mut self, state: C::State) -> Self {
        self.state = state;
        self
    }

    /// Get the storage key.
    pub fn storage_key(&self) -> &str {
        &self.storage_key
    }

    /// Get the component's state.
    pub fn state(&self) -> &C::State {
        &self.state
    }

    /// Get mutable access to the component's state.
    pub fn state_mut(&mut self) -> &mut C::State {
        &mut self.state
    }
}

impl<C: Component> Widget<C::Output> for ComponentWidget<C> {
    fn key(&self) -> Option<&str> {
        Some(&self.storage_key)
    }

    fn layout_props(&self) -> Layout {
        // Delegate to cached view if available
        if let Some(ref view) = self.cached_view {
            view.layout_props()
        } else {
            Layout::default()
        }
    }

    fn cursor(&self) -> CursorIcon {
        if let Some(ref view) = self.cached_view {
            view.cursor()
        } else {
            CursorIcon::Default
        }
    }

    fn layout(
        &mut self,
        layout_ctx: &mut LayoutContext,
        widget_ctx: &mut WidgetContext,
    ) -> LayoutNodeId {
        // Create component context for view()
        let mut component_ctx = ComponentContext::new(
            self.key_path.clone(),
            widget_ctx.state.component_storage(),
            &mut widget_ctx.font_system,
            widget_ctx.scale,
        );

        // Rebuild view each frame (Vexo's pattern)
        let view = C::view(&self.state, &mut component_ctx);
        self.cached_view = Some(view);

        // Layout the cached view
        if let Some(ref mut view) = self.cached_view {
            view.layout(layout_ctx, widget_ctx)
        } else {
            layout_ctx.create_leaf(&Layout::default())
        }
    }

    fn apply_layout(&mut self, layout: ComputedLayout) {
        self.computed_layout = Some(layout);
        
        // Also propagate to cached view
        if let Some(ref mut view) = self.cached_view {
            view.apply_layout(layout);
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Delegate to cached view
        if let Some(ref view) = self.cached_view {
            view.paint(ctx)
        } else {
            Vec::new()
        }
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &CursorBlinkState,
        widget_ctx: &mut WidgetContext,
    ) {
        // Delegate to cached view
        if let Some(ref view) = self.cached_view {
            view.draw(
                layout_view,
                node,
                renderer,
                offset,
                focused_id,
                cursor_blink,
                widget_ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_ctx: &mut WidgetContext,
    ) -> WidgetResponse<C::Output> {
        // Propagate event to cached view
        let response = if let Some(ref mut view) = self.cached_view {
            view.on_event(
                layout_view,
                node,
                offset,
                event,
                focused_id,
                widget_ctx,
            )
        } else {
            WidgetResponse::default()
        };

        // Handle internal messages
        if let Some(internal_msg) = response.message {
            // Update component state
            C::update(&mut self.state, internal_msg.clone());
            
            // Map to output message
            let output_msg = C::map_message(internal_msg, &self.state);
            
            WidgetResponse {
                message: output_msg,
                focus_request: response.focus_request,
                handled: response.handled,
                clear_focus: response.clear_focus,
                cursor: response.cursor,
            }
        } else {
            response
        }
    }
}

// Enable Box<dyn Widget<C::Output>> pattern
impl<C: Component> Widget<C::Output> for Box<ComponentWidget<C>> {
    fn key(&self) -> Option<&str> {
        (**self).key()
    }

    fn layout_props(&self) -> Layout {
        (**self).layout_props()
    }

    fn cursor(&self) -> CursorIcon {
        (**self).cursor()
    }

    fn layout(
        &mut self,
        layout_ctx: &mut LayoutContext,
        widget_ctx: &mut WidgetContext,
    ) -> LayoutNodeId {
        (**self).layout(layout_ctx, widget_ctx)
    }

    fn apply_layout(&mut self, layout: ComputedLayout) {
        (**self).apply_layout(layout)
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        (**self).paint(ctx)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &CursorBlinkState,
        widget_ctx: &mut WidgetContext,
    ) {
        (**self).draw(layout_view, node, renderer, offset, focused_id, cursor_blink, widget_ctx)
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_ctx: &mut WidgetContext,
    ) -> WidgetResponse<C::Output> {
        (**self).on_event(layout_view, node, offset, event, focused_id, widget_ctx)
    }
}
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/component/widget.rs
git commit -m "feat(component): add ComponentWidget bridge between Component and Widget"
```

---

## Task 7: Add Tests for Component Integration

**Files:**
- Modify: `vexo/src/component/mod.rs`

- [ ] **Step 1: Add integration test module**

Add to `vexo/src/component/mod.rs` at the end:

```rust
// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{Column, Widget, WidgetContext};
    use crate::core::WidgetId;
    use crate::layout::{LayoutContext, LayoutEngine, TaffyLayoutEngine};
    use glyphon::FontSystem;
    use crate::core::Scale;

    // Simple test component
    #[derive(Clone, Debug)]
    enum TestMessage {
        Increment,
    }

    #[derive(Clone, Debug)]
    enum TestOutput {
        CountReached(u32),
    }

    #[derive(Default)]
    struct TestState {
        count: u32,
    }

    struct TestComponent;

    impl Component for TestComponent {
        type Message = TestMessage;
        type Output = TestOutput;
        type State = TestState;

        fn update(state: &mut Self::State, message: Self::Message) {
            match message {
                TestMessage::Increment => state.count += 1,
            }
        }

        fn view(
            _state: &Self::State,
            _ctx: &mut ComponentContext<'_, Self::Message>,
        ) -> Box<dyn Widget<Self::Output>> {
            // Simple empty column for testing
            Box::new(Column::new())
        }

        fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output> {
            match message {
                TestMessage::Increment if state.count >= 3 => {
                    Some(TestOutput::CountReached(state.count))
                }
                _ => None,
            }
        }
    }

    #[test]
    fn test_component_widget_creation() {
        let widget = ComponentWidget::<TestComponent>::new("test");
        assert_eq!(widget.storage_key(), "test");
        assert_eq!(widget.state().count, 0);
    }

    #[test]
    fn test_component_state_update() {
        let mut widget = ComponentWidget::<TestComponent>::new("test");
        
        // Simulate update
        TestComponent::update(&mut widget.state_mut(), TestMessage::Increment);
        assert_eq!(widget.state().count, 1);
        
        TestComponent::update(&mut widget.state_mut(), TestMessage::Increment);
        assert_eq!(widget.state().count, 2);
    }

    #[test]
    fn test_component_message_mapping() {
        let mut widget = ComponentWidget::<TestComponent>::new("test");
        
        // Update to count = 3
        for _ in 0..3 {
            TestComponent::update(&mut widget.state_mut(), TestMessage::Increment);
        }
        
        // Now message should map to output
        let output = TestComponent::map_message(TestMessage::Increment, &widget.state());
        assert!(matches!(output, Some(TestOutput::CountReached(3))));
    }

    #[test]
    fn test_component_context_widget_id() {
        let mut storage = ComponentStateStorage::new();
        let mut font_system = FontSystem::new();
        let ctx = ComponentContext::<TestMessage>::new(
            KeyPath::root().child("my_component"),
            &mut storage,
            &mut font_system,
            Scale::new(1.0),
        );

        let id = ctx.widget_id("my_widget");
        assert_eq!(id, WidgetId::from_key("my_component/my_widget"));
    }

    #[test]
    fn test_component_context_auto_id() {
        let mut storage = ComponentStateStorage::new();
        let mut font_system = FontSystem::new();
        let ctx = ComponentContext::<TestMessage>::new(
            KeyPath::root().child("comp"),
            &mut storage,
            &mut font_system,
            Scale::new(1.0),
        );

        let id1 = ctx.auto_id();
        let id2 = ctx.auto_id();
        
        assert_ne!(id1, id2);
        assert_eq!(id1, WidgetId::from_key("comp/auto_0"));
        assert_eq!(id2, WidgetId::from_key("comp/auto_1"));
    }
}
```

- [ ] **Step 2: Run all component tests**

Run: `cargo test -p vexo -- component`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/component/mod.rs
git commit -m "test(component): add integration tests for component system"
```

---

## Task 8: Final Verification

- [ ] **Step 1: Run all vexo tests**

Run: `cargo test -p vexo`
Expected: All tests PASS

- [ ] **Step 2: Build vexo in release mode**

Run: `cargo build -p vexo --release`
Expected: Build succeeds

- [ ] **Step 3: Run desktop demo to verify no regressions**

Run: `cargo run -p desktop_demo`
Expected: Application starts without errors

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat(component): complete component system implementation

- Add Component trait for reusable UI building blocks
- Add ComponentStateStorage for type-erased state persistence
- Add KeyPath for auto-scoping WidgetIds
- Add ComponentContext for component view() method
- Add ComponentWidget bridge to Widget trait
- Extend WidgetStateRegistry with component storage

Components now support:
- Local state that persists across view tree rebuilds
- Message isolation with map_message()
- Auto-scoped WidgetIds to prevent collisions"
```

---

## Summary

This implementation adds a complete component system to Vexo:

| Feature | Implementation |
|---------|---------------|
| Local state | `ComponentStateStorage` with type-erased `Box<dyn Any>` |
| Auto-scoped IDs | `KeyPath` for hierarchical namespacing |
| Message isolation | `Component::Message` type + `map_message()` |
| Component trait | Mirrors `Application` pattern |
| Widget bridge | `ComponentWidget<C: Component>` implements `Widget<C::Output>` |

**Files created:**
- `vexo/src/component/mod.rs`
- `vexo/src/component/context.rs`
- `vexo/src/component/storage.rs`
- `vexo/src/component/widget.rs`

**Files modified:**
- `vexo/src/lib.rs` (added `pub mod component;`)
- `vexo/src/state/registry.rs` (added `component_storage` field)
