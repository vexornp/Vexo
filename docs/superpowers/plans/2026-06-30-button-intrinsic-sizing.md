# Button Intrinsic Sizing & Padding Fix — Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Each task ends with a commit.

**Goal:** Fix two visible bugs in `vexo_uikit::Button` — (1) buttons stretch to fill the parent Column's width instead of sizing to their text content, and (2) the button label sits flush against the left edge of the background with no leading padding.

**Architecture:** Restructure `Button::render()` so visual decoration (background, padding, border, corner_radius) lives on a `DecoratedContainer` wrapping a plain `Text` leaf, and add `align_self(Start)` as the outermost modifier to break the Column's `AlignItems::Stretch` cascade. Make `DecoratedContainer` public so `vexo_uikit` can construct it directly.

**Tech Stack:** Rust, Taffy, vexo three-tree architecture

**Spec:** `docs/superpowers/specs/2026-06-30-button-intrinsic-sizing-design.md`

## Global Constraints

- `Text` becomes a pure leaf in `Button::render()` — no `background`/`padding`/`border`/`corner_radius` modifiers on it.
- All visual decoration moves to `DecoratedContainer`, which already defaults to `align_self(Start).flex_shrink(0.0)`.
- `.align_self(Start)` is applied **last** in the builder chain so it wraps the outermost `WithLayout` — this is critical because the pass-through wrappers (`GestureDetector`, `MouseRegion`, `Opacity`) all default to `Column + AlignItems::Stretch` internally.
- Token values (Desktop H=16/V=8, Mobile H=20/V=12) are unchanged.
- `GestureDetector` and `MouseRegion` stay `pub(crate)` — `Button` reaches them via the `Widget::on_press`/`on_enter`/etc. trait methods.
- Per `CLAUDE.md`: never run `cargo run -p desktop_demo` — ask the user to run it for visual verification.

**Dependency order:** Task 1 (make `DecoratedContainer` public) must complete before Task 2 (restructure `Button::render()` to use it). Task 3 (unit test) depends on Task 2. Task 4 (full verification) depends on all. Recommended order: 1 → 2 → 3 → 4.

---

### Task 1: Make `DecoratedContainer` public

**Files:**
- Modify: `vexo/src/widgets/mod.rs:37`
- Modify: `vexo/src/lib.rs:189-192`

**Interfaces:**
- Produces: `vexo::DecoratedContainer` as a public type re-exported from the crate root.

- [ ] **Step 1: Change visibility in `vexo/src/widgets/mod.rs`**

At line 37, change:
```rust
pub(crate) use decorated_container::DecoratedContainer;
```
to:
```rust
pub use decorated_container::DecoratedContainer;
```

- [ ] **Step 2: Add `DecoratedContainer` to the crate-root re-export in `vexo/src/lib.rs`**

At lines 189-192, change:
```rust
pub use widgets::{
    Column, Flex, Grid, Image, Opacity, Row, ScrollView, Text, TextEdit, TextEditState,
    TextEditingController, Widget,
};
```
to:
```rust
pub use widgets::{
    Column, DecoratedContainer, Flex, Grid, Image, Opacity, Row, ScrollView, Text, TextEdit,
    TextEditState, TextEditingController, Widget,
};
```

- [ ] **Step 3: Run vexo build to verify no breakage**

Run: `cargo build -p vexo`
Expected: SUCCESS (visibility widening is backward-compatible).

- [ ] **Step 4: Run vexo tests to verify no regressions**

