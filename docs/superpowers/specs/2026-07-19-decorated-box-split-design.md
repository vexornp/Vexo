# DecoratedBox vs DecoratedContainer Split — Design

**Date:** 2026-07-19
**Status:** Approved (section-by-section)
**Scope:** `vexo` crate

## Motivation

`DecoratedContainer` currently bundles two responsibilities: visual decoration
(`Style`: background, border, corner radius, clip, shadow) and sized layout
(`Layout`: padding, margin, width/height, flex, alignment). Its `new()`
defaults to `align_self(Start).flex_shrink(0.0)` — the "size to content"
behavior — which is a layout opinion that not every caller wants.

When a caller only wants to paint a decoration around an existing widget
*without* changing how that widget sizes itself, `DecoratedContainer` is the
wrong tool: it imposes a Taffy layout node and content-sized defaults that
break fill chains and shrink `flex_fill` children to their intrinsic size.

Flutter solves this by splitting `Container` (sized, with padding/margin/
decoration) from `DecoratedBox` (decoration only, transparent passthrough).
This spec brings the same split to Vexo.

## Goals

- Add a `DecoratedBox` widget for decoration-only use cases with **no layout
  opinion** — transparent to the Taffy layout engine.
- Keep `DecoratedContainer` unchanged so all existing callers keep working
  without modification.
- Single source of truth for decoration painting logic (no duplication
  between the two widgets).
- Fix the latent sizing bug in `WidgetExt::background/.border/
  .corner_radius/.clip` forward-looking: route them to `DecoratedBox` so
  they don't impose `align_self(Start).flex_shrink(0.0)` on the wrapped
  widget.

## Non-Goals

- No refactor of `ContainerRenderObject` other than extracting the
  `paint_style()` helper. Its layout, hit-test, and child management stay.
- No changes to `Flex`/`Stack`/`Grid`/`IndexedStack` inherent decoration
  methods. They own their `Style` directly and paint via
  `ContainerRenderObject`.
- No changes to `WidgetExt::padding/.margin/.width/.height/...` — those
  already route to `WithLayout` (correct behavior).
- No migration of existing `DecoratedContainer` callers to `DecoratedBox`.
  Callers that use both decoration and layout (all six current call sites)
  stay on `DecoratedContainer`.
- No deprecation of `DecoratedContainer`. Both widgets coexist long-term,
  matching Flutter.

## Architecture

### New types

Three new types split across two new files, following the existing pattern
where widgets/elements live in `vexo/src/widgets/` and render objects
live in `vexo/src/render_objects/` (cf. `DecoratedContainer` widget in
`widgets/decorated_container.rs` and `ContainerRenderObject` in
`render_objects/container.rs`):

- **`DecoratedBox`** (widget) + **`DecoratedBoxElement`** in a new file
  `vexo/src/widgets/decorated_box.rs`. The widget owns `Style` only (no
  `Layout`). Single child. Distinct `type_id()` from `DecoratedContainer`
  so the reconciler cannot confuse them.
- **`DecoratedBoxRenderObject`** in a new file
  `vexo/src/render_objects/decorated_box.rs`. True proxy render object:
  returns the child's Taffy node from `layout()`,
  `is_pass_through() == true`, no owned Taffy node. Additionally paints
  `Style` against its `computed_bounds` (which equals the child's bounds,
  since they share the Taffy node).

### Unchanged

`DecoratedContainer` — widget, element, and `ContainerRenderObject` — is
untouched. Defaults (`align_self(Start).flex_shrink(0.0)`), builders,
`border()` padding-adding behavior, and all call sites remain as-is.

### Painting helper

Extract a free function in `vexo/src/painter.rs`:

```rust
pub(crate) fn paint_style(
    style: &Style,
    bounds: Bounds<Logical>,
    ctx: &mut PaintContext,
) -> Vec<RenderCommand> { ... }
```

Body is the existing decoration-painting code lifted out of
`ContainerRenderObject::paint()` (background rect, border, corner-radius
push/pop, shadows, clip). `ContainerRenderObject::paint()` becomes a
one-liner:

```rust
fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
    self.computed_bounds
        .map_or(Vec::new(), |b| paint_style(&self.style, b, ctx))
}
```

`DecoratedBoxRenderObject::paint()` uses the same helper. Single source of
truth for decoration painting.

## Component Details

### `DecoratedBox` widget

```rust
pub struct DecoratedBox {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    style: Style,
}
```

