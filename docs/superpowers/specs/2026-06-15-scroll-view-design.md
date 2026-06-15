# ScrollView Design Spec

## Problem

Vexo lacks a scrollable container. Content that overflows its parent is either clipped or visually broken. Users cannot scroll to see off-screen content.

## Requirements

- Vertical-only scrolling
- No visible scrollbar (scroll via input events only)
- Input methods: mouse wheel/trackpad, keyboard (arrows, PageUp/Down, Home/End)
- Immediate/clamped physics (no momentum, no bouncing, no overscroll)
- ScrollView as a dedicated widget with modifier methods
- Internal scroll offset state (no external ScrollController)
- Demo in shared_app

## Approach

Dedicated `ScrollViewRenderObject` that owns scroll state. Infrastructure changes to the Painter (offset emission), EventHandler (Scroll event dispatch), and RenderObject trait (`scroll_offset()` method).

## Widget & Element

### ScrollView Widget (`vexo/src/widgets/scroll_view.rs`)

```rust
pub struct ScrollView {
    child: Box<dyn Widget>,
    key: Option<WidgetKey>,
}

impl ScrollView {
    pub fn new(child: impl Widget) -> Self { ... }
}
```

- Holds one child
- No Style/Layout fields — viewport decoration is achieved by wrapping in `Flex` with `.background()` etc.
- Widget trait modifier methods work after boxing via trait defaults

### ScrollViewElement (`vexo/src/elements/scroll_view.rs`)

- Single child (like `DecoratedContainerElement`)
- Owns scroll state:
  - `scroll_offset: f32` — current vertical scroll offset in logical pixels
  - `content_height: f32` — total content height from child layout
  - `viewport_height: f32` — ScrollView's own height
  - `max_scroll: f32` — `(content_height - viewport_height).max(0.0)`