Run: `cargo test -p vexo`
Expected: ALL PASS — `DecoratedContainer` tests in `vexo/src/widgets/decorated_container.rs` are unaffected by the visibility change.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat: make DecoratedContainer public for vexo_uikit consumption"
```

---

### Task 2: Restructure `Button::render()` to compose container + text

**Files:**
- Modify: `vexo_uikit/src/button.rs:1-7` (imports)
- Modify: `vexo_uikit/src/button.rs:194-247` (render body)

**Interfaces:**
- Consumes: `vexo::DecoratedContainer`, `vexo::AlignSelf` (both public after Task 1; `AlignSelf` already public at `vexo/src/lib.rs:49`)
- Produces: restructured `Button::render()` returning the widget tree described in the spec.

- [ ] **Step 1: Update imports in `vexo_uikit/src/button.rs`**

At lines 1-7, change:
```rust
use vexo::{Color, Component, ComponentState, RenderContext, Signal, Text, Widget};
```
to:
```rust
use vexo::{
    AlignSelf, Color, Component, ComponentState, DecoratedContainer, RenderContext, Signal, Text,
    Widget,
};
```

- [ ] **Step 2: Replace the `Button::render()` body**

Replace lines 194-247 (the entire `fn render` method) with:

```rust
    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_pressed = state.is_pressed.get();
        let is_hovered = state.is_hovered.get();

        let bg = self.resolve_bg(is_pressed, is_hovered);
        let (border_color, border_width) = self.resolve_border();
        // TODO: apply text_color when vexo Text widget supports text color
        let _text_color = self.resolve_text_color(is_hovered);
        let corner_radius = self.resolve_corner_radius();
        let (pt, pr, pb, pl) = self.resolve_padding();
        let opacity = if self.disabled {
            tokens::button::DISABLED_OPACITY
        } else {
            1.0
        };

        let disabled = self.disabled;
        let on_press_cb = self.on_press.clone();
        let is_pressed_signal = state.is_pressed.clone();
        let is_pressed_signal_release = state.is_pressed.clone();
        let is_pressed_signal_exit = state.is_pressed.clone();
        let is_hovered_signal = state.is_hovered.clone();
        let is_hovered_signal_exit = state.is_hovered.clone();

        // Plain leaf — no modifiers on Text itself.
        // with_font_size(24.0) is explicit to preserve current behavior
        // (Text::new() defaults to 24.0); insulates Button from future
        // changes to Text's default.
        let text = Text::new(&self.label).with_font_size(24.0);

        // All decoration on the container. DecoratedContainer defaults to
        // align_self(Start).flex_shrink(0.0), so the container sizes to its
        // content (text intrinsic width + padding + border).
        let mut container = DecoratedContainer::new(text)
            .background(bg)
            .corner_radius(corner_radius)
            .padding_each(pt, pr, pb, pl);

        if border_width > 0.0 {
            container = container.border(border_color, border_width);
        }

        container
            .boxed()
            .on_press(move || {
                if !disabled {
                    is_pressed_signal.set(true);
                    (on_press_cb.borrow_mut())();
                }
            })
            .on_release(move || {
                is_pressed_signal_release.set(false);
            })
            .on_enter(move || {
                if !disabled {
                    is_hovered_signal.set(true);
                }
            })
            .on_exit(move || {
                is_hovered_signal_exit.set(false);
                is_pressed_signal_exit.set(false);
            })
            .opacity(opacity)
            .align_self(AlignSelf::Start)
    }
```

**Verification points for this step:**
- `padding_each(pt, pr, pb, pl)` keeps the same TRBL call signature — `DecoratedContainer` inherits `padding_each` from `layout_builder_methods!()` (`vexo/src/widgets/decorated_container.rs:329`).
- `.align_self(Start)` is applied **after** `.opacity(opacity)` so it wraps the outermost `WithLayout`. Order matters.
- The callback bodies are identical to the current implementation — only the widget structure around them changes.
- `_text_color` remains unused (Text has no text color API yet) — out of scope per spec.

- [ ] **Step 3: Build vexo_uikit to verify compilation**

Run: `cargo build -p vexo_uikit`
Expected: SUCCESS

If the build fails with "cannot find type `DecoratedContainer` in crate `vexo`", Task 1 was not completed — go back and verify the visibility change.

- [ ] **Step 4: Build the whole workspace to catch any downstream breakage**

Run: `cargo build`
Expected: SUCCESS

- [ ] **Step 5: Run vexo_uikit tests**

Run: `cargo test -p vexo_uikit`
Expected: ALL PASS (the `press()` method and public API are unchanged).

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/button.rs
git commit -m "fix: restructure Button to compose DecoratedContainer + Text leaf

Moves visual decoration (background, padding, border, corner_radius) off
the Text leaf and onto a DecoratedContainer wrapping it. Adds
align_self(Start) as the outermost modifier to break the Column's
AlignItems::Stretch cascade through the pass-through wrappers.

Fixes: buttons stretching to column width, and text sitting flush against
the left edge of the button background with no visible padding."
```

---

### Task 3: Add unit test for the Button widget tree shape

**Files:**
- Modify: `vexo_uikit/src/button.rs` (add `#[cfg(test)] mod tests` at end of file)

**Interfaces:**
- Consumes: `Button`, `ButtonVariant`, public `vexo::DecoratedContainer`, public `Widget::child()` / `Widget::as_any()` for tree traversal.

- [ ] **Step 1: Write the failing test**