- `DecoratedBox::new(child)` — defaults `style = Style::default()`. **No
  layout field, no `align_self`/`flex_shrink` defaults** — by design, the
  widget imposes zero layout opinion.
- Builders:
  - `.style(Style)` — replace entire style
  - `.background(Color)`, `.corner_radius(f32)`, `.clip()`,
    `.shadow(BoxShadow)`, `.shadows(Vec<BoxShadow>)` — same semantics as
    `DecoratedContainer`'s style builders
  - `.border(Color, f32)` — sets border on style. **Does NOT add padding**
    (Flutter semantics). Doc comment explicitly calls out the difference
    from `DecoratedContainer::border()` and cross-references it.
- `.with_key(impl Into<WidgetKey>)` — same as other widgets.
- No `layout_builder_methods!()` — there is no `Layout` to set. If a
  caller wants padding/sizing, they compose with `WithLayout` or use
  `DecoratedContainer`.
- `Widget::create_element()` → `DecoratedBoxElement::new()` +
  `set_widget(self)`.
- `Widget::create_render_object()` →
  `DecoratedBoxRenderObject::new(self.style.clone())`.
- `Widget::update_render_object()` — downcast to
  `DecoratedBoxRenderObject`, call `set_style()`. Return `PAINT` if
  changed, `NONE` otherwise. Never `LAYOUT` — proxy has no layout.
- `Widget::can_update()` — `type_id()` equality. Distinct from
  `DecoratedContainer` so the reconciler will not confuse them.

### `DecoratedBoxElement`

Structurally identical to `DecoratedContainerElement` minus the layout
bookkeeping:

```rust
pub struct DecoratedBoxElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}
```

Implements `RenderObjectElement` and `Element` traits.

- `mount()` — create focus attachment (before child mount, same as
  `DecoratedContainerElement::mount`), mount render object, inflate
  single child via `context.inflate_child(None, child_widget)`.
- `update()` — delegate to `update_render_object()`.
- `unmount()` — unmount render object, detach focus node.
- `rebuild()` — downcast widget, update render object (returns `PAINT`
  only, never `LAYOUT`), `mark_needs_paint()` if changed. Reconcile
  single child via `update_child` / `inflate_child` / `unmount_child`.
  Reparent focus node if parent changed.
- `child_mounted()` — link child render object via
  `insert_child_render_object()`.
- `can_update()` — `type_id()` equality.
- `on_event()` — returns `None`. Pure decoration, no event handling.

### `DecoratedBoxRenderObject`

Lives in `vexo/src/render_objects/decorated_box.rs` (matching
`ContainerRenderObject`'s placement in `render_objects/container.rs`).

```rust
pub struct DecoratedBoxRenderObject {
    child: Option<RenderObjectKey>,
    style: Style,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
}
```

Behavior mirrors `ProxyRenderObject` (`stateful_widget.rs:862-977`)
exactly, with two additions:

- Stores `style: Style` and a `set_style(Style) -> bool` setter.
- `paint()` returns `paint_style(&self.style, self.computed_bounds?, ctx)`
  instead of empty `Vec`.

Like `ProxyRenderObject`:

- `is_pass_through() == true`
- `layout()` returns the child's Taffy node directly (or a throwaway
  zero-size leaf if no child — defensive, never panics)
- `apply_layout()` reads the shared Taffy node's computed bounds into
  `self.computed_bounds`
- `set_child_id()`, `replace_child()`, `children()`, `layout_node()`,
  `computed_bounds()` — same implementations
- `hit_test()` — bounds check against `computed_bounds`

`set_style()` returns `true` if the new style differs from the old one,
so `Widget::update_render_object()` can return `PAINT` only when something
actually changed.

## Data Flow

### Layout

```
grandparent (Taffy node)
    │
    └── DecoratedBoxRenderObject (NO own Taffy node — is_pass_through=true)
            │  layout() returns child's Taffy node directly
            │  apply_layout() reads shared node's computed_bounds
            └── child render object (owns the Taffy node grandparent links to)
```

The grandparent's Taffy layout node points directly at the child's Taffy
node — `DecoratedBoxRenderObject` is invisible to the layout engine,
exactly like `ProxyRenderObject`. The child sizes itself naturally;
`DecoratedBoxRenderObject.computed_bounds` equals the child's bounds (same
Taffy node, same `get_layout()` result).

### Paint

`Painter::paint_recursive` (`painter.rs:71`) visits
`DecoratedBoxRenderObject`:

1. Reads `position_in_parent` from `computed_bounds` (equal to child's,
   since they share the Taffy node).
2. Computes `absolute_position` and calls
   `ctx.set_absolute_position(absolute_position)`.
3. Calls `obj.paint(ctx)` → `paint_style(&style, computed_bounds, ctx)`
   emits background / border / corner-radius / shadow / clip commands
   positioned at `absolute_position`.
4. Because `is_pass_through() == true`, `child_parent_absolute =
   absolute_position - position_in_parent` (`painter.rs:167-174`) —
   cancels the double-count so the child paints at the same absolute
   position as the wrapper. Background and child overlap exactly, which
   is what we want (background fills the child's area).
5. Recurses into the child. Child paints at the same absolute position.

Order matters: **wrapper's decoration commands are pushed before the
child's commands**, so the child paints on top of the background. This
matches `DecoratedContainer`'s current behavior
(`ContainerRenderObject::paint()` emits decoration commands, then the
painter recurses into children).

### Hit test

`HitTest::hit_test_recursive` (`hit_test.rs:387-394`) applies the same
pass-through correction. `DecoratedBoxRenderObject.hit_test()` does a
bounds check against `computed_bounds` (inherited from
`ProxyRenderObject`'s implementation). If the pointer is inside the
bounds, the test recurses into the child with the corrected absolute
position.

### Update (rebuild)

When `DecoratedBox`'s `Style` changes (e.g. `.background(RED)` →
`.background(BLUE)`):

1. `Widget::update_render_object()` calls `set_style(new_style)` on the
   existing `DecoratedBoxRenderObject` → returns `true`.
2. Returns `UpdateResult::PAINT` (never `LAYOUT` — proxy has no layout).
3. `DecoratedBoxElement::rebuild()` sees `PAINT` →
   `context.mark_needs_paint(ro_id)`.
4. Next paint frame, `paint_style()` re-runs with the new style.

When the child widget changes: standard `update_child` reconciliation —
element reuses the existing child element if `can_update`, else
replaces. The render object subtree is unchanged.

### Removal

`RenderObjectRegistry::remove()` (`render_object.rs:618-624`) checks
`is_pass_through()` and skips orphan-node collection —
`DecoratedBoxRenderObject` has no Taffy node to clean up. The child's
Taffy node is cleaned up when the child's render object is removed (its
own `remove()` call).

## Error Handling & Edge Cases

- **Missing child (defensive):**
  `DecoratedBoxRenderObject::layout()` with no `child_nodes` creates a
  throwaway zero-size leaf Taffy node (same as `ProxyRenderObject::layout()`
  at `stateful_widget.rs:907-914`). `paint()` with no `computed_bounds`
  returns empty `Vec`. Never panics.
- **`set_style()` on wrong RO type:** `Widget::update_render_object()`
  downcasts to `DecoratedBoxRenderObject`; on failure returns
  `UpdateResult::ALL` (matches `DecoratedContainer`'s pattern at
  `decorated_container.rs:435-437`). Cannot happen in practice — element
  owns its RO — but defensive.
- **Border with no inset (Flutter semantics):** `DecoratedBox::border(RED,
  2.0)` paints a 2px border over the child's edge pixels. Caller must add
  padding explicitly (via `WithLayout` or `DecoratedContainer`) if they
  want the child inset. Documented on `DecoratedBox::border()` with a
  cross-reference to `DecoratedContainer::border()` which adds the padding
  automatically.
- **`DecoratedBox` wrapping a widget that itself has `Style`** (e.g.
  `Text.background(RED)` then wrapped in `DecoratedBox`): both styles
  paint. The inner `Text`'s background fills the text's intrinsic size; the
  outer `DecoratedBox`'s background fills the same bounds (since proxy).
  They overlap exactly — same behavior as `DecoratedContainer` wrapping a
  styled `Text` today.
- **`DecoratedBox` inside a `Column` with `align: Stretch`:** because the
  proxy has no Taffy node, the `Column`'s cross-axis stretch applies to
  the *child* directly. The child stretches to fill the column's width;
  `DecoratedBox` adopts that same width. Contrast with
  `DecoratedContainer` which would also stretch but imposes its own
  `align_self(Start)` default unless overridden.
- **Reconciliation type stability:** `DecoratedBox` and
  `DecoratedContainer` have distinct `type_id()`s. If a caller swaps one
  for the other at the same tree position, the reconciler unmounts the
  old element and mounts a new one (cannot `update` across types). This
  is the desired behavior — they have different render object types. The
  `navigation.rs:742` comment about type-stability (steady `Stack` vs
  `DecoratedContainer(Stack)`) does not apply here because we are not
  changing `DecoratedContainer`'s type.

