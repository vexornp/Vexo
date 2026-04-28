# Counter Component Demo Design

**Date:** 2026-04-24
**Status:** Approved

## Context

The Vexo component system is fully implemented. This design specifies how to update the desktop demo to showcase custom component usage with a Counter Component that demonstrates local state, message isolation, and output message mapping.

---

## Component: CounterComponent

### Types

```rust
#[derive(Clone, Debug)]
pub enum CounterMessage {
    Increment,
    Decrement,
    Reset,
}

#[derive(Clone, Debug)]
pub enum CounterOutput {
    CountReached(u32),
}

#[derive(Default)]
pub struct CounterState {
    count: u32,
}

pub struct CounterComponent;
```

### Behavior

- **Increment**: Increases count by 1
- **Decrement**: Decreases count by 1 (minimum 0)
- **Reset**: Sets count back to 0
- **Output**: Emits `CountReached(10)` when count reaches exactly 10

### Message Mapping Logic

```rust
fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output> {
    match message {
        CounterMessage::Increment if state.count == 10 => Some(CounterOutput::CountReached(10)),
        _ => None,
    }
}
```

---

## Application Integration

### App State

```rust
pub struct State {
    milestones: u32,  // Number of times counter reached 10
}
```

### App Message

```rust
pub enum Message {
    CounterOutput(CounterOutput),
}
```

### Layout

```
┌─────────────────────────────────────┐
│  Counter Component Demo             │
│                                     │
│  ┌─────────────────────────────┐   │
│  │  Count: 5                   │   │
│  │  [-]  [+]  [Reset]          │   │
│  └─────────────────────────────┘   │
│                                     │
│  Milestones reached: 0              │
└─────────────────────────────────────┘
```

---

## Implementation

### Files to Modify

| File | Changes |
|------|---------|
| `shared_app/src/lib.rs` | Add `CounterComponent` implementation, update `State` and `Message`, update `view()` |

### Component Implementation

The `CounterComponent` will implement the `Component` trait:
- `initial_state()`: Returns `CounterState::default()` (count = 0)
- `update()`: Handles Increment/Decrement/Reset messages
- `view()`: Renders count display and three buttons using `ComponentContext` for scoped WidgetIds
- `map_message()`: Returns `CountReached(10)` when count hits 10

### Widget Usage

Use `ComponentWidget<CounterComponent>` in the app's `view()` function, wrapped in appropriate styling (padding, background, border).

---

## Verification

### Manual Testing

```bash
cargo run -p desktop_demo
```

1. Click [+] 10 times → "Milestones reached: 1"
2. Click [Reset] → count goes to 0
3. Click [+] 10 times again → "Milestones reached: 2"
4. Click [-] → count decreases
5. Verify count never goes below 0

### Expected Behavior

- Counter state persists across parent re-renders
- WidgetIds are auto-scoped (no collisions)
- Output messages propagate to parent correctly
- Milestone count increments only when counter hits exactly 10
