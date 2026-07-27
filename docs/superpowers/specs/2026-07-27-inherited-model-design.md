# InheritedModel — Aspect-Based Subscriptions (Design Note)

**Date:** 2026-07-27
**Status:** Design note (not approved for implementation)
**Scope:** `vexo/` framework

## Problem

Today, `RenderContext::depend_on_inherited_widget::<V>()` registers the
caller as a dependent of the **entire** `V` value. When any field of `V`
changes, all dependents rebuild — regardless of which field the dependent
actually read.

`MediaQueryData` has seven fields (`size`, `device_pixel_ratio`, `padding`,
`viewInsets`, `viewPadding`, `platform_brightness`, `orientation`). A
widget that reads only `size` still rebuilds when `viewInsets.bottom`
changes by 1px during a keyboard animation frame.

This forces app authors to isolate `MediaQuery::of(ctx)` calls into small
leaf components (see `KeyboardInsetPadding` in `shared_app/src/chats/`).
The isolation works but is manual: the author must know to do it, and
must structure their tree around it. If a component legitimately needs to
read MediaQuery for a layout decision and sits in a hot path, isolation
may not be feasible.

## Two solutions considered

### Solution A — InheritedModel (aspect-based subscriptions)

**What it does:** dependents subscribe to a specific *aspect* of the
value, not the whole value. Only dependents whose subscribed aspect
changed get rebuilt.

This is Flutter's `InheritedModel` design. It is a clean, principled
extension of the existing `InheritedWidget` system. It does not break the
widget model. It composes with `Memo` and `should_rebuild`.

#### Sketch of the API

1. Define an aspect enum per InheritedWidget:

```rust
pub enum MediaQueryAspect {
    Size,
    DevicePixelRatio,
    Padding,
    ViewInsets,
    ViewPadding,
    PlatformBrightness,
    Orientation,
}
```

2. Extend the `InheritedWidget` trait:

```rust
pub trait InheritedWidget: Clone + 'static {
    type Value: Clone + PartialEq + Send + Sync + 'static;
    type Aspect: Hash + Eq + Clone + 'static;  // NEW

    fn value(&self) -> &Self::Value;
    fn child(&self) -> &dyn Widget;

    // Existing: should value change notify anyone?
    fn update_should_notify(&self, old: &Self, new: &Self) -> bool { ... }

    // NEW: should this specific dependent be notified?
    fn update_should_notify_dependent(
        &self,
        old: &Self,
        new: &Self,
        aspects: &HashSet<Self::Aspect>,
    ) -> bool;
}
```

3. Extend `RenderContext` to accept an optional aspect:

```rust
pub fn depend_on_inherited_widget_with_aspect<V>(
    &mut self,
    aspect: V::Aspect,
) -> Option<V>
```

4. `InheritedRegistry` stores
   `HashMap<(ElementKey, TypeId), HashSet<Aspect>>` instead of just
   `HashSet<ElementKey>`. On provider update, iterate dependents and call
   `update_should_notify_dependent` per-dependent — only mark dirty if
   their subscribed aspects changed.

5. `MediaQuery` implements `update_should_notify_dependent` with one
   clause per field.

#### What this solves

| Component | What it reads | Today (whole-value) | With InheritedModel |
|---|---|---|---|
| `KeyboardInsetPadding` | `viewInsets.bottom` | Rebuilds on every MediaQuery change (correct, but also rebuilds on rotation unnecessarily) | Rebuilds only when `ViewInsets` changes |
| Hypothetical `ChatScreen` that reads `size` | `size` | Rebuilds on every keyboard frame (spurious) | Rebuilds only on rotation |
| Hypothetical `TabBar` that reads `padding.bottom` | `padding.bottom` | Rebuilds on every keyboard frame | Rebuilds only when `Padding` changes |

#### What InheritedModel does NOT solve

It does **not** eliminate `KeyboardInsetPadding`. Even with aspects, the
lookup still registers a dependency — just a finer-grained one.
`KeyboardInsetPadding` still exists as a leaf dependent; the only
difference is it doesn't rebuild on rotation (only on keyboard change).

The big win is for components that today are *spuriously* rebuilt. If
`ChatScreen` needed `size` for some layout decision, today it would
rebuild on every keyboard frame; with InheritedModel, it would rebuild
only on rotation. That's a real win — but only if such components exist.
Today, we've isolated them all away, so the win is theoretical.

#### Cost

~200–300 lines of framework code plus per-InheritedWidget aspect
definitions. Touches `InheritedWidget` trait, `InheritedRegistry`,
`RenderContext`, and every concrete `InheritedWidget` implementation
(`MediaQuery`, `Theme`).

### Solution B — Non-subscribing reads in the render-object layer

**The observation:** `KeyboardInsetPadding` only changes *padding* — a
layout property. The widget tree doesn't need to rebuild at all; the
render object could just re-read the current keyboard height each frame
and update its padding in `apply_layout()`.

