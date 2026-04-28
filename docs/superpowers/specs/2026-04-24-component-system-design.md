# Vexo Component System Design

**Date:** 2026-04-24
**Status:** Draft

## Context

Vexo currently provides a `Widget<M>` trait for building UI elements, but lacks a way to define reusable components with:
- Local state that persists across view tree rebuilds
- Props passed from parent components
- Message isolation (components have their own message type)
- Auto-scoped WidgetIds to prevent collisions

This design addresses these gaps by introducing a `Component` trait and supporting infrastructure, inspired by patterns from SwiftUI, React, Flutter, and Jetpack Compose.

---

## Cross-Framework Research Summary

All major UI frameworks converge on a **three-part architecture**:

| Layer | Purpose | Lifecycle |
|-------|---------|-----------|
| **Identity** | Determines if state persists | Stable across renders |
| **State** | Stores mutable data | External storage |
| **View** | Renders current state | Ephemeral, recreated each frame |

**Key insight:** State cannot live inside the view - it must be externalized and keyed by identity.

### Identity Mechanisms

| Framework | Identity Mechanism |
|-----------|-------------------|
| SwiftUI | View position + `.id()` modifier |
| React | Hooks call order + `key` prop |
| Flutter | `runtimeType + Key` |
| Compose | Call site position + `key()` function |

### State Storage Patterns

| Framework | State Storage |
|-----------|--------------|
| SwiftUI | External backing store (keyed by view identity) |
| React | Internal hooks array (keyed by call order) |
| Flutter | Element tree (State objects) |
| Compose | Recomposition scope (positional memoization) |

---

## Architecture

### Component System Layers

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│  Application trait, Message, State, update(), view()        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Component Layer                           │
│  Component trait, ComponentContext, ComponentWidget         │
│  - Local state management                                    │
│  - Message isolation with mapping                            │
│  - Auto-scoped WidgetIds                                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Widget Layer                              │
│  Widget<M> trait, primitives, containers, modifiers         │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    State Layer                               │
│  WidgetStateRegistry, ComponentStateStorage, FocusState     │
└─────────────────────────────────────────────────────────────┘
```

---

## Core Types

### 1. Component Trait

```rust
/// A reusable component with local state and message isolation.
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

### 2. ComponentContext

```rust
/// Context provided to components during view().
pub struct ComponentContext<'a, M: Clone + std::fmt::Debug + Send> {
    key_path: KeyPath,
    state_storage: &'a mut ComponentStateStorage,
    font_system: &'a mut FontSystem,
    scale: Scale,
    _marker: std::marker::PhantomData<M>,
}

impl<'a, M: Clone + std::fmt::Debug + Send> ComponentContext<'a, M> {
    /// Generate a scoped WidgetId from a local key.
    pub fn widget_id(&self, local_key: &str) -> WidgetId {
        WidgetId::from_key(&self.key_path.scoped(local_key))
    }

    /// Create a child context for nested components.
    pub fn child_context(&mut self, component_key: &str) -> ComponentContext<'_, M>;
}
```

### 3. KeyPath (Auto-Scoping)

```rust
/// Hierarchical key path for automatic WidgetId scoping.
#[derive(Debug, Clone)]
pub struct KeyPath {
    segments: Vec<String>,
}

impl KeyPath {
    pub fn root() -> Self;
    pub fn child(&self, segment: &str) -> Self;

    /// Generate scoped key: ["app", "login"] + "username" → "app/login/username"
    pub fn scoped(&self, local_key: &str) -> String;
}
```

### 4. ComponentStateStorage

```rust
/// Storage for component state, keyed by scoped string ID.
/// Uses type-erased storage (Box<dyn Any>).
pub struct ComponentStateStorage {
    states: HashMap<String, Box<dyn Any>>,
}

impl ComponentStateStorage {
    pub fn new() -> Self;
    pub fn get_or_create<S: Default + 'static>(&mut self, key: &str) -> &mut S;
    pub fn remove(&mut self, key: &str);
}
```

### 5. ComponentWidget (Bridge)

```rust
/// Widget wrapper that hosts a Component.
pub struct ComponentWidget<C: Component> {
    state: C::State,
    storage_key: String,
    cached_view: Option<Box<dyn Widget<C::Output>>>,
}

impl<C: Component> Widget<C::Output> for ComponentWidget<C> {
    fn layout(&mut self, layout_ctx: &mut LayoutContext, widget_ctx: &mut WidgetContext) -> LayoutNodeId;
    fn on_event(...) -> WidgetResponse<C::Output>;
}
```

