# Retain Mode Widget Demo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand the retain mode demo to showcase all 7 retain mode widgets with interactive event handling.

**Architecture:** Single Column-based layout with button controls, counter display, and styled widget demonstrations. Uses nested Rows for horizontal button groups and visual modifiers for styling.

**Tech Stack:** Rust, vexo retain mode widgets

---

## Important Constraint

**Background, Border, and CornerRadius modifiers use `Widget<()>`** (unit message type). They cannot directly wrap widgets with custom message types like `Button<Message>`. The demo will show these modifiers with `Text` widgets (which can be `Text<()>`).

---

### Task 1: Add State and Message Types

**Files:**
- Modify: `shared_app/src/lib.rs:6-12` (Message enum)
- Modify: `shared_app/src/lib.rs:87-90` (State struct)

- [ ] **Step 1: Add retain counter to State struct**

Add `retain_counter` field to the `State` struct at line 87:

```rust
pub struct State {
    click_count: u32,
    milestones: u32,
    retain_counter: u32,
}
```

- [ ] **Step 2: Initialize retain_counter in State::new()**

Update the `new()` method at line 96:

```rust
fn new() -> Self::State {
    Self {
        click_count: 0,
        milestones: 0,
        retain_counter: 0,
    }
}
```

- [ ] **Step 3: Add new message variants**

Add three new message variants to the `Message` enum at line 6:

```rust
#[derive(Debug, Clone)]
pub enum Message {
    None,
    Clicked,
    CounterOutput(CounterOutput),
    ToggleRetainMode,
    RetainButtonClicked,
    RetainIncrement,
    RetainDecrement,
    RetainReset,
}
```

- [ ] **Step 4: Add message handlers to update()**

Add handlers for the new messages in the `update()` method at line 103:

```rust
fn update(state: &mut Self::State, message: Self::Message) {
    match message {
        Message::Clicked => {
            state.click_count += 1;
        }
        Message::RetainButtonClicked => {
            state.click_count += 1;
        }
        Message::RetainIncrement => {
            state.retain_counter += 1;
        }
        Message::RetainDecrement => {
            state.retain_counter = state.retain_counter.saturating_sub(1);
        }
        Message::RetainReset => {
            state.retain_counter = 0;
        }
        Message::CounterOutput(CounterOutput::CountReached(_n)) => {
            state.milestones += 1;
        }
        Message::None => {}
        Message::ToggleRetainMode => {
            // This message is handled by WindowState, not the app state
            // The retain mode toggle is a framework-level concern
        }
    }
}
```

- [ ] **Step 5: Build to verify compilation**

Run: `cargo build -p shared_app`
Expected: Build succeeds without errors

- [ ] **Step 6: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat: add retain_counter state and message types for retain mode demo"
```

---

### Task 2: Implement retain_view() Widget Tree

**Files:**
- Modify: `shared_app/src/lib.rs:180-188` (retain_view method)

- [ ] **Step 1: Rewrite retain_view() with comprehensive widget demo**

Replace the entire `retain_view()` method (lines 180-188) with:

```rust
fn retain_view(state: &Self::State) -> Option<Box<dyn retain::Widget<Self::Message>>> {
    let counter_text = format!("Count: {}", state.retain_counter);

    Some(Box::new(
        retain::Column::new()
            // Header
            .push(retain::Text::new("Retain Mode Widget Demo"))
            // Button controls in a Row
            .push(
                retain::Row::new()
                    .push(retain::Button::new("Increment (+)")
                        .with_message(Message::RetainIncrement))
                    .push(retain::Button::new("Decrement (-)")
                        .with_message(Message::RetainDecrement))
                    .push(retain::Button::new("Reset")
                        .with_message(Message::RetainReset))
            )
            // Counter display
            .push(retain::Text::new(counter_text))
            // Styled widgets section
            .push(retain::Text::new("─── Styled Widgets ───"))
            // Background modifier with Text
            .push(
                retain::Background::new(
                    Box::new(retain::Text::new("Background + Text")),
                    vexo::Color::rgb(0.85, 0.9, 1.0)
                )
            )
            // Border modifier with Text
            .push(
                retain::Border::new(
                    Box::new(retain::Text::new("Border + Text")),
                    vexo::Color::rgb(0.8, 0.2, 0.2),
                    2.0
                )
            )
            // CornerRadius modifier with Text
            .push(
                retain::CornerRadius::new(
                    Box::new(retain::Text::new("CornerRadius + Text")),
                    10.0
                )
            )
            // Container demo: Row with two Columns
            .push(retain::Text::new("─── Container Layout ───"))
            .push(
                retain::Row::new()
                    .push(
                        retain::Column::new()
                            .push(retain::Text::new("Left Column"))
                            .push(retain::Button::new("Button L")
                                .with_message(Message::RetainIncrement))
                    )
                    .push(
                        retain::Column::new()
                            .push(retain::Text::new("Right Column"))
                            .push(retain::Button::new("Button R")
                                .with_message(Message::RetainDecrement))
                    )
            )
    ))
}
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p shared_app`
Expected: Build succeeds without errors

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat: implement comprehensive retain mode widget demo"
```

---

### Task 3: Verify End-to-End

**Files:**
- None (verification only)

- [ ] **Step 1: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Application launches successfully

- [ ] **Step 2: Toggle to retain mode**

Press `R` key to toggle to retain mode.
Expected: Retain mode demo appears with:
- Header text "Retain Mode Widget Demo"
- Three buttons: Increment, Decrement, Reset
- Counter display
- Styled widgets section with Background, Border, CornerRadius
- Container layout with nested Row/Column

- [ ] **Step 3: Test button interactions**

Click "Increment (+)" button multiple times.
Expected: Counter increases

Click "Decrement (-)" button.
Expected: Counter decreases

Click "Reset" button.
Expected: Counter resets to 0

- [ ] **Step 4: Verify styled widgets render correctly**

Visual check:
- Background + Text: Light blue background behind text
- Border + Text: Red border around text
- CornerRadius + Text: Text with rounded corners applied

- [ ] **Step 5: Final commit (if any fixes needed)**

If any issues were fixed:

```bash
git add shared_app/src/lib.rs
git commit -m "fix: resolve retain mode demo issues"
```