- On `mount()`: mounts child, creates ScrollViewRenderObject
- On `rebuild()`: uses `update_child()` to reconcile child
- On `on_event()`: handles scroll and keyboard events (see Event Handling section)
- Does NOT pass scroll-consumed events to children; passes non-scroll events through
- Wraps its child in a `Focus` widget (via the widget's `build()` method) so the ScrollView can receive keyboard events when focused

## Render Object & Painter Integration

### ScrollViewRenderObject (`vexo/src/render_objects/scroll_view.rs`)

Fields:
- `child: Option<RenderObjectKey>`
- `scroll_offset: f32` — synced from ScrollViewElement (the element owns the canonical state)
- `content_size: Size<Logical>`
- `viewport_size: Size<Logical>`
- `computed_bounds: Option<Bounds<Logical>>`
- `layout_node: Option<LayoutNodeKey>`

Layout:
- Creates a Taffy container node with `overflow_y: Overflow::Scroll`
- This tells Taffy: content can overflow vertically, node intrinsic height not influenced by overflow content, content beyond viewport is still laid out
- After `compute()`, reads own bounds (viewport) and child's bounds (content size)
- `max_scroll = (content_height - viewport_height).max(0.0)`

Paint:
- Emits decoration commands if Style is present (background, border, corner radius)
- Does NOT emit `PushOffset` directly — the Painter handles that via `scroll_offset()`

### New RenderObject trait method

```rust
fn scroll_offset(&self) -> Option<Point<Logical>> { None }
```

`ScrollViewRenderObject` overrides to return `Some(Point::new(0.0, -self.scroll_offset))`.

### Painter changes (`painter.rs`)

In `paint_recursive()`, after checking `clip_bounds()` and before painting children:
1. Check `obj.scroll_offset()` — if `Some(offset)`, emit `PushOffset { offset }`
2. Paint children with position adjusted by the offset
3. Emit `PopOffset`

### Clip + Offset interaction

For ScrollView, `clip_bounds()` always returns the viewport bounds. The Painter emits:
1. `PushClip` (viewport)
2. `PushOffset` (scroll translation)
3. Children
4. `PopOffset`
5. `PopClip`

This ensures children are clipped to the viewport and visually offset by the scroll position.

## Event Handling & Hit Testing

### Scroll event dispatch (`event_handler.rs`)

Currently `InputEvent::Scroll` is dropped. Add `handle_scroll_event()`:
1. Run hit test at the pointer position (from most recent `PointerMoved`)
2. Walk the hit path from deepest to root
3. Find the first render object that returns a non-`None` `scroll_offset()` — that's the ScrollView
4. Dispatch the `Scroll` event to the corresponding element
5. If no ScrollView in the path, drop the event

### Keyboard scrolling

ScrollViewElement handles keyboard events when focused:
- `ArrowUp` / `ArrowDown`: scroll by ~40px (approximate line height)
- `PageUp` / `PageDown`: scroll by viewport height
- `Home` / `End`: scroll to top / bottom

### Hit testing (`hit_test.rs`)

`ScrollViewRenderObject` does NOT override `hit_test_transform()`. The `scroll_offset()` method
handles child pointer adjustment during hit testing — it shifts the child pointer position by the
scroll offset so children are tested at the correct content-space coordinates. Using
`hit_test_transform()` would break the `is_inside` check on the ScrollView itself by shifting the
local pointer position outside the viewport bounds.

## Layout Integration

ScrollView sets `overflow_y: Overflow::Scroll` on its layout node. After layout computation, `ScrollViewRenderObject.layout()` reads:
- `self.computed_bounds` → viewport size
- Child's `computed_bounds` → content size (may be taller than viewport)

When child rebuilds change content size, the next layout + paint cycle recomputes content size. If offset now exceeds `max_scroll`, clamp it.

## Public API

```rust
pub use widgets::scroll_view::ScrollView;
```

No `ScrollViewElement` or `ScrollViewRenderObject` exported — they're implementation details.

Usage:
```rust
ScrollView::new(
    Flex::column()
        .gap(8.0)
        .push(Text::new("Item 1").padding(8.0))
        .push(Text::new("Item 2").padding(8.0))
        // ... many more items
)
```

## Edge Cases

- **Content smaller than viewport**: `max_scroll = 0`, scroll offset clamped to 0. No scrolling possible.
- **Content exactly fits viewport**: Same as above — no scroll.
- **Zero-size viewport**: Content laid out but not visible. Scroll offset remains 0.
- **Negative scroll delta at top / positive delta at bottom**: Clamped, no overscroll.
- **Child rebuilds changing content size**: Next layout + paint cycle recomputes content size. Offset clamped if it exceeds new `max_scroll`.

## Frame Scheduling

When scroll offset changes (from any input), the element marks itself dirty to trigger a repaint. No relayout needed — scroll offset is a paint-time transform, not a layout change.

## Testing

- Unit tests for scroll offset clamping logic
- Unit tests for keyboard scroll amounts
- Integration test: ScrollView with overflowing child, verify PushClip + PushOffset + PopOffset + PopClip in render commands
- Integration test: hit testing with scroll offset adjusts pointer positions
- Visual test via shared_app demo with ~20 scrollable items

## Files to Create/Modify

### New files
- `vexo/src/widgets/scroll_view.rs` — ScrollView widget
- `vexo/src/elements/scroll_view.rs` — ScrollViewElement
- `vexo/src/render_objects/scroll_view.rs` — ScrollViewRenderObject

### Modified files
- `vexo/src/render_object.rs` — add `scroll_offset()` default method to RenderObject trait
- `vexo/src/painter.rs` — emit PushOffset/PopOffset around children when `scroll_offset()` returns Some
- `vexo/src/event_handler.rs` — add Scroll event dispatch to hit-path ScrollView
- `vexo/src/hit_test.rs` — no changes needed (existing `hit_test_transform()` mechanism works)
- `vexo/src/widgets/mod.rs` — register ScrollView widget
- `vexo/src/elements/mod.rs` — register ScrollViewElement
- `vexo/src/render_objects/mod.rs` — register ScrollViewRenderObject
- `vexo/src/lib.rs` — re-export ScrollView
- `shared_app/src/lib.rs` — add ScrollView demo section