---

## Message Flow

```
User Interaction (click on Button)
         │
         ▼
┌─────────────────────┐
│  Button widget      │  emits LoginMessage::SubmitClicked
│  (inside component) │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  ComponentWidget    │  receives LoginMessage
│                     │
│  1. C::update()     │  modifies LoginState
│  2. C::map_message()│  converts to LoginOutput
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  Parent Widget      │  receives AppMessage
└─────────────────────┘
```

---

## Example: Login Form Component

```rust
#[derive(Clone, Debug)]
pub enum LoginMessage {
    UsernameChanged(String),
    PasswordChanged(String),
    SubmitClicked,
}

#[derive(Clone, Debug)]
pub enum LoginOutput {
    LoginRequested { username: String, password: String },
}

#[derive(Default)]
pub struct LoginState {
    username: String,
    password: String,
}

struct LoginComponent;

impl Component for LoginComponent {
    type Message = LoginMessage;
    type Output = LoginOutput;
    type State = LoginState;

    fn update(state: &mut Self::State, message: Self::Message) {
        match message {
            LoginMessage::UsernameChanged(s) => state.username = s,
            LoginMessage::PasswordChanged(s) => state.password = s,
            LoginMessage::SubmitClicked => {}
        }
    }

    fn view(state: &Self::State, ctx: &mut ComponentContext<'_, Self::Message>) -> Box<dyn Widget<Self::Output>> {
        column![
            text_edit!(ctx.widget_id("username"))
                .content(&state.username)
                .placeholder("Username"),
            text_edit!(ctx.widget_id("password"))
                .content(&state.password)
                .placeholder("Password"),
            button!(text!("Login"))
                .on_press(LoginMessage::SubmitClicked),
        ]
        .padding(16.0)
        .boxed()
    }

    fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output> {
        match message {
            LoginMessage::SubmitClicked => Some(LoginOutput::LoginRequested {
                username: state.username.clone(),
                password: state.password.clone(),
            }),
            _ => None,
        }
    }
}
```

---

## Integration with Vexo

### Extending WidgetStateRegistry

```rust
pub struct WidgetStateRegistry {
    editor_state: EditorState,
    focus_state: FocusState,
    component_storage: ComponentStateStorage,  // NEW
}
```

### File Structure

```
vexo/src/
├── component/
│   ├── mod.rs           # Component trait
│   ├── context.rs       # ComponentContext, KeyPath
│   ├── storage.rs       # ComponentStateStorage
│   └── widget.rs        # ComponentWidget
├── state/
│   └── registry.rs      # Extended
└── lib.rs               # Export component module
```

---

## Implementation Phases

### Phase 1: Foundation
1. Create `vexo/src/component/` module
2. Implement `ComponentStateStorage`
3. Implement `KeyPath`
4. Create `ComponentContext`
5. Extend `WidgetStateRegistry`

### Phase 2: Component Trait
1. Define `Component` trait
2. Implement `ComponentWidget` bridge
3. Add message mapping
4. Handle event propagation

### Phase 3: Ergonomics
1. Builder pattern for configuration
2. Helper macros
3. Documentation and examples

---

## Verification

### Unit Tests
- `ComponentStateStorage::get_or_create` creates and retrieves state
- `KeyPath::scoped` produces correct scoped strings
- `ComponentContext::widget_id` generates unique IDs

### Integration Tests
- Component state persists across view tree rebuilds
- Message mapping works correctly
- Auto-scoped WidgetIds prevent collisions

### Manual Testing
```bash
cargo run -p desktop_demo
cargo test -p vexo -- component
```

---

## Critical Files

| File | Purpose |
|------|---------|
| `vexo/src/component/mod.rs` | Component trait definition |
| `vexo/src/component/context.rs` | ComponentContext, KeyPath |
| `vexo/src/component/storage.rs` | ComponentStateStorage |
| `vexo/src/component/widget.rs` | ComponentWidget bridge |
| `vexo/src/state/registry.rs` | Extend with component_storage |
| `vexo/src/lib.rs` | Export component module |
