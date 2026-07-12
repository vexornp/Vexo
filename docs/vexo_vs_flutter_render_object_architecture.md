# Vexo vs Flutter: RenderObject Architecture Differences

Status: Finding — migration planned for later.
Related bug: navigation transition text wrapping (fixed in `76bfc73` via `AlignItems::Stretch` workaround).

## TL;DR

Flutter has two kinds of RenderObjects: **pass-through** (forward constraints, adopt child size) and **layout containers** (compute their own layout). Vexo has only layout containers — every render object is a flex container with its own Taffy node. This architectural difference is why the navigation transition text-wrapping bug was possible in Vexo but cannot occur in Flutter.

---

## The Two Levels of "Invisible to Layout"

### Level 1: Widget → Element (no RenderObject)

Both Flutter and Vexo have this. Most widgets don't create a render object at all — their element just calls `build()`/`render()` and forwards to the child's render object.

| Flutter | Vexo | Effect on layout |
|---|---|---|
| `StatelessWidget` | `Component` (stateless usage) | None — no RenderObject created |
| `StatefulWidget` | `Component` + `ComponentState` | None — no RenderObject created |
| `InheritedWidget` | (n/a — Vexo uses `Signal`) | None — no RenderObject created |
| `Builder` | closures in `render()` | None |

This level is **not the source of the bug**. Vexo handles this correctly.

### Level 2: RenderObject (pass-through vs layout container) — **the difference**

Even among widgets that DO create RenderObjects, Flutter splits them into two categories. **Vexo does not** — every Vexo render object is a layout container.

#### Flutter's pass-through RenderObjects

These forward the parent's `BoxConstraints` directly to the child and adopt the child's size. No layout node added. The child sees the **grandparent's** constraints directly.

```dart
// RenderOpacity (Flutter)
void performLayout() {
  child?.layout(constraints, parentUsesSize: true);  // forward as-is
  size = child!.size;                                 // adopt child size
}
```

| Pass-through (forward + adopt) | Layout containers (compute own layout) |
|---|---|
| `RenderOffstage` | `RenderFlex` (Row/Column) |
| `RenderOpacity` | `RenderStack` |
| `RenderTransform` | `RenderIndexedStack` |
| `RenderClipRect` | `RenderConstrainedBox` |
| `RenderDecoratedBox` | `RenderPadding` |
| `RenderFractionalTranslation` | `RenderLimitedBox` |
| `RenderFittedBox` | `RenderAspectRatio` |

#### Vexo's all-containers approach

Every Vexo render object becomes a `Column + AlignItems::Stretch` flex container with its own Taffy node. The child is always a flex item, never a direct recipient of grandparent constraints.

| Vexo RenderObject | Layout used | Flutter equivalent |
|---|---|---|
| `OffstageRenderObject` (onstage) | `Column + Stretch` | `RenderOffstage` (pass-through) |
| `OpacityRenderObject` | `Column + Stretch` | `RenderOpacity` (pass-through) |
| `TransformRenderObject` | `Column + Stretch` | `RenderTransform` (pass-through) |
| `DecoratedContainerRenderObject` | `Column + Stretch` + style | `RenderDecoratedBox` (pass-through) |
| `ContainerRenderObject` (Flex/Stack/IndexedStack) | custom `Layout` | `RenderFlex` / `RenderStack` |

---

## The Bug This Caused

### Scenario: NavigationStackView push/pop transition

During a transition, the widget tree is:
```
Flex::column (NavStackView root)
├── nav_bar (Flex::row)
└── Stack
    ├── Positioned(left=0, right=0, top=0, bottom=0)  // outgoing
    │   └── Opacity
    │       └── Transform
    │           └── page Column → Text
    └── Positioned(left=0, right=0, top=0, bottom=0)  // incoming
        └── Opacity
            └── Transform
                └── page Column → Text
```

### Why it works in Flutter

Flutter's `IndexedStack.performLayout` lays out **only the visible child**, hands it the **parent constraints directly**, and sizes itself to the result:
```dart
void performLayout() {
  final RenderBox? child = _childAtIndex();
  if (child != null) {
    child.layout(constraints, parentUsesSize: true);  // parent constraints!
    size = constraints.constrain(child.size);
  }
}
```
The visible child receives a definite `BoxConstraints` from the start — no max-content measurement, no circular dependency. `Offstage`, `Opacity`, `Transform` are all pass-throughs, so the page Column's Text receives those same definite constraints through every layer.

### Why it broke in Vexo

Vexo's width resolution is **bottom-up max-content, then top-down stretch**, via Taffy flexbox:

1. The `IndexedStack`'s parent (`Offstage`) is content-sized (IndexedStack used `AlignItems::Start`, so children weren't stretched).
2. Taffy computes the IndexedStack's max-content from in-flow children.
3. **Steady-state** worked by accident: the visible `Offstage` → page Column had a max-content width (345px = text natural 297px + 48px padding) just wide enough for the text to fit on one line.
4. **Transition** broke: the `Stack`'s children were all `Positioned` (absolute → zero in-flow max-content contribution). So the Stack's max-content collapsed to 0.
5. The parent `Flex::column` shrank to its other child's max-content: the nav_bar (140px).
6. `Stack`'s `width_percent(1.0)` resolved to 140px.
7. `Positioned(left=0, right=0)` resolved to 140px.
8. Page Column → 140px. Text → 92px (140 − 48 padding).
9. Text natural width 297px > 92px → **wraps into 5 lines**.

### The workaround fix

