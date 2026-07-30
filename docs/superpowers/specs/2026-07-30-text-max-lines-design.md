# Text Max Lines Design

**Date:** 2026-07-30

## Context

The `Text` widget renders content with no limit on the number of lines displayed. When text wraps (due to a width constraint) or contains explicit `\n` newlines, the widget grows vertically to fit all lines. There is no way to cap the visible line count and indicate truncation to the user.

This is needed for common UI patterns: card titles, list-item subtitles, chat message previews — anywhere a fixed-height text region must show a truncated snippet with an ellipsis.

## Goal

Add a `max_lines` option to the `Text` widget that caps visible lines and appends `…` when the content is truncated. Both width-wrapped lines and explicit `\n` newlines count toward the limit. `TextEdit` is out of scope (ellipsis on an editable field is YAGNI).

## Non-Goals

- `TextOverflow` configurability (clip vs. ellipsis). Ellipsis-only for now; a knob can be added later if needed.
- Truncation of `TextEdit` content.
- Word-boundary-aware truncation. We work at the glyph level (CSS `text-overflow: ellipsis` semantics), which is RTL/CJK safe.
- Changes to the render command, text request, text cache, or text processor layers. Truncation is resolved before those layers see the content.

## Design

### Approach

A new module `vexo/src/text_overflow.rs` owns the ellipsis algorithm as one pure, GPU-free, unit-testable function. The measurement layer caps the returned height (so Taffy allocates the right box); `TextRenderObject::apply_layout` — the only place that knows the final wrap width — calls the function to compute the truncated string, which `paint` then emits. Downstream layers render the pre-truncated string unchanged.

This isolates the trickiest logic (ellipsis fitting) in a testable unit, avoids glyphon-internal glyph surgery, and keeps the cache/command/request layers thin.

### Data Flow

1. `Text` widget carries `max_lines: Option<u32>` → `TextRenderObject` via `create_render_object` / `update_render_object` (change flags `LAYOUT`).
2. `TextRenderObject::layout()` puts `max_lines` into `TextMeasureContext`.
3. `measure_text_node()` caps the returned **height** to `max_lines * font_size * line_height` so Taffy allocates a box of the truncated size.
4. `TextRenderObject::apply_layout()` (which knows the final box width) calls `truncate_with_ellipsis(...)` when `max_lines.is_some()`, and stores the resulting truncated string + line count on the RO. `measured_text_height` uses the truncated height.
5. `TextRenderObject::paint()` emits the **truncated** string instead of `self.content`; vertical centering uses the truncated height. The `natural_text_width` tolerance logic is skipped when truncating (the truncated string is already width-fitted).
6. Downstream: `RenderCommand::Text{content: truncated}` → `TextRequest` → `text_cache` (cache miss first frame, then cached) → `text_processor` → glyphon `TextArea`. **No changes below paint.**

### Key Invariant

Ellipsis is computed **once** at `apply_layout` time, and the pre-truncated string rides the existing pipeline. The measurement layer caps height; `apply_layout` computes content; the downstream pipeline renders a plain string.

## The Ellipsis Algorithm

### Signature

```rust
/// Result of truncating text to fit within `max_lines`.
pub struct TruncationResult {
    /// Content after truncation. Ends with `…` iff truncation occurred.
    pub content: String,
    /// Number of visible lines in the truncated content.
    pub line_count: usize,
    /// Height of the truncated content in logical pixels
    /// (`line_count * font_size * line_height`).
    pub height: f32,
}

/// Truncate `content` to fit within `max_lines` lines at `max_width`,
/// appending `…` if truncation occurred.
///
/// - `max_width`: wrap width in logical pixels. `None` means unbounded
///   (only `\n` breaks lines).
/// - `max_lines`: maximum number of visible lines. Must be ≥ 1.
pub fn truncate_with_ellipsis(
    content: &str,
    font_size: f32,
    line_height: f32,
    font_family: Option<&str>,
    max_width: Option<f32>,
    max_lines: u32,
    font_system: &mut glyphon::FontSystem,
) -> TruncationResult;
```

### Algorithm