Append to `vexo_uikit/src/button.rs` (after the `impl Component for Button` block):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use vexo::{
        AlignSelf, DecoratedContainer, GestureDetector, MouseRegion, Opacity, Text, Widget,
        WithLayout,
    };
    use vexo::layout::EdgeInsets;
    use vexo_uikit::platform::Platform;

    /// Walk down a single-child widget chain by downcasting to an expected
    /// concrete type and returning its child. Returns None if the type
    /// doesn't match or the widget has no child.
    fn peel<T: 'static>(w: &dyn Widget) -> Option<&dyn Widget> {
        if w.as_any().downcast_ref::<T>().is_some() {
            w.child()
        } else {
            None
        }
    }

    /// Same as `peel` but for `Box<dyn Widget>` (which delegates to the
    /// inner widget's methods via the blanket impl).
    fn peel_boxed<T: 'static>(w: &Box<dyn Widget>) -> Option<&dyn Widget> {
        peel::<T>(w.as_ref())
    }

    #[test]
    fn test_button_tree_structure_primary() {
        // Force Desktop platform so token values are deterministic.
        let _ = Platform::current; // no-op; just confirm symbol is in scope
        // Note: Platform::current is a function; Button calls it internally.
        // We assert Desktop token values directly below.

        let button = Button::new("Submit").variant(ButtonVariant::Primary);
        let mut state = ButtonState::default();
        let mut ctx = RenderContext::new();
        let tree = button.render(&mut state, &mut ctx);

        // Outermost: WithLayout with align_self == Some(Start)
        let outer = tree.as_any().downcast_ref::<WithLayout>().expect(
            "outermost widget should be WithLayout(align_self=Start)"
        );
        assert_eq!(
            outer.layout().align_self,
            Some(AlignSelf::Start),
            "outermost WithLayout must set align_self=Start to break Column stretch"
        );

        // WithLayout -> Opacity -> MouseRegion(on_exit) -> MouseRegion(on_enter)
        // -> GestureDetector(on_release) -> GestureDetector(on_press)
        // -> DecoratedContainer -> Text
        let w = peel_boxed::<Opacity>(&outer.child().unwrap().boxed()).expect("expected Opacity");
        let w = peel::<MouseRegion>(w).expect("expected MouseRegion (on_exit layer)");
        let w = peel::<MouseRegion>(w).expect("expected MouseRegion (on_enter layer)");
        let w = peel::<GestureDetector>(w).expect("expected GestureDetector (on_release layer)");
        let w = peel::<GestureDetector>(w).expect("expected GestureDetector (on_press layer)");

        let dc = w.as_any().downcast_ref::<DecoratedContainer>().expect(
            "expected DecoratedContainer carrying the visual decoration"
        );

        // Background should be the Primary token (not None — that was the old
        // bug where decoration was on Text).
        assert_eq!(dc.style_ref().background, Some(tokens::button::PRIMARY_BG));

        // Padding should be the Desktop token values (TRBL -> EdgeInsets is
        // left/right/top/bottom). Button::resolve_padding returns
        // (PADDING_V_DESKTOP, PADDING_H_DESKTOP, PADDING_V_DESKTOP, PADDING_H_DESKTOP)
        // in TRBL order, and padding_each(top, right, bottom, left) delegates
        // to Layout::padding_each(left, right, top, bottom).
        let padding = dc.layout_ref().padding.expect("DecoratedContainer should have padding");
        assert_eq!(padding.top, tokens::button::PADDING_V_DESKTOP);
        assert_eq!(padding.bottom, tokens::button::PADDING_V_DESKTOP);
        assert_eq!(padding.left, tokens::button::PADDING_H_DESKTOP);
        assert_eq!(padding.right, tokens::button::PADDING_H_DESKTOP);

        // The Text leaf should be a pure leaf — no background of its own.
        let text = dc.child().as_any().downcast_ref::<Text>().expect(
            "DecoratedContainer's child should be a Text leaf"
        );
        // Text has no public accessor for style.background; we verify it's
        // a Text instance (the structure is what matters). The old bug put
        // background on Text; now Text has none.
        assert_eq!(text.content(), "Submit");
    }

    #[test]
    fn test_button_tree_structure_secondary_has_border() {
        let button = Button::new("Cancel").variant(ButtonVariant::Secondary);
        let mut state = ButtonState::default();
        let mut ctx = RenderContext::new();
        let tree = button.render(&mut state, &mut ctx);

        // Walk to the DecoratedContainer.
        let outer = tree.as_any().downcast_ref::<WithLayout>().expect("outermost WithLayout");
        let mut w: &dyn Widget = outer.child().unwrap();
        w = peel::<Opacity>(w).expect("Opacity");
        w = peel::<MouseRegion>(w).expect("MouseRegion");
        w = peel::<MouseRegion>(w).expect("MouseRegion");
        w = peel::<GestureDetector>(w).expect("GestureDetector");
        w = peel::<GestureDetector>(w).expect("GestureDetector");
        let dc = w.as_any().downcast_ref::<DecoratedContainer>().expect("DecoratedContainer");

        // Secondary variant has a 1px border.
        let border = dc.style_ref().border.expect("Secondary should have a border");
        assert_eq!(border.color, tokens::button::SECONDARY_BORDER);
        assert_eq!(border.width, 1.0);

        // Background is transparent for Secondary.
        assert_eq!(dc.style_ref().background, Some(tokens::button::SECONDARY_BG));
    }

    #[test]
    fn test_button_disabled_uses_opacity() {
        let button = Button::new("Submit").variant(ButtonVariant::Primary).disabled(true);
        let mut state = ButtonState::default();
        let mut ctx = RenderContext::new();
        let tree = button.render(&mut state, &mut ctx);

        // Walk to the Opacity layer and verify its value.
        let outer = tree.as_any().downcast_ref::<WithLayout>().expect("outermost WithLayout");
        let op = peel::<Opacity>(outer.child().unwrap()).expect("Opacity");
        assert_eq!(op.opacity_value(), tokens::button::DISABLED_OPACITY);
    }
}
```

**Note on test helpers:** `peel` / `peel_boxed` traverse the single-child chain by downcasting. This works because `Widget::child()` and `Widget::as_any()` are public, and after Task 1 `DecoratedContainer` is downcastable from `vexo_uikit`. The chain order (Opacity → MouseRegion ×2 → GestureDetector ×2 → DecoratedContainer → Text) is determined by the order of `.on_press` / `.on_release` / `.on_enter` / `.on_exit` / `.opacity` / `.align_self` calls in `Button::render()`.

**Caveat — API surface check:** Before finalizing the test, verify that `WithLayout::layout()`, `DecoratedContainer::style_ref()`, `DecoratedContainer::layout_ref()`, `DecoratedContainer::child()`, and `Opacity::opacity_value()` are accessible from outside the `vexo` crate. Run `cargo doc -p vexo --no-deps` and search if needed. If any accessor is missing, either (a) add a public accessor method in `vexo`, or (b) adjust the test to assert only what's publicly observable. Prefer (a) for accessors that are genuinely useful for downstream testing.

- [ ] **Step 2: Run the test to verify it fails (or compiles and passes)**

Run: `cargo test -p vexo_uikit test_button_tree_structure`
Expected: PASS (Task 2 already implemented the structure).

If it FAILS with a compilation error (missing accessor), resolve per the caveat above. If it FAILS with an assertion error, the structure does not match the spec — re-examine `Button::render()` from Task 2.

- [ ] **Step 3: Run all vexo_uikit tests**

Run: `cargo test -p vexo_uikit`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add vexo_uikit/src/button.rs
git commit -m "test: add Button widget tree shape tests

Asserts the restructured tree: WithLayout(align_self=Start) > Opacity >
MouseRegion ×2 > GestureDetector ×2 > DecoratedContainer > Text. Verifies
decoration lives on the container (not the Text leaf) and that disabled
state still applies opacity."
```