## Testing

### Unit tests (`vexo/src/widgets/decorated_box.rs`)

- `test_decorated_box_creation` — `new()` has no key, default style.
- `test_decorated_box_with_key` — local + global keys.
- `test_decorated_box_style_builder_chain` — `.background().border()
  .corner_radius().clip().shadow().shadows()` all set the corresponding
  style fields.
- `test_decorated_box_border_does_not_add_padding` — regression guard
  for the semantic difference from `DecoratedContainer`.
- `test_decorated_box_render_object_is_pass_through` —
  `is_pass_through() == true`.
- `test_decorated_box_render_object_paint_emits_commands` — background +
  border yields 2 commands; corner-radius yields 3 (push/pop); empty
  style yields 0. Mirrors `decorated_container.rs:498-542`.
- `test_decorated_box_render_object_set_style_returns_true_on_change` —
  setter change detection.
- `test_decorated_box_update_render_object_returns_paint_only` — style
  change returns `PAINT`, never `LAYOUT`.

### Render object tests (`vexo/src/render_objects/decorated_box.rs` — new file)

- `test_decorated_box_layout_returns_child_node` — `layout()` with one
  child node returns that node; `layout_node()` returns it after layout.
- `test_decorated_box_layout_no_child_creates_throwaway_node` —
  defensive path.
- `test_decorated_box_paint_adopts_child_bounds` — set
  `computed_bounds`, verify `paint()` emits commands at those bounds via
  `paint_style()`.

### Integration tests (extend `vexo/src/e2e_test.rs`)

- `test_decorated_box_in_pipeline` — `DecoratedBox(Text)` with
  background+border+corner-radius. Verify: render object count is 2
  (decorated box + text), parent RO `is_pass_through()`, child RO's
  Taffy node is linked directly to grandparent, background/border
  commands appear in output. Mirrors `e2e_test.rs:125`
  (`test_decorated_container_in_pipeline`).
- `test_decorated_box_width_propagates_to_child` —
  `Column > DecoratedBox(width=200) > Text`. Verify the text wraps at
  200px (proxy passes width constraint through). This is the regression
  guard for the latent `WidgetExt` sizing bug.

### `paint_style()` helper tests

Existing `ContainerRenderObject::paint()` tests in
`render_objects/container.rs:412-530` automatically re-test the helper
once `ContainerRenderObject::paint()` delegates to it. No new tests
needed there.

### WidgetExt routing tests (extend `vexo/src/widgets/mod.rs` tests)

- `test_widget_ext_background_wraps_in_decorated_box` —
  `Text("x").boxed().background(RED)` (note: requires `Box<dyn Widget>`
  to trigger `WidgetExt`, since `Text` has inherent `.background()`) →
  downcast outer to `DecoratedBox`. Replaces the implicit assumption
  that it wraps in `DecoratedContainer`.

### Existing tests that must still pass

- All `decorated_container.rs` tests — `DecoratedContainer` is unchanged.
- `vexo_uikit/tests/navigation_animation_tests.rs` — downcasts to
  `DecoratedContainer` still work because the type is unchanged.
- `vexo_uikit/tests/button_render_tests.rs` — downcasts to
  `DecoratedContainer` still work.
- `vexo/src/e2e_test.rs` existing `DecoratedContainer` tests — unchanged.

## Migration & Scope

### `DecoratedContainer` is unchanged

File `vexo/src/widgets/decorated_container.rs`: zero edits. All six
existing `DecoratedContainer` call sites keep working without
modification:

- `vexo_uikit/src/transitions.rs:87` — shadow + fill on transition page
- `vexo_uikit/src/navigation.rs:742` — clip + fill on nav content stack
- `vexo_uikit/src/button.rs:257` — background + corner-radius + padding
  + border on button
- `shared_app/src/chats/chat_screen.rs:127` — padding + corner-radius +
  background + border + max_width on message bubble
- `shared_app/src/chats/conversation_list.rs:79` — fixed size +
  background + corner-radius + center alignment on unread badge
- `vexo/src/e2e_test.rs:137, 224, 304, 376, 523` — test usage

These all use **both** decoration and layout, so `DecoratedContainer`
remains the right widget for them. No migration needed.

### `WidgetExt` modifier routing change

`vexo/src/widgets/mod.rs:196-223` — four methods change their wrapping
target:

