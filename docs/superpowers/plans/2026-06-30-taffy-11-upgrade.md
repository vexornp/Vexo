# Taffy 0.11.0 Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade taffy from 0.9.1 to 0.11.0, fixing all breaking API changes so the project compiles and all tests pass.

**Architecture:** The upgrade is isolated to Vexo's taffy conversion layer. Vexo's own types (`Layout`, `JustifyContent`, `AlignItems`, etc.) remain unchanged — only the `to_taffy()` conversion impls and direct Taffy API calls are updated.

**Tech Stack:** Rust, taffy 0.11.0, existing Vexo codebase

## Global Constraints

- Upgrade taffy from 0.9.1 to 0.11.0 (minimal migration, no new features)
- All existing tests must pass after migration
- Vexo's own public API types (`Layout`, `JustifyContent`, `AlignItems`, etc.) must not change
- Commit each task independently with descriptive messages

---

### Task 1: Bump taffy dependency and verify build failure

**Files:**
- Modify: `Cargo.toml:34`

**Interfaces:**
- Consumes: Current taffy 0.9.1 API
- Produces: taffy 0.11.0 in Cargo.lock, triggering compile errors in subsequent tasks

- [ ] **Step 1: Update the workspace taffy version**

Change line 34 of `Cargo.toml` from `taffy = "0.9.1"` to `taffy = "0.11"`.

- [ ] **Step 2: Update Cargo.lock**

Run: `cargo update -p taffy`
Expected: taffy updated to 0.11.x in Cargo.lock

- [ ] **Step 3: Verify build fails (confirms API breakage)**

Run: `cargo build -p vexo 2>&1 | head -40`
Expected: Multiple compile errors from alignment types, position rename, etc. This confirms the version bump took effect and we know what to fix.

- [ ] **Step 4: Commit the dependency bump**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump taffy from 0.9.1 to 0.11"
```

---

### Task 2: Fix alignment conversion methods in style.rs

**Files:**
- Modify: `vexo/src/layout/style.rs:790-828` (JustifyContent, AlignItems, AlignContent `to_taffy()` impls)

**Interfaces:**
- Consumes: taffy 0.11.0 alignment types (associated constants)
- Produces: Working `JustifyContent::to_taffy()`, `AlignItems::to_taffy()`, `AlignContent::to_taffy()` conversions

- [ ] **Step 1: Update `JustifyContent::to_taffy()`**

Replace the impl at lines 790-802 with:

```rust
impl JustifyContent {
    fn to_taffy(self) -> taffy::prelude::JustifyContent {
        use taffy::prelude::JustifyContent as TaffyJustify;
        match self {
            JustifyContent::Start => TaffyJustify::START,
            JustifyContent::End => TaffyJustify::END,
            JustifyContent::Center => TaffyJustify::CENTER,
            JustifyContent::SpaceBetween => TaffyJustify::SPACE_BETWEEN,
            JustifyContent::SpaceAround => TaffyJustify::SPACE_AROUND,
            JustifyContent::SpaceEvenly => TaffyJustify::SPACE_EVENLY,
        }
    }
}
```

- [ ] **Step 2: Update `AlignItems::to_taffy()`**

Replace the impl at lines 804-815 with:

```rust
impl AlignItems {
    fn to_taffy(self) -> taffy::prelude::AlignItems {
        use taffy::prelude::AlignItems as TaffyAlign;
        match self {
            AlignItems::Stretch => TaffyAlign::STRETCH,
            AlignItems::Start => TaffyAlign::START,
            AlignItems::End => TaffyAlign::END,
            AlignItems::Center => TaffyAlign::CENTER,
            AlignItems::Baseline => TaffyAlign::BASELINE,
        }
    }
}
```

- [ ] **Step 3: Update `AlignContent::to_taffy()`**

Replace the impl at lines 817-829 with:

```rust
impl AlignContent {
    fn to_taffy(self) -> taffy::prelude::AlignContent {
        use taffy::prelude::AlignContent as TaffyAlign;
        match self {
            AlignContent::Start => TaffyAlign::START,
            AlignContent::End => TaffyAlign::END,
            AlignContent::Center => TaffyAlign::CENTER,
            AlignContent::Stretch => TaffyAlign::STRETCH,
            AlignContent::SpaceBetween => TaffyAlign::SPACE_BETWEEN,
            AlignContent::SpaceAround => TaffyAlign::SPACE_AROUND,
        }
    }
}
```

- [ ] **Step 4: Verify partial build**

Run: `cargo build -p vexo 2>&1 | head -20`
Expected: Fewer errors than before. Alignment-related errors should be resolved.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "fix: update alignment to_taffy() conversions for taffy 0.11"
```

