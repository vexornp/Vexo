# Retain Mode Widget Demo Design

**Date:** 2026-04-30

## Context

The retain mode demo in `shared_app/src/lib.rs` currently shows only 3 buttons in a column. This design expands it to demonstrate all 7 retain mode widgets (Button, Text, Column, Row, Background, Border, CornerRadius) with interactive event handling.

## Architecture

```
retain::Column (main container)
├── Header Text ("Retain Mode Widget Demo")
├── Button Section (Row)
│   ├── Button "Increment (+)" → RetainIncrement
│   ├── Button "Decrement (-)" → RetainDecrement
│   └── Button "Reset" → RetainReset
├── Counter Display (Text: "Count: X")
├── Styled Widgets Section (Column)
│   ├── Background(Color::BLUE) + Button
│   ├── Border(Color::RED, 2.0) + Text
│   └── CornerRadius(10.0) + Button
└── Container Demo (Row)
    ├── Left Column with widgets
    └── Right Column with widgets
```

## State Changes

**Add to State struct:**
```rust
retain_counter: u32,
```

**Add to Message enum:**
```rust
RetainIncrement,
RetainDecrement,
RetainReset,
```

**Add to update() method:**
```rust
Message::RetainIncrement => state.retain_counter += 1,
Message::RetainDecrement => state.retain_counter = state.retain_counter.saturating_sub(1),
Message::RetainReset => state.retain_counter = 0,
```

## Widget Demonstrations

| Widget | Demo Purpose |
|--------|--------------|
| Button | Click handling, message emission |
| Text | Reactive state display (counter) |
| Column | Vertical layout container |
| Row | Horizontal button groups |
| Background | Colored background behind button |
| Border | Border around text |
| CornerRadius | Rounded corners on button |

## Files to Modify

- `shared_app/src/lib.rs`
  - Add `retain_counter` to `State` struct
  - Add message variants to `Message` enum
  - Add handlers to `update()` method
  - Rewrite `retain_view()` method

## Verification

1. Run `cargo run -p desktop_demo`
2. Toggle to retain mode using existing toggle button
3. Click Increment/Decrement/Reset buttons
4. Verify counter updates correctly
5. Verify all styled widgets render with proper modifiers