#### Sketch

A render object that pulls the keyboard height directly from a source,
bypassing the InheritedWidget dependency system:

```rust
pub struct KeyboardPaddingRenderObject {
    layout_node: LayoutNodeKey,
}

impl RenderObject for KeyboardPaddingRenderObject {
    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        // Read CURRENT value, no subscription registered.
        let kb = ctx.build_owner.keyboard_inset_source().get();
        ctx.engine().set_padding(self.layout_node, EdgeInsets::bottom(kb));
    }
}
```

Then `KeyboardInsetPadding` becomes a trivial wrapper widget — no
`Component`, no `MediaQuery::of(ctx)`, no rebuilds at all. The render
object pulls the current value each layout pass.

#### Why this is harder than it looks

1. **Render objects don't currently have BuildOwner access.** `LayoutContext`
   has the layout engine and font system, not the media-query sources.
   Plumbing it through is invasive.

2. **Layout invalidation trigger.** Today, `mark_all_needs_layout()` on
   resize triggers re-layout. But the keyboard source changing doesn't
   call `mark_all_needs_layout()` — it calls `mark_needs_build()` on
   MediaQuery dependents, which then rebuild their render objects, which
   then re-layout. If we bypass the widget layer, we need a separate
   mechanism: "when keyboard source changes, mark this render object's
   layout dirty." That's a new subscription channel — and it would
   reproduce the same dependency-tracking problem InheritedModel solves,
   just at the render-object layer.

3. **It breaks the "everything is a widget" philosophy.** The widget tree
   is supposed to be the source of truth for *what to render*. A render
   object that pulls its own data is a step back toward immediate mode.
   Justifiable for hot paths, but a real design tension.

4. **Flutter doesn't do this.** Flutter's `MediaQuery` is purely
   widget-driven; padding changes go through widget rebuilds. Flutter
   makes this cheap via `const` widgets and `Element.update`'s
   `identical()` check — the rebuild happens but is O(1). Vexo's `Memo`
   is our equivalent. Solution B is something Flutter explicitly chose
   NOT to do.

## Recommendation

**InheritedModel (Solution A) is the right direction, but not now.**
Solution B is a trap — it creates a parallel data path that bypasses the
widget tree and reproduces the same dependency-tracking problem at a
different layer.

### Why not now

Trigger conditions for implementing InheritedModel:

1. We have a component that legitimately needs to read `MediaQuery` (or
   `Theme`) for a layout decision, AND
2. It sits in a hot path where rebuilding on unrelated aspects would blow
   the frame budget, AND
3. Component isolation isn't feasible (e.g., the read is intrinsic to the
   component's purpose, not a wrapper).

None of our current components hit all three:

- `KeyboardInsetPadding` is a pure wrapper — isolation works.
- `ChatScreen` doesn't read `MediaQuery` at all.
- `TabBar` doesn't either.
- `NavigationStackView` reads `MediaQuery::of(ctx).padding` for safe-area
  insets, but it's gated by `should_rebuild()` returning false on parent
  cascades, so the cost is bounded.

When we do hit all three — probably when we add responsive layouts that
switch based on `size.width` — InheritedModel becomes worth the cost.

### What to do in the meantime

- Continue using **component isolation** (the `KeyboardInsetPadding`
  pattern) for new hot-path components that need MediaQuery.
- Use **`Memo<T>`** to cache stable subtrees under frequently-rebuilding
  parents.
- Use **`should_rebuild()`** as the escape hatch when neither isolation
  nor `Memo` is feasible.

See `docs/rebuild-skipping-patterns.md` for the full ladder.

## Open questions (for when we do implement)

1. **Aspect granularity.** Is `ViewInsets` one aspect, or four
   (`ViewInsetsTop`, `ViewInsetsBottom`, ...)? Flutter uses whole-field
   aspects; finer granularity adds complexity for marginal gain. Lean
   toward whole-field.

2. **Backward compatibility.** Can `depend_on_inherited_widget::<V>()`
   (no aspect) continue to mean "subscribe to all aspects"? If yes,
   existing code keeps working. New code opts in to fine-grained
   subscriptions.

3. **Aspect inheritance.** Does `Theme::of(ctx)` subscribe to all of
   `ThemeData`'s aspects by default, or do we force callers to specify?
   `ThemeData` has ~10 color fields; an aspect per color is probably
   too fine. One aspect per logical group (backgrounds, text, borders)
   might be the right granularity.

4. **Multi-aspect subscriptions.** A component that reads both `size`
   and `orientation` needs to subscribe to both. API options:
   - `depend_on_inherited_widget_with_aspects::<V>(&[aspect1, aspect2])`
   - Two separate calls, each registering one aspect
   The first is cleaner; the second composes better with the existing
   single-aspect API.

These questions can be deferred until we have a concrete use case that
forces a decision.
