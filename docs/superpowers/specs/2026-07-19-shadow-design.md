# BoxShadow Support for DecoratedContainer — Design Spec

**Date:** 2026-07-19
**Status:** Draft

## Problem

`DecoratedContainer` bundles visual decorations (background, border, corner
radius, clip) into a single element and render object for efficiency. It does
not support shadows. Real UIs need shadows for cards, modals, popovers, and
elevated buttons; today developers have no way to render them.

The framework already has every ingredient needed for efficient shadow
rendering:

- An SDF-based rounded-rect shader (`vexo/src/shader.wgsl:96-119`) whose
  signed-distance-to-edge computation is the foundation of Gaussian-blurred
  shadows.
- A `Style` struct (`vexo/src/style.rs:22-35`) that bundles decorations.
- A `DecoratedContainer` widget that exposes builder methods over `Style`.
- A `ContainerRenderObject::paint()` that emits `RenderCommand::Rect`s.
- A `QuadInstance` GPU primitive (`vexo/src/quad_instance.rs:7-20`) consumed
  by the existing single-pass render pipeline.

The goal of this spec is to add Flutter-`BoxShadow`-parity drop shadows to
`DecoratedContainer` with minimal code delta, maximal reuse of the existing
SDF shader, and no new render pipeline infrastructure.

## Scope (Decisions Locked During Brainstorming)

| # | Decision | Choice |
|---|----------|--------|
| 1 | Shadow capability | **Full BoxShadow list (drop shadows)** — color, offset, blur_radius, spread_radius. No inset shadows. Multiple shadows per container stack in list order. |
| 2 | API surface | **Add to `Style`/`DecoratedContainer`** — consistent with the existing pattern of bundling decorations in `Style` for single-element/single-RO efficiency. No new widget. |
| 3 | Animation | **Static now, animation-friendly later.** `BoxShadow` is a plain data struct; future animation work can interpolate between two `Vec<BoxShadow>` values without API changes. |
| 4 | Rendering approach | **SDF-shadow extension to existing shader** (Approach A from the brainstorm). Reuses the existing rounded-rect SDF; one extra `DrawOp::Quad` per shadow; no new render passes. |
| 5 | `RenderCommand::Rect` & `QuadInstance` extension | **Extend rather than introduce new variants.** Non-shadow quads carry zero-valued shadow fields; shader branches on `shadow_color.a > 0`. |
| 6 | Discriminator | **`shadow_color.a > 0`.** Correctly handles sharp (`blur = 0`) shadows; naturally false for zero-initialized non-shadow quads. |

## Non-Goals

- Inset shadows (shadow drawn inside the rect edges, like a pressed-button
  well). Deferred.
- Built-in animated shadow transitions. Callers can animate by rebuilding per
  frame from a `Signal`; framework-provided animation is deferred.
- True multi-pass Gaussian blur (render-to-texture). The SDF approximation is
  visually indistinguishable from Flutter's own approximate blur at typical UI
  radii (4-24px).
- Shadow-specific culling / occlusion optimizations beyond skipping
  fully-transparent shadows.
- New `DrawOp::Shadow` variant or separate shadow render pipeline. Not
  justified by current workloads.

## Architecture

No new widget, no new element, no new render object, no new render command
variant, no new `DrawOp` variant, no new render pipeline. The data flows
through every existing layer unchanged except for **added fields**.

### Data flow

```
Application::view()
  └─ DecoratedContainer { style: Style { shadows: Vec<BoxShadow>, .. }, .. }
     └─ Widget::create_render_object() → ContainerRenderObject { style, .. }
        └─ ContainerRenderObject::paint() emits N shadow Rects + fill/border Rects
           └─ RenderCommand::Rect (extended with shadow fields)
              └─ command_processor::process_commands → frame_builder.add_rect
                 └─ DrawOp::Quad(QuadInstance) (extended with shadow fields)
                    └─ WgpuBackend upload + draw (existing pipeline)
                       └─ shader.wgsl fs_main branches on shadow_color.a > 0
```