Changed `IndexedStack` and `Stack` from `AlignItems::Start` to `AlignItems::Stretch`. This makes the visible child fill the stack's cross-axis, giving it a definite width from the parent (top-down) rather than from its own content (bottom-up). `Positioned` children are absolute and unaffected by `AlignItems`.

**Limitation of the workaround:** This is still flex-nesting. Similar circular-dependency bugs can resurface whenever a flex container's size becomes content-derived and its visible subtree has weak in-flow max-content (all-absolute children, or children whose natural width depends on the available width).

---

## Migration Plan: Add Pass-Through RenderObjects

### Goal

Introduce a category of render objects whose `layout()` forwards the parent's available space to the child and adopts the child's size — matching Flutter's `RenderOpacity`/`RenderOffstage`/`RenderTransform` semantics.

### Candidates (in priority order)

| RenderObject | Current layout | Target behavior | Priority |
|---|---|---|---|
| `OpacityRenderObject` | `Column + Stretch` | Pass-through (forward + adopt) | High — used in every transition |
| `TransformRenderObject` | `Column + Stretch` | Pass-through (forward + adopt) | High — used in every transition |
| `OffstageRenderObject` (onstage) | `Column + Stretch` | Pass-through (forward + adopt) | High — used in IndexedStack |
| `DecoratedContainerRenderObject` | `Column + Stretch` + style | Pass-through (forward + adopt) | Medium — decoration shouldn't constrain |
| `ContainerRenderObject` (Flex) | custom `Layout` | Keep as layout container | — |
| `ContainerRenderObject` (Stack/IndexedStack) | custom `Layout` | Keep as layout container (but consider Flutter-style `performLayout` for IndexedStack) | — |

### Design considerations

1. **Taffy constraint model vs Flutter BoxConstraints.** Taffy's `compute_layout` takes `available_space: Size<AvailableSpace>` where `AvailableSpace` is `Definite(f32) | MaxContent | MinContent`. A pass-through render object would:
   - Receive `available_space` from parent.
   - Forward it to the child's Taffy node as `known_dimensions: None` + `available_space` unchanged.
   - Read back the child's computed size.
   - Adopt that size as its own.

   This is different from the current `Column + Stretch` which inserts a flex container that participates in max-content measurement.

2. **Single-child vs multi-child.** Pass-through only applies to single-child render objects (`Opacity`, `Transform`, `Offstage`, `DecoratedContainer`). Multi-child containers (`Flex`, `Stack`, `IndexedStack`) remain layout containers.

3. **IndexedStack special case.** RESOLVED. `IndexedStack` now uses a dedicated `IndexedStackRenderObject` that filters its Taffy `set_children()` to include only the child at `index` (Option B: offstage children's zero-size leaf nodes are not linked to the stack's Taffy node). This matches Flutter's `RenderIndexedStack.performLayout`. See `vexo/src/render_objects/indexed_stack.rs`.

4. **Measurement caching.** Pass-through eliminates the extra max-content measurement layers. This is both a correctness improvement (no circular dependencies) and a performance improvement (fewer Taffy nodes, fewer measurement passes).

5. **Backward compatibility.** Widgets that currently rely on `Opacity`/`Transform`/etc. being flex containers (e.g. using `.gap()` or `.align()` on them) would need to either:
   - Be wrapped in an explicit `Flex` when flex behavior is needed, OR
   - The render object could support an opt-in flex mode via a layout builder method.

   Audit needed: grep for `.gap()`, `.align()`, `.flex_grow()` chained on `Opacity`/`Transform`/`Offstage`/`DecoratedContainer`.

### Migration steps

1. **Audit** — Find all usages of `Opacity`/`Transform`/`Offstage`/`DecoratedContainer` that chain flex-builder methods (`.gap()`, `.align()`, `.flex_grow()`, `.width_percent()`, etc.). These rely on the flex-container behavior and would need explicit `Flex` wrappers after migration.

2. **Prototype** — Implement pass-through on `OpacityRenderObject` first (simplest, most-used in transitions). Add a layout test that verifies the child receives the grandparent's definite width directly.

3. **Verify** — Run the navigation transition with the pass-through `Opacity` and confirm text no longer wraps even if `IndexedStack`/`Stack` revert to `AlignItems::Start`.

4. **Roll out** — Migrate `Transform`, `Offstage`, `DecoratedContainer` one at a time, running the full test suite after each.

5. **IndexedStack Flutter-style performLayout** — DONE. Implemented via `IndexedStackRenderObject`. See plan `docs/superpowers/plans/2026-07-12-indexed-stack-flutter-style-perform-layout.md`.

6. **Revert workaround** — Once pass-through render objects are in place, evaluate whether the `AlignItems::Stretch` workaround on `IndexedStack`/`Stack` is still needed. It may remain (Stretch is reasonable default behavior) or revert to `Start` if pass-through eliminates the circular dependency.

### References

- Bug fix commit: `76bfc73` — `fix(widgets): stretch IndexedStack/Stack children to fix nav transition text wrapping`
- `vexo/src/widgets/indexed_stack.rs:39` — `indexed_stack_layout()`
- `vexo/src/widgets/stack.rs:41` — `stack_layout()`
- `vexo/src/render_objects/offstage.rs:96-100` — onstage layout (flex container)
- `vexo/src/render_objects/opacity.rs:59-61` — Opacity layout (flex container)
- `vexo/src/widgets/transform.rs:85-87` — Transform layout (flex container)
- Flutter `RenderOffstage`: `packages/flutter/lib/src/widgets/basic.dart`
- Flutter `RenderIndexedStack`: `packages/flutter/lib/src/widgets/basic.dart` (`performLayout` override)
- Flutter `RenderOpacity`: `packages/flutter/lib/src/rendering/proxy_box.dart`
