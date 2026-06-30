# Taffy 0.11.0 Upgrade Design

## Summary

Upgrade the taffy layout dependency from 0.9.1 to 0.11.0, fixing all breaking API changes. This is a minimal migration — no new features (direction, float, CSS parsing) are adopted yet.

## Motivation

- Bug fixes: grid item percentage resolution, flexbox sizing under min-content constraints, aspect ratio correctness, absolute positioning margin handling, inset application for relatively positioned flexbox children, gap style cross-contamination
- Safe alignment keywords (CSS spec compliance)
- Type safety improvements (LengthPercentage vs LengthPercentageAuto vs Dimension)
- Position property names now match CSS spec

## Scope

- Bump taffy dependency
- Fix all compile errors from breaking API changes
- Update conversion layer (`to_taffy()` methods) to match new Taffy API
- Update tests to match new API
- Do NOT add: direction/RTL support, float/clear support, CSS string parsing

## Breaking Changes & Migration Plan

### 1. Dependency bump

File: `Cargo.toml` (workspace root)

```
taffy = "0.9.1"  →  taffy = "0.11"
```

### 2. Alignment types: enum variants → associated constants

**Files:** `vexo/src/layout/style.rs`

Taffy 0.11.0 changed alignment types from enums to structs with associated constants. The naming convention changed from `PascalCase` variants to `SCREAMING_SNAKE_CASE` constants.

**JustifyContent mapping:**

| Vexo | Taffy 0.9.1 | Taffy 0.11.0 |
|------|-------------|--------------|
| `Start` | `JustifyContent::Start` | `JustifyContent::START` |
| `End` | `JustifyContent::End` | `JustifyContent::END` |
| `Center` | `JustifyContent::Center` | `JustifyContent::CENTER` |
| `SpaceBetween` | `JustifyContent::SpaceBetween` | `JustifyContent::SPACE_BETWEEN` |
| `SpaceAround` | `JustifyContent::SpaceAround` | `JustifyContent::SPACE_AROUND` |
| `SpaceEvenly` | `JustifyContent::SpaceEvenly` | `JustifyContent::SPACE_EVENLY` |

**AlignItems mapping:**

| Vexo | Taffy 0.9.1 | Taffy 0.11.0 |
|------|-------------|--------------|
| `Stretch` | `AlignItems::Stretch` | `AlignItems::STRETCH` |
| `Start` | `AlignItems::Start` | `AlignItems::START` |
| `End` | `AlignItems::End` | `AlignItems::END` |
| `Center` | `AlignItems::Center` | `AlignItems::CENTER` |
| `Baseline` | `AlignItems::Baseline` | `AlignItems::BASELINE` |

**AlignContent mapping:**

| Vexo | Taffy 0.9.1 | Taffy 0.11.0 |
|------|-------------|--------------|
| `Start` | `AlignContent::Start` | `AlignContent::START` |
| `End` | `AlignContent::End` | `AlignContent::END` |
| `Center` | `AlignContent::Center` | `AlignContent::CENTER` |
| `Stretch` | `AlignContent::Stretch` | `AlignContent::STRETCH` |
| `SpaceBetween` | `AlignContent::SpaceBetween` | `AlignContent::SPACE_BETWEEN` |
| `SpaceAround` | `AlignContent::SpaceAround` | `AlignContent::SPACE_AROUND` |

**JustifyContent and AlignContent merge:** `JustifyContent` is now an alias of `AlignContent`. The `Stretch` variant is now available on `JustifyContent` (ignored for Flexbox, valid for Grid). New variants `FLEX_START` and `FLEX_END` are also available.

**AlignItems and AlignSelf merge:** `AlignSelf` is now an alias of `AlignItems`. The `Auto` variant is removed — use `Option::None` on the `Style.align_self` field instead.

**Default impls removed:** Alignment types no longer implement `Default`. The `Style` struct still provides defaults, so this is handled by `to_taffy_style()`.

### 3. Position property rename

**Files:** `vexo/src/layout/style.rs`, `vexo/src/layout/taffy_engine.rs`

In Taffy's `Style` struct:

| Taffy 0.9.1 | Taffy 0.11.0 |
|-------------|--------------|
| `Style::position` (Position enum) | `Style::position_type` (PositionType enum) |
| `Style::inset` (Rect<LengthPercentageAuto>) | `Style::position` (Rect<LengthPercentageAuto>) |

The enum `taffy::prelude::Position` is renamed to `taffy::prelude::PositionType`.

In `to_taffy_style()`:
```rust
// Before
position: self.position.map(|p| p.to_taffy()).unwrap_or_default(),
inset: self.inset.map(|i| i.to_taffy()).unwrap_or_else(Rect::auto),

// After
position_type: self.position.map(|p| p.to_taffy()).unwrap_or_default(),
position: self.inset.map(|i| i.to_taffy()).unwrap_or_else(Rect::auto),
```

### 4. AlignSelf migration

**Files:** `vexo/src/layout/style.rs`

`AlignSelf::Auto` is removed. The `Style.align_self` field is now `Option<AlignItems>`.

In `to_taffy_style()`:
```rust
// Before
align_self: self.align_self.map(|a| a.to_taffy()),

// After
align_self: self.align_self.map(|a| a.to_taffy()),
```

Vexo's `Layout.align_self` is `Option<AlignSelf>`. When it's `None`, Taffy gets `None` (which means "auto" in 0.11). When it's `Some(AlignSelf::Auto)`, we also map to `None`. When it's `Some(AlignSelf::Start)`, etc., we call `to_taffy()` which returns an `AlignItems` constant.

The `to_taffy()` impl for `AlignSelf` changes from returning `AlignSelf` variants to returning `AlignItems` constants. The `Auto` variant is never actually reached in practice (it's filtered to `None` before calling `to_taffy()`), but must still be handled in the match for exhaustiveness:
```rust
// Before
AlignSelf::Auto => TaffyAlign::Stretch,
AlignSelf::Start => TaffyAlign::Start,

// After
AlignSelf::Auto => TaffyAlign::STRETCH,  // unreachable in practice; Auto is filtered to None before to_taffy()
AlignSelf::Start => TaffyAlign::START,
```

### 5. LengthPercentage / LengthPercentageAuto strictness

**Files:** `vexo/src/layout/style.rs`

Some `Style` fields now use stricter types:
- `padding`, `margin`: `Rect<LengthPercentage>` (no `Auto` variant — padding/margin can't be auto)
- `position` (was `inset`): `Rect<LengthPercentageAuto>`
- `gap`: `Size<LengthPercentage>`

The helper functions `length()`, `percent()`, `auto()` still work and return the appropriate types. The conversion code should work with minimal changes since we already use these helpers.

### 6. AvailableSpace module move

`AvailableSpace` moved from `layout` module to `style` module. Since Vexo imports via `taffy::prelude::AvailableSpace`, no change needed.

### 7. Test updates

**Files:** `vexo/src/layout/style.rs` (tests), `vexo/src/layout/taffy_engine.rs` (tests)

Tests that assert against Taffy enum variants need updating:
```rust
// Before
assert_eq!(style.flex_direction, taffy::prelude::FlexDirection::Column);
assert_eq!(style.position, taffy::prelude::Position::Absolute);

// After
assert_eq!(style.flex_direction, taffy::prelude::FlexDirection::Column);
assert_eq!(style.position_type, taffy::prelude::PositionType::Absolute);
```

Alignment assertions need associated constant syntax:
```rust
// Before
assert_eq!(style.justify_content, Some(taffy::prelude::JustifyContent::Start));

// After
assert_eq!(style.justify_content, Some(taffy::prelude::JustifyContent::START));
```

## Files Affected

| File | Changes |
|------|---------|
| `Cargo.toml` | Bump taffy version |
| `vexo/src/layout/style.rs` | Alignment conversions, position rename, AlignSelf migration, LengthPercentage types, test updates |
| `vexo/src/layout/taffy_engine.rs` | Position rename in `set_root_size()`, test updates |
| `vexo/src/layout/measurement.rs` | Verify AvailableSpace import (likely no change via prelude) |

## Verification

1. `cargo build -p vexo` — compiles without errors
2. `cargo test -p vexo` — all tests pass
3. `cargo build -p desktop_demo` — desktop demo compiles