### Files touched

| File | Change |
|------|--------|
| `vexo/src/style.rs` | Add `BoxShadow` struct; extend `Style` with `shadows: Vec<BoxShadow>`; add `.shadow()`/`.shadows()` builders |
| `vexo/src/widgets/decorated_container.rs` | Add `.shadow()`/`.shadows()` builder pass-throughs |
| `vexo/src/render_objects/container.rs` | Emit shadow `Rect`s in `paint()`; clamp `blur ≥ 0` |
| `vexo/src/render/command.rs` | Extend `RenderCommand::Rect` with shadow fields |
| `vexo/src/frame_builder.rs` | Extend `add_rect()` (or add `add_shadow_rect()`) to populate new `QuadInstance` fields |
| `vexo/src/quad_instance.rs` | Add 5 new f32 fields + 4 vertex attributes (16 bytes incl. padding) |
| `vexo/src/shader.wgsl` | Add shadow inputs to `vs_main`/`VertexOutput`; add shadow branch to `fs_main` |
| `shared_app/src/lib.rs` | Add shadow showcase screen as manual smoke test |

## Public API

### `BoxShadow` (new, in `vexo/src/style.rs`)

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct BoxShadow {
    pub color: Color,
    pub offset: Point<Logical>,
    pub blur_radius: f32,
    pub spread_radius: f32,
}

impl BoxShadow {
    pub fn new(color: Color) -> Self;
    pub fn offset(self, x: f32, y: f32) -> Self;
    pub fn blur(self, radius: f32) -> Self;
    pub fn spread(self, radius: f32) -> Self;
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            offset: Point::zero(),
            blur_radius: 0.0,
            spread_radius: 0.0,
        }
    }
}
```

Fields mirror Flutter's `BoxShadow` minus `blurStyle` (we excluded inset). The
`Point<Logical>` type is used for offset to match the rest of the codebase.

### `Style` extension

```rust
pub struct Style {
    pub background: Option<Color>,
    pub border: Option<Border>,
    pub corner_radius: Option<CornerRadius>,
    pub clip: bool,
    pub shadows: Vec<BoxShadow>,   // ← NEW
}

impl Style {
    /// Append a shadow to the shadow list.
    ///
    /// Unlike `.background()` (which replaces), `.shadow()` appends —
    /// shadows are inherently a list, and the common case is stacking
    /// 2-3 shadows per container (e.g., a tight dark shadow below a
    /// soft light shadow). Use `.shadows(Vec)` to replace the whole list.
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadows.push(shadow);
        self
    }

    /// Replace the shadow list.
    pub fn shadows(mut self, shadows: Vec<BoxShadow>) -> Self {
        self.shadows = shadows;
        self
    }
}
```

`Style::default()` produces `shadows: Vec::new()` (empty) via
`#[derive(Default)]` on `Vec<BoxShadow>`.

### `DecoratedContainer` extension

Mirror the same two builder methods (matches existing pattern for
`.background()`, `.border()`, `.corner_radius()`, `.clip()`):

```rust
impl DecoratedContainer {
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.style = self.style.shadow(shadow);
        self
    }
    pub fn shadows(mut self, shadows: Vec<BoxShadow>) -> Self {
        self.style = self.style.shadows(shadows);
        self
    }
}
```

### Usage example

```rust
DecoratedContainer::new(Text::new("Elevated card").boxed())
    .background(Color::WHITE)
    .corner_radius(12.0)
    .shadow(
        BoxShadow::new(Color::rgba(0.0, 0.0, 0.0, 0.15))
            .offset(0.0, 4.0)
            .blur(12.0),
    )
    .shadow(
        BoxShadow::new(Color::rgba(0.0, 0.0, 0.0, 0.10))
            .offset(0.0, 2.0)
            .blur(4.0),
    )
```