---

### Task 3: Fix position rename and AlignSelf migration in style.rs

**Files:**
- Modify: `vexo/src/layout/style.rs:713-715` (position/inset rename in `to_taffy_style()`)
- Modify: `vexo/src/layout/style.rs:718` (align_self field)
- Modify: `vexo/src/layout/style.rs:831-849` (Position and Inset `to_taffy()` impls)
- Modify: `vexo/src/layout/style.rs:913-925` (AlignSelf `to_taffy()` impl)

**Interfaces:**
- Consumes: taffy 0.11.0 `Style` field names (`position_type`, `position` instead of `position`, `inset`)
- Produces: Working position conversion, AlignSelf conversion using `AlignItems` constants

- [ ] **Step 1: Fix position/inset field names in `to_taffy_style()`**

In the `to_taffy_style()` method (around lines 713-715), change:

```rust
// Before
position: self.position.map(|p| p.to_taffy()).unwrap_or_default(),
inset: self.inset.map(|i| i.to_taffy()).unwrap_or_else(Rect::auto),

// After
position_type: self.position.map(|p| p.to_taffy()).unwrap_or_default(),
position: self.inset.map(|i| i.to_taffy()).unwrap_or_else(Rect::auto),
```

- [ ] **Step 2: Fix `Position::to_taffy()` return type**

Replace the impl at lines 831-838 with:

```rust
impl Position {
    fn to_taffy(self) -> taffy::prelude::PositionType {
        match self {
            Position::Relative => taffy::prelude::PositionType::Relative,
            Position::Absolute => taffy::prelude::PositionType::Absolute,
        }
    }
}
```

- [ ] **Step 3: Fix `AlignSelf::to_taffy()` return type**

Replace the impl at lines 913-925 with:

```rust
impl AlignSelf {
    fn to_taffy(self) -> taffy::prelude::AlignItems {
        use taffy::prelude::AlignItems as TaffyAlign;
        match self {
            AlignSelf::Auto => TaffyAlign::STRETCH,
            AlignSelf::Start => TaffyAlign::START,
            AlignSelf::End => TaffyAlign::END,
            AlignSelf::Center => TaffyAlign::CENTER,
            AlignSelf::Stretch => TaffyAlign::STRETCH,
            AlignSelf::Baseline => TaffyAlign::BASELINE,
        }
    }
}
```

- [ ] **Step 4: Verify partial build**

Run: `cargo build -p vexo 2>&1 | head -20`
Expected: Fewer errors remaining.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "fix: update position and align_self conversions for taffy 0.11"
```

---

### Task 4: Fix taffy_engine.rs position rename

**Files:**
- Modify: `vexo/src/layout/taffy_engine.rs:175-180` (set_root_size method)

**Interfaces:**
- Consumes: taffy 0.11.0 `Style` struct field names
- Produces: Working `set_root_size()` method

- [ ] **Step 1: Fix the `taffy::Style` construction in `set_root_size()`**

In `set_root_size()` (around line 175), the `taffy::Style` struct literal uses field names from 0.9.1. Since it uses `..existing_style`, only the `size` field is explicitly set and no position/inset fields are referenced, so the struct literal itself should compile. However, verify that `taffy::style::LengthPercentage::percent(1.0).into()` still works — in taffy 0.11, `LengthPercentage` is a stricter type and the `.into()` conversion to `Dimension` may have changed.

Run: `cargo build -p vexo 2>&1 | grep -A3 "set_root_size\|LengthPercentage"`
Expected: If errors appear, adjust the `size` field construction. The `size` field type is `Size<Dimension>`, and `LengthPercentage::percent(1.0).into()` should still produce `Dimension::Percent(1.0)`. If the `.into()` conversion is no longer available, use `taffy::prelude::percent(1.0)` directly.

- [ ] **Step 2: Commit (if changes were needed)**

```bash
git add vexo/src/layout/taffy_engine.rs
git commit -m "fix: update set_root_size for taffy 0.11 Style changes"
```

---

### Task 5: Fix remaining compile errors and verify full build

**Files:**
- Modify: any remaining files with compile errors from the taffy API changes
- May include: `vexo/src/layout/style.rs` (LengthPercentage type changes), `vexo/src/layout/measurement.rs` (if AvailableSpace import breaks)

**Interfaces:**
- Consumes: taffy 0.11.0 full API
- Produces: Clean `cargo build -p vexo` and `cargo build -p desktop_demo`

- [ ] **Step 1: Build and collect remaining errors**

Run: `cargo build -p vexo 2>&1`
Expected: Some remaining errors from type mismatches (e.g., `LengthPercentage` vs `Dimension` in padding/margin/gap fields, `Rect::zero()`/`Rect::auto()` availability)

- [ ] **Step 2: Fix each remaining error**

Common fixes likely needed:

1. **`Rect::zero()` / `Rect::auto()`** — In taffy 0.11, these may have changed. If `Rect::zero()` doesn't exist for `Rect<LengthPercentage>`, construct manually:
   ```rust
   Rect { left: length(0.0), right: length(0.0), top: length(0.0), bottom: length(0.0) }
   ```

2. **`Size::zero()` for gap** — If `Size::zero()` doesn't exist for `Size<LengthPercentage>`, construct manually:
   ```rust
   Size { width: length(0.0), height: length(0.0) }
   ```

3. **`padding`/`margin` type change** — These fields are now `Rect<LengthPercentage>` instead of `Rect<LengthPercentageAuto>`. Since we only ever use `length()` values (never `auto()`) for padding/margin, this should work without changes. But if the compiler complains, verify the `length()` helper returns `LengthPercentage` (not `LengthPercentageAuto`).

4. **`align_self` type change** — The `Style.align_self` field is now `Option<AlignItems>`. Our code already wraps in `.map()`, which produces `Option<AlignItems>`, so this should be compatible.

- [ ] **Step 3: Verify full build**

Run: `cargo build -p vexo`
Expected: Clean build with no errors.

- [ ] **Step 4: Verify desktop demo builds**

Run: `cargo build -p desktop_demo`
Expected: Clean build with no errors.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "fix: resolve remaining taffy 0.11 compile errors"
```