---

### Task 4: Full workspace verification

**Files:**
- No new files.

- [ ] **Step 1: Run full workspace build**

Run: `cargo build`
Expected: SUCCESS

- [ ] **Step 2: Run full workspace tests**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 3: Verify the demo compiles**

Run: `cargo build -p shared_app`
Expected: SUCCESS (the demo uses `Button` unchanged from the call site's perspective — no API change).

- [ ] **Step 4: Ask the user to run the demo for visual verification**

Per `CLAUDE.md`, do **not** run `cargo run -p desktop_demo` yourself. Ask the user:

> Please run `cargo run -p desktop_demo` and confirm:
> 1. Buttons size to their text content + padding (not full column width).
> 2. "Submit" is visibly wider than "More" (different label lengths produce different button widths).
> 3. Leading padding is visible between the text and the left edge of the background.
> 4. Trailing padding is also visible (text is left-aligned within the padded box, not flush-right).
> 5. Hover/press color changes only trigger when the pointer is inside the visible button bounds — not in the empty column space beside it.
> 6. The disabled "Submit" button still renders faded and does not respond to clicks.

Wait for the user's confirmation. If any check fails, return to the relevant task and debug. Do not mark this task complete until all six checks pass.

- [ ] **Step 5: Final commit (if any fixup was needed)**

If Steps 1-4 required no fixups, this step is a no-op. Otherwise, commit the fixups with a descriptive message.

---

## Out of scope (per spec)

- **Text color wiring** — `resolve_text_color` still computes an unused value; `vexo::Text` has no text color API yet.
- **Fixing `TextRenderObject` to honor `layout.padding`** — this design avoids depending on that behavior by moving padding to `DecoratedContainer`.
- **Changing pass-through wrapper defaults** (`GestureDetector`/`MouseRegion`/`Opacity` render objects still default to `Column + AlignItems::Stretch`).
- **Button font size** — preserved at 24.0 explicitly.
- **Platform token values** — Desktop H=16/V=8 and Mobile H=20/V=12 unchanged.