## Render Command Extension

### `RenderCommand::Rect` (extended)

```rust
pub enum RenderCommand {
    Rect {
        bounds: Bounds<Logical>,
        fill: Color,
        stroke: Option<Stroke>,
        corner_radius: f32,

        // ← NEW (all zero for non-shadow rects)
        shadow_color: [f32; 4],   // [0,0,0,0] = not a shadow
        shadow_blur: f32,          // logical px
    },
    // ... other variants unchanged ...
}
```

Only 2 new fields (5 f32s) are added to the command. `spread_radius` and
`offset` are baked into `bounds` and `corner_radius` by
`ContainerRenderObject::paint()`; they do not need to flow through the
command. `shadow_color` and `shadow_blur` are the only fields the shader needs
beyond what `Rect` already carries.

### `QuadInstance` (extended)

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadInstance {
    // ... existing fields (88 bytes total: 22 f32s) ...
    pub shadow_color: [f32; 4],   // ← NEW (16 bytes)
    pub shadow_blur: f32,          // ← NEW (4 bytes)
    pub _padding2: [f32; 3],       // ← NEW (12 bytes padding for 16-byte alignment)
}
```

5 new f32s of payload plus 3 f32s of padding (20 + 12 = 32 bytes total).
Existing `QuadInstance` is 88 bytes (22 f32s); the extended struct is 120
bytes (30 f32s). Existing construction (`from_logical`, `with_transform`)
initializes shadow fields to zero.

### `QuadInstance::desc()` — new vertex attributes

Two new `VertexAttribute`s (padding does not consume shader locations):

```rust
// location 10: shadow_color (Float32x4), 4 bytes × 4
// location 11: shadow_blur  (Float32),    4 bytes
```

The existing `desc()` uses shader locations 1-9 (splitting the 6-element
`transform` array across locations 7-9 as `Float32x2` triples). The new
shadow fields use locations 10-11. Offsets are computed from the existing
80-byte offset of `_padding` (end of existing struct) plus the dropped
`_padding` field — exact offsets are an implementation detail finalized
during coding.

## Paint Order & Clipping

### `ContainerRenderObject::paint()` (revised)

```
1. For each shadow in style.shadows (in list order):
     if shadow.color.a == 0.0 { continue; }       // skip invisible
     let blur = shadow.blur_radius.max(0.0);       // clamp negative
     let pad = blur + shadow.spread_radius;
     let shadow_bounds = Bounds::new(
         bounds.left   + shadow.offset.x - pad,
         bounds.top    + shadow.offset.y - pad,
         bounds.right  + shadow.offset.x + pad,
         bounds.bottom + shadow.offset.y + pad,
     );
     let shadow_corner_radius = base_corner_radius + shadow.spread_radius;
     emit RenderCommand::Rect {
         bounds: shadow_bounds,
         fill: shadow.color,
         stroke: None,
         corner_radius: shadow_corner_radius,
         shadow_color: shadow.color.to_array(),
         shadow_blur: blur,
     }