1. **Shape once** at `max_width` to get the natural layout runs. Count visible lines.
2. **Fast path — fits:** If `line_count <= max_lines`, return content unchanged, `line_count`, and `height = line_count * font_size * line_height`. No ellipsis. (Covers all non-truncated text.)
3. **Truncation path:**
   - The last kept line is line `max_lines` (1-indexed). Take the first `max_lines - 1` lines verbatim.
   - For the final line, we have its glyph runs from the initial shape (step 1). Shape the single character `"…"` once to get its width. Then walk back from the end of the final line's glyphs, accumulating removed-glyph width, until `final_line_width - removed_width + ellipsis_width <= max_width`. This arithmetic on already-shaped glyph widths avoids re-shaping per iteration.
   - Output = first `max_lines - 1` lines + fitted final line + `"…"`.
   - **Correctness invariant:** the output string, when shaped at `max_width`, must produce at most `max_lines` lines and end with `…`. A final verify-shape of the output string confirms this; if (in a rare edge case) it wraps to `max_lines + 1`, fall back to removing one more glyph from the final line and re-verify.
4. **Edge cases:**
   - `max_lines == 0` → treated as `1` (the widget builder clamps to ≥1, but the function defends anyway).
   - `max_width` is `None` → only `\n` breaks lines; truncate at line `max_lines`, append `…`.
   - `max_width` too narrow for even `…` alone → return just `"…"`. We never produce an empty truncation; a single `…` may visually overflow but is always more useful than nothing.
   - Empty content → return empty content unchanged (no ellipsis); height matches the existing empty-text measurement (one line height, `font_size * line_height`), consistent with `TextMeasurer`'s empty-text behavior.

### Why Glyph-Width Arithmetic (Not Re-Shaping Per Iteration)

Appending `…` can itself push the line over width (especially when the kept glyph was near the edge), and in rare cases trigger a re-wrap to a new line. The naive approach — re-shaping `candidate + "…"` per removed glyph — is O(N) shapes and expensive. Instead we read glyph widths from the already-shaped layout runs of the final line and shape `"…"` once, then do O(N) arithmetic to find the cutoff. A single verify-shape of the final output string confirms the invariant; a rare wrap-overflow triggers at most one extra removal + re-verify.

### Caching

`TruncationResult` is memoized via the existing `MeasureCache` (in `layout/measurement.rs`). The `MeasureCacheKey` gains a `max_lines_bits: Option<u32>` field so capped measurements cache distinctly from uncapped. Shaping is the expensive part; per truncation we do 1 initial shape + 1 ellipsis-width shape + 1 verify shape = 3 shapes, all cached. Subsequent frames with the same inputs are cache hits.

## Widget & Render Object API

### `Text` widget (`vexo/src/widgets/text.rs`)

One optional field, mirroring the existing `with_font_family` pattern:

```rust
pub struct Text {
    key: Option<WidgetKey>,
    content: String,
    font_size: f32,
    color: Color,
    font_family: Option<String>,
    max_lines: Option<u32>,   // NEW — None = unlimited (current behavior)
}

impl Text {
    /// Set the maximum number of visible lines. Truncation appends `…`.
    /// Values < 1 are clamped to 1.
    pub fn with_max_lines(mut self, max_lines: u32) -> Self {
        self.max_lines = Some(max_lines.max(1));
        self
    }

    pub fn max_lines(&self) -> Option<u32> { self.max_lines }
}
```

`Clone`, `create_render_object`, and `update_render_object` carry the field through. `update_render_object` flags `LAYOUT` when `max_lines` changes (the box height depends on it).

### `TextRenderObject` (`vexo/src/render_objects/text.rs`)

```rust
pub struct TextRenderObject {
    // ...existing fields...
    max_lines: Option<u32>,              // NEW
    /// Content after ellipsis truncation, computed in apply_layout.
    /// None until apply_layout runs, or when max_lines is None.
    /// paint falls back to self.content when None.
    truncated_content: Option<String>,  // NEW
    /// Line count of the truncated content, for height/centering.
    truncated_line_count: Option<usize>,// NEW
}
```

- `with_max_lines` builder + `set_max_lines` setter (returns `bool` for change detection).
- `layout()` puts `max_lines` into `TextMeasureContext`.
- `apply_layout()` calls `truncate_with_ellipsis(...)` only when `max_lines.is_some()`; stores `truncated_content` + `truncated_line_count`. `measured_text_height` uses the truncated height. When `max_lines.is_none()`, behavior is unchanged.
- `paint()` emits `truncated_content.as_ref().unwrap_or(&self.content)`; centering uses the truncated height. The `natural_text_width` tolerance check is skipped when truncating (truncated content is already width-fitted, so no spurious-wrap guard is needed).

### `TextMeasureContext` (`vexo/src/layout/measurement.rs`)