```rust
// Before:
fn background(self, color: Color) -> Box<dyn Widget> {
    Box::new(DecoratedContainer::new(self).background(color))
}
// After:
fn background(self, color: Color) -> Box<dyn Widget> {
    Box::new(DecoratedBox::new(self).background(color))
}
```

Same for `.border()`, `.corner_radius()`, `.clip()`. Import
`DecoratedBox` in `widgets/mod.rs`.

### Audit of existing `WidgetExt` decoration call sites

Traced every `.background()` / `.border()` / `.corner_radius()` /
`.clip()` chain in the repo. **Zero existing callers go through
`WidgetExt` for these.** Every one resolves to an *inherent* method on a
concrete widget type:

| Call site | Receiver type | Method resolution |
|---|---|---|
| `shared_app/src/widgets/avatar.rs:20-21` | `Image` | inherent `Image::corner_radius`, `Image::clip` |
| `shared_app/src/chats/conversation_list.rs:86-87` | `DecoratedContainer` | inherent |
| `shared_app/src/chats/chat_screen.rs:117` | `Column` (= `Flex`) | inherent `Flex::background` |
| `shared_app/src/chats/chat_screen.rs:137-143` | `DecoratedContainer` | inherent |
| `vexo_uikit/src/button.rs:258-259, 263` | `DecoratedContainer` | inherent |
| `vexo_uikit/src/navigation.rs:743` | `DecoratedContainer` | inherent |
| `vexo_uikit/src/navigation.rs:846, 858` | `Flex` | inherent `Flex::background` |
| `vexo_uikit/src/tab_bar.rs:206` | `Flex` | inherent `Flex::background` |
| `vexo/src/widgets/text_edit.rs:602-604` | `TextEditContent` | inherent |
| All test files | concrete widget types | inherent |

`Flex`, `Stack`, `Grid`, `IndexedStack`, `Text`, `Image`,
`TextEditContent`, and `DecoratedContainer` all have inherent
`.background/.border/.corner_radius/.clip` methods (via
`modifier_methods!()` for `Text`/`Image`/`TextEditContent`, hand-written
for `Flex`/`Stack`/`Grid`/`IndexedStack`, hand-written for
`DecoratedContainer`). The `WidgetExt` trait versions only fire when the
receiver is `Box<dyn Widget>` (dynamic dispatch), which never happens in
any current call site.

### Audit conclusion

**Zero behavior changes for existing callers.** The `WidgetExt` routing
change is forward-looking — it fixes the latent sizing bug for any
future `Box<dyn Widget>.background(RED)` call so the wrapper doesn't
impose `align_self(Start).flex_shrink(0.0)` on the wrapped widget.

## File-Level Change Summary

| File | Change |
|---|---|
| `vexo/src/widgets/decorated_box.rs` | **NEW** — widget + element + unit tests |
| `vexo/src/render_objects/decorated_box.rs` | **NEW** — `DecoratedBoxRenderObject` + render object tests |
| `vexo/src/widgets/mod.rs` | Export `DecoratedBox`; rewire 4 `WidgetExt` methods to use it; add 1 routing test |
| `vexo/src/painter.rs` | Extract `paint_style()` free function |
| `vexo/src/render_objects/container.rs` | `ContainerRenderObject::paint()` delegates to `paint_style()` (one-liner) |
| `vexo/src/render_objects/mod.rs` | Export `DecoratedBoxRenderObject` |
| `vexo/src/lib.rs` | Re-export `DecoratedBox` |
| `vexo/src/e2e_test.rs` | Add `test_decorated_box_in_pipeline` + `test_decorated_box_width_propagates_to_child` |

## Resolved Decisions

1. **Pass-through semantics** → Option B: true `ProxyRenderObject`-style
   (no Taffy node, adopts child bounds, `is_pass_through() == true`).
2. **`WidgetExt` routing** → Route + audit. Audit found zero existing
   callers (all use inherent methods), so the change is forward-looking
   with zero migration.
3. **`border()` padding behavior** → `DecoratedBox::border()` does NOT
   add padding (Flutter semantics). `DecoratedContainer::border()` keeps
   its current padding-adding behavior. Documented difference.
4. **Painting DRY** → Extract `paint_style()` free function in
   `painter.rs`. Both `ContainerRenderObject::paint()` and
   `DecoratedBoxRenderObject::paint()` delegate to it.
5. **`DecoratedContainer` fate** → Unchanged. Both widgets coexist
   long-term.
