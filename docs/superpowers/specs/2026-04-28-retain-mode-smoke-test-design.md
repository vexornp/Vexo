# Retain-Mode Smoke Test Design

**Date:** 2026-04-28
**Status:** Design Approved

## Goal

Add a keyboard toggle to the existing demo app to switch between immediate-mode and retain-mode rendering, verifying the retain-mode pipeline works end-to-end.

## Approach

Integrate into `shared_app/src/lib.rs` with a keyboard toggle (press 'R') to switch modes. The retain-mode view shows a simple widget tree to verify rendering.

## Changes

### State

Add `use_retain_mode: bool` to `State` struct:

```rust
pub struct State {
    click_count: u32,
    milestones: u32,
    use_retain_mode: bool,  // New field
}
```

### Message

Add `ToggleRetainMode` to `Message` enum:

```rust
pub enum Message {
    None,
    Clicked,
    CounterOutput(CounterOutput),
    ToggleRetainMode,  // New variant
}
```

### Update

Handle the toggle message:

```rust
fn update(state: &mut Self::State, message: Self::Message) {
    match message {
        // ... existing cases ...
        Message::ToggleRetainMode => {
            state.use_retain_mode = !state.use_retain_mode;
        }
    }
}
```

### View

Add keyboard handler to emit `ToggleRetainMode` on 'R' key press.

### Retain View

Implement `retain_view()` returning a simple widget tree:

```rust
fn retain_view(state: &Self::State) -> Option<Box<dyn retain::Widget>> {
    if !state.use_retain_mode {
        return None;
    }

    Some(Box::new(
        retain::Background::new(
            Box::new(
                retain::Border::new(
                    Box::new(retain::Text::new("Retain Mode Active")),
                    Color::BLACK,
                    2.0,
                )
            ),
            Color::BLUE,
        )
    ))
}
```

### WindowState Integration

The existing `set_retain_mode()` method will be called when the flag changes. This requires plumbing the flag through to WindowState.

## Widget Tree

```
Background(Color::BLUE)
└── Border(Color::BLACK, 2.0)
    └── Text("Retain Mode Active")
```

This tests:
- **Background**: Fill color rendering
- **Border**: Stroke rendering
- **Text**: Glyphon text integration

## Success Criteria

1. Press 'R' toggles between immediate and retain mode
2. Retain mode shows blue rectangle with black border and text
3. Immediate mode continues working unchanged
4. No crashes or GPU errors
5. Mode state persists across frames

## Out of Scope

- CornerRadius (can be added later if needed)
- Event handling in retain-mode
- Complex layouts
- Performance testing