```rust
pub struct TextMeasureContext {
    pub content: String,
    pub font_size: f32,
    pub line_height: f32,
    pub font_family: Option<String>,
    pub max_lines: Option<u32>,   // NEW
}
```

`measure_text_node()` uses `max_lines` to cap the returned height:

- When `max_lines = Some(n)` and the measured line count exceeds `n`, return `height = n * font_size * line_height` instead of the natural wrapped height. Width is unaffected.
- When `max_lines = None`, behavior is unchanged.

`MeasureCacheKey` gains `max_lines_bits: Option<u32>` so capped measurements cache distinctly.

### Layers Unchanged

- `RenderCommand::Text` — still a plain `content: String`. The truncated string is emitted here.
- `TextRequest` — unchanged.
- `FrameBuilder::add_text` — unchanged.
- `text_cache.rs` — unchanged; caches by content string, and the truncated string is just another string.
- `text_processor.rs` — unchanged.
- `TextEditRenderObject` — builds its own `TextMeasureContext` with `max_lines: None`; no behavior change.

## Backward Compatibility

`max_lines` defaults to `None` on `Text`, `TextRenderObject`, and `TextMeasureContext`. Existing `Text::new()` calls behave identically. All existing tests pass without modification.

## Files to Modify

| File | Change |
|------|--------|
| `vexo/src/text_overflow.rs` | NEW: `TruncationResult`, `truncate_with_ellipsis`, edge-case handling, unit tests |
| `vexo/src/lib.rs` | Add `mod text_overflow;` and public export |
| `vexo/src/widgets/text.rs` | Add `max_lines` field, `with_max_lines` builder, `max_lines()` getter; thread through `Clone`, `create_render_object`, `update_render_object` (LAYOUT flag on change) |
| `vexo/src/render_objects/text.rs` | Add `max_lines`, `truncated_content`, `truncated_line_count` fields; `with_max_lines`/`set_max_lines`; call `truncate_with_ellipsis` in `apply_layout`; emit truncated content in `paint` |
| `vexo/src/layout/measurement.rs` | Add `max_lines` to `TextMeasureContext`; cap height in `measure_text_node`; add `max_lines_bits` to `MeasureCacheKey` |
| `vexo/src/render_objects/text_edit.rs` | Set `max_lines: None` when constructing `TextMeasureContext` (one-line change) |
| `vexo/src/layout/taffy_engine.rs` | Update the three test sites that construct `TextMeasureContext` to include `max_lines: None` |

## Testing

### `text_overflow.rs` unit tests (pure, GPU-free)

- No truncation when content fits within `max_lines`.
- Exact-fit (line count == max_lines) → no ellipsis.
- Truncation with ellipsis appended on the last kept line.
- Mid-word ellipsis (English text, no space before cutoff).
- CJK text (no spaces) truncates at glyph level.
- Explicit `\n` newlines count toward the limit.
- `max_width = None` truncates at `\n` boundaries only.
- `max_lines = 1` single-line ellipsis (most common case).
- `max_lines = 0` defended to `1`.
- Empty content → empty result.
- `max_width` too narrow for `…` alone → returns `"…"`.

### `render_objects/text.rs` tests

- `apply_layout` with `max_lines` set produces a `truncated_content` ending in `…` when content overflows.
- `paint` emits the truncated content (not the original).
- `max_lines = None` leaves `truncated_content` as `None` and `paint` emits `self.content` (regression guard).
- `set_max_lines` change detection returns `true`/`false` correctly.

### `widgets/text.rs` tests

- `with_max_lines` stores the value; values < 1 clamp to 1.
- `max_lines()` getter.
- `Clone` preserves `max_lines`.
- `update_render_object` flags `LAYOUT` when `max_lines` changes, no flag when unchanged.

### `layout/measurement.rs` tests

- `measure_text_node` caps height when `max_lines` is set and content overflows.
- `measure_text_node` returns natural height when content fits within `max_lines`.
- `MeasureCacheKey` with different `max_lines` produces distinct cache entries.

### Integration

- A `Text` widget with `with_max_lines(2)` in a width-constrained container renders exactly 2 lines + `…` when content overflows, verified via `MockBackend` command inspection (following the `stateful_integration_test.rs` pattern of matching on `RenderCommand::Text { content, .. }`).

## Verification

1. `cargo build` compiles without errors.
2. `cargo test -p vexo` passes all existing + new tests.
3. `cargo run -p desktop_demo` (run by user) renders truncated text correctly in a demo screen.