2. PushCornerRadius?                  (if style.corner_radius set)
3. background Rect?                   (if style.background set)
4. border Rect?                       (if style.border set)
5. PopCornerRadius?                   (if style.corner_radius set)
6. [children paint here]              (existing — pushed by pipeline)
```

### Decisions

| Concern | Decision | Rationale |
|---------|----------|-----------|
| Self-clip (`style.clip`) clips shadows? | **No** | `style.clip`'s `PushClip` wraps only fill/border/children. Shadows must extend outside the container bounds; clipping them to the very shape casting them defeats the purpose. Matches Flutter `PhysicalShape`. |
| `PushCornerRadius` context wraps shadows? | **No** | Each shadow `Rect` carries its own `corner_radius` field (computed as `base + spread`). The explicit value is authoritative; wrapping in the context would be redundant at best. |
| Parent clip/transform/opacity apply? | **Yes** | Automatic via existing stacks. Shadow `Rect`s are ordinary `DrawOp::Quad`s and inherit everything for free. |
| Z-order vs. children | **Shadows behind fill/border/children** | Matches Flutter. |
| Multiple shadows | **Painted in list order** | First = back, last = front. Matches Flutter. |
| Hit testing | **Shadows excluded** | Existing `ContainerRenderObject::hit_test()` uses `computed_bounds` only. A tap in the shadow area (outside bounds) does not register. No change to hit-test code. |

### Caveat: shadow visible through transparent fill

If `background = None` or `background.a < 1`, the shadow's silhouette interior
is visible *behind* the container's content. This is correct — the shadow is a
real light-blocking shape, and a transparent container still casts a shadow.
Flutter behaves identically.

A developer who wants a "shadow only outside the bounds" effect should set
`background` to an opaque color, which occludes the shadow interior via
z-order.

## Shader Math

### Geometry

For a shadow with `blur_radius = B`, `spread_radius = S`, on a source rect of
size `W × H` with corner radius `R`:

- **Expanded rect** (what's actually drawn): size = `(W + 2(B+S)) × (H + 2(B+S))`
- **Silhouette** (the shape casting the shadow): size = `(W + 2S) × (H + 2S)`,
  corner radius = `R + S`, centered within the expanded rect.
- The silhouette's offset from the expanded rect's top-left = `(B+S, B+S)` —
  but since the silhouette is always centered, the shader derives it as
  `silhouette_size = rect_size - 2·blur_px`.

Only `shadow_color` and `shadow_blur` are new GPU inputs. The silhouette's
size, position, and corner radius are derivable from existing `size`,
`corner_radius` fields plus `shadow_blur`.

### Alpha falloff

Standard SDF-Gaussian shadow formula:

```wgsl
let sigma = max(blur_px * 0.5, 0.5);      // σ = blur/2 matches Flutter
let d = max(shadow_sdf, 0.0);              // clamp: alpha = 1 inside silhouette
let falloff = exp(-d * d / (2.0 * sigma * sigma));
let alpha = falloff * shadow_color.a;
```

`max(shadow_sdf, 0.0)` is critical — it keeps alpha = 1 throughout the
silhouette interior (where `sdf < 0`), so the shadow is fully opaque inside
the shape. The fill rect drawn on top occludes it.

This matches Flutter's Skia `MaskFilter.blur(BlurStyle.normal, blurSigma)`
where `blurSigma = blur_radius / 2`. At distance = `blur_radius`, alpha ≈
0.135 — visually indistinguishable from Flutter for typical UI radii.

### Fragment shader branch

```wgsl
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // === SHADOW PATH (when shadow_color.a > 0) ===
    if (in.shadow_color.a > 0.0) {
        let blur_px = in.shadow_blur * globals.scale_factor;
        let silhouette_size = max(in.size - vec2<f32>(2.0 * blur_px), vec2<f32>(0.0));
        let silhouette_half = silhouette_size * 0.5;
        let silhouette_radius = min(in.corner_radius,
                                    min(silhouette_size.x, silhouette_size.y) * 0.5);

        let pixel_pos = in.uv * in.size;
        let center_pos = pixel_pos - (in.size * 0.5);

        let inner_dist = abs(center_pos) - (silhouette_half - silhouette_radius);
        let corner_dist = length(max(inner_dist, vec2<f32>(0.0))) - silhouette_radius;
        let shadow_sdf = min(max(inner_dist.x, inner_dist.y), 0.0) + corner_dist;

        let sigma = max(blur_px * 0.5, 0.5);
        let d = max(shadow_sdf, 0.0);
        let falloff = exp(-d * d / (2.0 * sigma * sigma));
        return vec4<f32>(in.shadow_color.rgb, falloff * in.shadow_color.a);
    }

    // === EXISTING FILL/BORDER PATH (unchanged) ===
    let radius = min(in.corner_radius, min(in.size.x, in.size.y) * 0.5);
    // ... rest of existing shader unchanged ...
}
```

### Edge cases

| Case | Behavior |
|------|----------|
| `blur = 0` | `silhouette_size = rect_size`; SDF is sharp; Gaussian with σ=0.5 gives ~1px AA. Equivalent to a hard-edged offset rect. |
| `blur > min(W,H)/2` | `silhouette_size` clamped to ≥ 0; SDF degenerates to point-distance; result is a Gaussian blob. |
| `spread < 0` (inset spread) | Silhouette smaller than source; corner radius clamped to `≤ silhouette_half`. Produces a smaller shadow shape — valid use case. |
| `corner_radius = 0` | Silhouette SDF is a plain rectangle. |

## Error Handling

Vexo's existing style code does no input validation. Shadows follow the same
convention: **no validation, clamp in the shader or in `paint()`.**

| Input | Behavior |
|-------|----------|
| `blur_radius < 0` | Clamped to 0 in `ContainerRenderObject::paint()` before sending to GPU. One branch, cheaper than per-pixel clamp. |
| `spread_radius < 0` | Silhouette smaller than source. Shader clamps `silhouette_size ≥ 0`. Valid use case (Flutter allows it). |
| `offset` huge (shadow offscreen) | No special handling. GPU's own scissor / frustum culling handles it. |
| `color.a = 0` | Shadow skipped at paint time. No `DrawOp::Quad` emitted. |
| `blur = 0, spread = 0, offset = (0,0)` | Silhouette identical to source rect. Shadow is fully occluded by an opaque fill. For transparent fill, shadow is visible as a same-shape tinted overlay. Matches Flutter. |

### Invisible shadow optimization

A shadow with `color.a == 0` is skipped at paint time:

```rust
for shadow in &self.style.shadows {
    if shadow.color.a == 0.0 { continue; }   // skip fully-transparent
    // ... emit shadow Rect ...
}
```

This is the only filter. We do **not** skip shadows with `blur = 0` (sharp
shadows are legitimate) or shadows fully offscreen (checking bounds per shadow
would cost more than it saves).

### Stacking many shadows

No hard limit on `shadows.len()`. Each shadow is one `DrawOp::Quad`. For
pathological cases (100+ shadows on one container), the instance buffer grows
linearly — not a regression vs. existing behavior.

**Docstring guidance:** "Prefer 1-3 shadows per container for visual clarity
and performance. More than 8 shadows on a single container is rarely visually
justified."

### Change detection

`Style` derives `PartialEq`. `Vec<BoxShadow>` derives `PartialEq`. `BoxShadow`
derives `PartialEq`. So `ContainerRenderObject::set_style()` correctly detects
shadow list changes — including reordering, single-shadow edits, and full-list
replacement — and returns `true` → triggers `UpdateResult::PAINT`.

No new change-detection code required.

### Backward compatibility

- `Style` gains `shadows: Vec<BoxShadow>`. `Style::default()` produces
  `shadows: Vec::new()`. Existing `Style::new().background(...)` calls compile
  and behave identically.
- `QuadInstance` gains new fields, all defaulting to zero. Existing
  construction initializes shadow fields to zero. The shader's
  `shadow_color.a > 0` check correctly identifies these as non-shadow quads.
- `RenderCommand::Rect` gains new fields. Existing constructors
  (`RenderCommand::rect`, `rect_with_border`, `rounded_rect`) initialize
  shadow fields to zero.
- All existing `ContainerRenderObject::paint()` tests assert command counts;
  those counts are unchanged when `shadows` is empty.

**No existing test should break.**

## Testing Strategy

### Test layers

| Layer | What's tested | How |
|-------|---------------|-----|
| **Style API** | Builder ergonomics, append vs. replace, default values | Pure Rust unit tests on `Style`/`BoxShadow` |
| **Render object paint** | Correct command count, command order, shadow bounds math | `ContainerRenderObject::paint()` with `set_computed_bounds()`, assert on returned `Vec<RenderCommand>` |
| **FrameBuilder integration** | Shadow `Rect`s flow through `add_rect()` correctly | `process_commands()` + inspect `frame_builder.quad_instances()` |
| **Shader math** | Alpha falloff is correct at known points | Port the SDF+Gaussian formula to a Rust function; test that. The WGSL mirrors the Rust formula. (Cheaper than headless wgpu; catches math regressions.) |
| **Integration** | End-to-end widget tree → render → quads | Add a shadow showcase screen to `shared_app`; smoke-test via `cargo build` (existing convention — no automated GUI tests). |
| **Backward compat** | Existing tests still pass | `cargo test` — all existing paint-count tests must remain green. |

### Specific unit tests

**In `vexo/src/style.rs`:**

- `test_box_shadow_new` — color set, others default
- `test_box_shadow_builder_chain` — `.offset().blur().spread()`
- `test_box_shadow_default` — all fields default (transparent, zero)
- `test_style_shadow_appends` — `.shadow(s1).shadow(s2)` → len 2
- `test_style_shadows_replaces` — `.shadows(vec![s1, s2])` → len 2
- `test_style_shadow_default_empty` — `Style::default().shadows.is_empty()`
- `test_style_with_shadows_clone` — shadows survive `Clone`
- `test_style_with_shadows_eq` — `PartialEq` incl. shadows
- `test_style_shadow_does_not_overwrite_background` — `.background().shadow()` keeps both

**In `vexo/src/render_objects/container.rs`:**

- `test_container_paint_with_single_shadow` — bg + 1 shadow → 2 cmds;
  shadow bounds = expanded; shadow corner_radius = base + spread
- `test_container_paint_with_multiple_shadows` — bg + 3 shadows → 4 cmds in
  order: shadow, shadow, shadow, bg
- `test_container_paint_shadow_respects_offset` — shadow with offset (10, 20);
  bounds.left = base.left + 10 - pad, bounds.top = base.top + 20 - pad
- `test_container_paint_shadow_respects_blur_and_spread` — blur=12, spread=4;
  width = base + 2*(12+4), height = base + 2*(12+4)
- `test_container_paint_shadow_with_corner_radius` — bg + corner_radius(8) +
  shadow(spread=4); shadow corner_radius = 12
- `test_container_paint_shadow_skips_transparent_color` — shadow with
  `color.a = 0`; no shadow Rect emitted (cmds.len() == 1, just bg)
- `test_container_paint_shadow_negative_blur_clamped` — shadow with blur=-5;
  emitted shadow_blur = 0.0
- `test_container_paint_shadow_zero_blur_sharp` — shadow with blur=0; emitted
  with blur = 0 (sharp shadow)
- `test_container_paint_shadow_bypasses_self_clip` — bg + clip + shadow; no
  `PushClip` wraps the shadow Rect (PushClip appears after shadows, before bg)
- `test_container_paint_shadow_no_corner_radius_context` — corner_radius(8) +
  shadow; no `PushCornerRadius`/`PopCornerRadius` wraps the shadow
- `test_container_paint_shadows_do_not_affect_hit_test` — hit_test with point
  inside shadow area (outside `computed_bounds`) → false
- `test_container_set_style_detects_shadow_change` — set_style(style1 with 1
  shadow) → set_style(style2 with 2 shadows) → returns true
- `test_container_set_style_same_shadows_no_change` — set_style(style with
  shadow) → set_style(same style) → returns false
- `test_container_paint_shadow_no_background_still_emits_shadow` — no bg, 1
  shadow → 1 cmd (shadow only)

**In `vexo/src/frame_builder.rs` (or `command_processor.rs`):**

- `test_shadow_rect_produces_shadow_quad` — process_commands with shadow Rect;
  assert `quad.shadow_color != [0,0,0,0]` and `quad.shadow_blur == expected`
- `test_non_shadow_rect_produces_zero_shadow_fields` — process_commands with
  regular Rect; assert `quad.shadow_color == [0,0,0,0]` and
  `quad.shadow_blur == 0.0`

**Shader math port (in `vexo/src/render_objects/container.rs` or a new
`vexo/src/shadow_math.rs` test module):**

```rust
// Mirror of the shader's shadow alpha formula, for testing.
fn shadow_alpha(distance_from_silhouette: f32, blur_px: f32, alpha: f32) -> f32 {
    let sigma = (blur_px * 0.5).max(0.5);
    let d = distance_from_silhouette.max(0.0);
    let falloff = (-d * d / (2.0 * sigma * sigma)).exp();
    falloff * alpha
}
```

- `test_shadow_alpha_at_silhouette_edge` — distance = 0 → alpha = full alpha
- `test_shadow_alpha_at_blur_radius` — distance = blur_radius → alpha ≈ 0.135
  × alpha (matches Flutter)
- `test_shadow_alpha_at_zero_blur` — blur = 0, distance > 0 → alpha ≈ 0
  (sharp edge)
- `test_shadow_alpha_inside_silhouette` — distance < 0 → alpha = full alpha
  (interior is opaque)

### What's NOT tested automatically

- **Visual correctness of the WGSL shader** — verified by manual inspection
  via `cargo run -p desktop_demo` (existing convention per CLAUDE.md).
- **Performance** — no automated benchmark; the design's performance claims
  rely on the SDF approach being inherently cheap (1 extra quad per shadow,
  no new render passes).
- **iOS Metal rendering** — verified by building (`./build_for_ios.sh`), not
  by automated test.

### Demo update

Update `shared_app/src/lib.rs` to include a shadow showcase screen — e.g., a
card with a 12px blur shadow, an elevated button with a 4px shadow, and a
multi-shadow stack. This serves as the manual smoke test for both desktop and
iOS.

### Test-first ordering (TDD)

Per the framework's TDD convention, implementation follows red-green-refactor:

1. Write `style.rs` tests for `BoxShadow` → red → implement `BoxShadow` → green
2. Write `container.rs` paint tests → red → extend `paint()` → green
3. Write `command_processor`/`frame_builder` tests → red → extend `add_rect()`
   / `QuadInstance` → green
4. Write shader math port tests → red → implement formula → green
5. Update `shader.wgsl` → run `cargo build` → ask user to run demo → green
6. Update `shared_app` demo screen → ask user to run demo → green

## Open Questions

None. All decisions locked during brainstorming.

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| SDF approximation looks visibly different from Flutter at extreme blur radii (50+px) | Low (UI shadows rarely exceed 24px) | Documented as a known limitation; can be revisited if a real use case emerges. |
| `QuadInstance` size growth (80 → 96 bytes) affects instance buffer upload perf | Very low | 20% growth; existing pipeline handles buffer growth. No measured bottleneck today. |
| Shader compile error on iOS Metal | Low | WGSL is cross-compiled to MSL via Naga; SDF+Gaussian uses only standard ops. Caught at `cargo test` time. |
| Existing tests break due to new fields | Very low | All new fields default to zero; existing constructors initialize them. Existing paint-count assertions unchanged for empty `shadows`. |

## Success Criteria

- All new unit tests pass (`cargo test`).
- All existing tests continue to pass (`cargo test`).
- `cargo build -p vexo --release` succeeds.
- `./build_for_ios.sh` succeeds.
- Manual smoke test: shadow showcase screen renders correctly on desktop
  (user runs `cargo run -p desktop_demo`) and iOS (user runs via Xcode).
- Visual quality matches Flutter's `BoxShadow` at typical UI radii (4-24px
  blur).