---

### Task 6: Fix test assertions for taffy 0.11 API

**Files:**
- Modify: `vexo/src/layout/style.rs:1260-1272` (test_layout_to_taffy_style_flex_direction, test_layout_to_taffy_style_position)
- Modify: `vexo/src/layout/taffy_engine.rs` (tests if any reference Taffy types)

**Interfaces:**
- Consumes: taffy 0.11.0 alignment associated constants, PositionType rename
- Produces: All tests passing

- [ ] **Step 1: Run tests to find failures**

Run: `cargo test -p vexo 2>&1 | grep -E "FAILED|test result"`
Expected: Some test failures from assertions using old Taffy enum variants or old field names.

- [ ] **Step 2: Fix `test_layout_to_taffy_style_flex_direction`**

At line 1264, this test asserts:
```rust
assert_eq!(style.flex_direction, taffy::prelude::FlexDirection::Column);
```
`FlexDirection` is still an enum in 0.11, so this assertion should still work. Verify.

- [ ] **Step 3: Fix `test_layout_to_taffy_style_position`**

At line 1271, this test asserts:
```rust
assert_eq!(style.position, taffy::prelude::Position::Absolute);
```
This needs to change to:
```rust
assert_eq!(style.position_type, taffy::prelude::PositionType::Absolute);
```

- [ ] **Step 4: Run tests and fix any remaining assertion failures**

Run: `cargo test -p vexo 2>&1`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/layout/style.rs vexo/src/layout/taffy_engine.rs
git commit -m "fix: update test assertions for taffy 0.11 API"
```

---

### Task 7: Final verification

**Files:**
- No modifications — verification only

**Interfaces:**
- Consumes: Fully migrated codebase
- Produces: Confirmed working upgrade

- [ ] **Step 1: Clean build all targets**

Run: `cargo build -p vexo && cargo build -p desktop_demo && cargo build -p shared_app`
Expected: All targets compile cleanly.

- [ ] **Step 2: Run full test suite**

Run: `cargo test -p vexo`
Expected: All tests pass, no failures.

- [ ] **Step 3: Verify no taffy 0.9.x remnants in code**

Run: `grep -rn "taffy::prelude::Position::\|taffy::prelude::AlignItems::\|taffy::prelude::AlignContent::\|taffy::prelude::JustifyContent::\|taffy::prelude::AlignSelf::" vexo/src/`
Expected: No results (all old enum-variant syntax replaced with associated constants or PositionType)

- [ ] **Step 4: Verify Cargo.lock has taffy 0.11.x**

Run: `grep -A2 'name = "taffy"' Cargo.lock`
Expected: Version shows 0.11.x
