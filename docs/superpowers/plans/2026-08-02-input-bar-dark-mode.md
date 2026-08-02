# Chat Input Bar Dark Mode Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the chat input bar's `TextEdit` adapt to dark mode by exposing color setters on `TextEdit` and passing theme-derived colors from `build_input_bar`.

**Architecture:** Add a `color` field through the `TextEditContent` → `TextEditRenderObject` chain (currently the glyph color is hardcoded `Color::BLACK` at `vexo/src/render_objects/text_edit.rs:321`). Add three `Option<Color>` fields with builder methods to the `TextEdit` widget (`with_background`, `with_text_color`, `with_border_color`); `render()` uses `unwrap_or` to preserve today's hardcoded defaults when no builder is called. Focus state keeps the caller's border color and changes only width (1 → 2). `build_input_bar` in `shared_app/src/chats/chat_screen.rs` takes `&ThemeData` and passes `theme.surface` / `theme.on_surface` / `theme.outline`.

**Tech Stack:** Rust, Vexo three-tree framework, Taffy layout, glyphon text rendering. Tests use `ThreeTreePipeline` + tree-walk assertions (existing pattern at `shared_app/src/chats/chat_screen.rs:335-364` and `vexo/src/widgets/text_edit.rs:854-866`).

## Global Constraints

- No new crates, no new public types, no new modules. All changes are additive fields/methods on existing structs plus one signature change to `build_input_bar`.
- Default behavior must be preserved: a bare `TextEdit::new(controller)` with no `.with_*` calls renders exactly today's white background / black text / gray border. This is enforced by `Option::unwrap_or` fallbacks.
- `TextEdit` stays theme-agnostic — it does NOT read `Theme::of(ctx)`. Callers pass colors. This matches the `Text` widget pattern (`shared_app/src/chats/chat_screen.rs:183-187`).
- Focus state changes border **width** (1 → 2), not border **color**. The focused-color branch (`vexo/src/widgets/text_edit.rs:584-588`) is removed.
- Cursor color (`CURSOR_COLOR` at `vexo/src/render_objects/text_edit.rs:18`) stays hardcoded — out of scope.
- Per `CLAUDE.md`: run `cargo build` after every edit and `cargo test` after every feature. Never run `cargo run -p desktop_demo` — ask the user for visual smoke test.
- No comments added to code unless explicitly requested by a step.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `vexo/src/render_objects/text_edit.rs` | Paints text glyphs + cursor | Add `color: Color` field, `with_color`/`set_color`/`color()`; `paint()` reads it instead of `Color::BLACK` |
| `vexo/src/widgets/text_edit_content.rs` | Leaf widget carrying text edit config | Add `color: Color` field, `with_color`/`color()`; update `Clone`, `create_render_object`, `update_render_object` |
| `vexo/src/widgets/text_edit.rs` | `TextEdit` Component widget | Add 3 `Option<Color>` fields + builders; `render()` uses them with `unwrap_or` defaults; remove focused-color branch |
| `vexo/src/render_objects/decorated_box.rs` | Paints `Style` (bg/border) | Add `pub fn style(&self) -> &Style` accessor (needed for tests) |
| `shared_app/src/chats/chat_screen.rs` | Chat screen + input bar | `build_input_bar` takes `&ThemeData`, passes colors; call site at `:146` updated; integration test added |

---

## Task 1: Add `color` field to `TextEditRenderObject`

**Files:**
- Modify: `vexo/src/render_objects/text_edit.rs` (struct at `:43-54`, `new` at `:58-68`, `paint` at `:199-345`, test module at `:403-649`)
- Test: `vexo/src/render_objects/text_edit.rs` test module

**Interfaces:**
- Consumes: nothing (leaf change)
- Produces: `TextEditRenderObject::with_color(Color) -> Self`, `set_color(&mut self, Color) -> bool`, `color() -> Color`. Default `Color::BLACK`. `paint()` emits `RenderCommand::Text { color: self.color, ... }` instead of `Color::BLACK`.

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/render_objects/text_edit.rs` test module (after `test_text_edit_render_object_set_cursor_blink_visible` at `:484`):

```rust
    #[test]
    fn test_text_edit_render_object_default_color_is_black() {
        let editor = create_test_editor();
        let obj = TextEditRenderObject::new("Hello", editor);
        assert_eq!(obj.color(), Color::BLACK);
    }

    #[test]
    fn test_text_edit_render_object_with_color_builder() {
        let editor = create_test_editor();
        let obj = TextEditRenderObject::new("Hello", editor)
            .with_color(Color::rgb(0.9, 0.1, 0.1));
        assert_eq!(obj.color(), Color::rgb(0.9, 0.1, 0.1));
    }

    #[test]
    fn test_text_edit_render_object_set_color_returns_changed() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor);
        assert!(obj.set_color(Color::WHITE));
        assert_eq!(obj.color(), Color::WHITE);
        assert!(!obj.set_color(Color::WHITE));
    }

    #[test]
    fn test_text_edit_render_object_paint_uses_color_field() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor)
            .with_color(Color::rgb(0.9, 0.1, 0.1));
        obj.set_focused(false);
        obj.set_cursor_blink_visible(false);

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        {
            let mut layout_ctx = LayoutContext::new(&mut engine, &mut font_system);
            let _ = obj.layout(&mut layout_ctx, &[]);
        }
        let root = engine.create_leaf(&Layout::default());
        engine.compute(root, Size::new(200.0, 50.0), &mut font_system);
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&mut ctx);
        }

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        let text_cmd = result
            .iter()
            .find_map(|c| match c {
                RenderCommand::Text { color, .. } => Some(*color),
                _ => None,
            })
            .expect("should emit a Text command");
        assert_eq!(text_cmd, Color::rgb(0.9, 0.1, 0.1));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib render_objects::text_edit::tests::test_text_edit_render_object_default_color_is_black 2>&1 | tail -20`
Expected: FAIL with "no method named `color` found" or "no method named `with_color` found" (compile error).

- [ ] **Step 3: Add the `color` field and accessors to `TextEditRenderObject`**

In `vexo/src/render_objects/text_edit.rs`, edit the struct definition at `:43-54` to add a `color` field. The struct currently is:

```rust
pub struct TextEditRenderObject {
    // Text fields (same as TextRenderObject)
    content: String,
    font_size: f32,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,

    // Cursor fields
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
}
```

Change to:

```rust
pub struct TextEditRenderObject {
    // Text fields (same as TextRenderObject)
    content: String,
    font_size: f32,
    color: Color,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,

    // Cursor fields
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
}
```

In `new` at `:58-68`, add `color: Color::BLACK,` after `font_size: 16.0,`:

```rust
    pub fn new(content: &str, editor: Rc<RefCell<Editor>>) -> Self {
        Self {
            content: content.to_string(),
            font_size: 16.0,
            color: Color::BLACK,
            computed_bounds: None,
            layout_node: None,
            editor,
            is_focused: false,
            cursor_blink_visible: false,
        }
    }
```

Add `with_color` builder right after `with_font_size` at `:71-74`:

```rust
    /// Set the text glyph color (builder pattern).
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
```

Add `color()` accessor right after `font_size()` at `:82-84`:

```rust
    /// Get the text glyph color.
    pub fn color(&self) -> Color {
        self.color
    }
```

Add `set_color` setter right after `set_font_size` at `:112-122`:

```rust
    /// Set the text glyph color.
    ///
    /// Returns true if the color changed.
    pub fn set_color(&mut self, color: Color) -> bool {
        let changed = self.color != color;
        if changed {
            self.color = color;
        }
        changed
    }
```

- [ ] **Step 4: Make `paint()` use `self.color` instead of `Color::BLACK`**

In `vexo/src/render_objects/text_edit.rs:321`, change:

```rust
            color: Color::BLACK,
```

to:

```rust
            color: self.color,
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vexo --lib render_objects::text_edit 2>&1 | tail -20`
Expected: PASS — all existing tests plus the 4 new ones.

- [ ] **Step 6: Build the whole crate**

Run: `cargo build -p vexo 2>&1 | tail -10`
Expected: PASS, no warnings about the new field.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/render_objects/text_edit.rs
git commit -m "feat(text_edit_ro): add color field to TextEditRenderObject

Replaces the hardcoded Color::BLACK glyph color at paint() :321 with a
settable field. Defaults to Color::BLACK so behavior is unchanged for
existing callers. Plumbs the way for TextEditContent to pass a color
through."
```

---

## Task 2: Add `color` field to `TextEditContent`

**Files:**
- Modify: `vexo/src/widgets/text_edit_content.rs` (struct at `:22-29`, `new` at `:33-42`, `Clone` at `:89-100`, `create_render_object` at `:113-119`, `update_render_object` at `:125-149`, test module at `:156-294`)
- Test: `vexo/src/widgets/text_edit_content.rs` test module

**Interfaces:**
- Consumes: `TextEditRenderObject::with_color(Color)` and `set_color(&mut self, Color) -> bool` from Task 1
- Produces: `TextEditContent::with_color(Color) -> Self`, `color() -> Color`. Default `Color::BLACK`. `create_render_object` passes color via `.with_color(...)`. `update_render_object` syncs color changes (paint-only invalidation).

- [ ] **Step 1: Write the failing tests**

Add to `vexo/src/widgets/text_edit_content.rs` test module (after `test_text_edit_content_with_cursor_blink_visible` at `:215`):

```rust
    #[test]
    fn test_text_edit_content_default_color_is_black() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor);
        assert_eq!(widget.color(), Color::BLACK);
    }

    #[test]
    fn test_text_edit_content_with_color() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor)
            .with_color(Color::rgb(0.9, 0.1, 0.1));
        assert_eq!(widget.color(), Color::rgb(0.9, 0.1, 0.1));
    }

    #[test]
    fn test_text_edit_content_clone_preserves_color() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor)
            .with_color(Color::WHITE);
        let cloned = widget.clone();
        assert_eq!(widget.color(), cloned.color());
    }

    #[test]
    fn test_text_edit_content_create_render_object_carries_color() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor)
            .with_color(Color::rgb(0.9, 0.1, 0.1));
        let ro = widget.create_render_object();
        let any_ro = ro.as_any();
        let te_ro = any_ro
            .downcast_ref::<TextEditRenderObject>()
            .expect("should be a TextEditRenderObject");
        assert_eq!(te_ro.color(), Color::rgb(0.9, 0.1, 0.1));
    }

    #[test]
    fn test_text_edit_content_update_render_object_color_change() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor)
            .with_color(Color::WHITE);
        let mut ro = TextEditRenderObject::new("Hello", create_test_editor());
        ro.set_font_size(24.0);

        let result = widget.update_render_object(&mut ro);
        assert!(
            result.contains(UpdateResult::PAINT),
            "color change should request paint"
        );
        assert_eq!(ro.color(), Color::WHITE);
    }
```

Add this import at the top of the test module if `Color` is not already imported (the test module currently `use super::*;` so `Color` resolves via `crate::core::Color` re-exports — verify by running the test). If the test fails with "Color not found", add `use crate::core::Color;` inside the test module.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib widgets::text_edit_content::tests::test_text_edit_content_default_color_is_black 2>&1 | tail -20`
Expected: FAIL with "no method named `color` found" or "no method named `with_color` found" (compile error).

- [ ] **Step 3: Add the `color` field, builder, and accessor to `TextEditContent`**

In `vexo/src/widgets/text_edit_content.rs`, edit the struct at `:22-29`. Currently:

```rust
pub struct TextEditContent {
    key: Option<WidgetKey>,
    content: String,
    font_size: f32,
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
}
```

Change to:

```rust
pub struct TextEditContent {
    key: Option<WidgetKey>,
    content: String,
    font_size: f32,
    color: Color,
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
}
```

Add `use crate::core::Color;` to the imports at the top of the file (after `use crate::{Element, RenderObject, UpdateResult, Widget};` at `:14`).

In `new` at `:33-42`, add `color: Color::BLACK,` after `font_size: 24.0,`:

```rust
    pub fn new(content: impl Into<String>, editor: Rc<RefCell<Editor>>) -> Self {
        Self {
            key: None,
            content: content.into(),
            font_size: 24.0,
            color: Color::BLACK,
            editor,
            is_focused: false,
            cursor_blink_visible: false,
        }
    }
```

Add `with_color` builder right after `with_font_size` at `:51-54`:

```rust
    /// Set the text glyph color.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
```

Add `color()` accessor right after `font_size()` at `:74-76`:

```rust
    /// Get the text glyph color.
    pub fn color(&self) -> Color {
        self.color
    }
```

- [ ] **Step 4: Update `Clone` impl to include `color`**

In `vexo/src/widgets/text_edit_content.rs:89-100`, the `Clone` impl is:

```rust
impl Clone for TextEditContent {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
            font_size: self.font_size,
            editor: self.editor.clone(),
            is_focused: self.is_focused,
            cursor_blink_visible: self.cursor_blink_visible,
        }
    }
}
```

Change to:

```rust
impl Clone for TextEditContent {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
            font_size: self.font_size,
            color: self.color,
            editor: self.editor.clone(),
            is_focused: self.is_focused,
            cursor_blink_visible: self.cursor_blink_visible,
        }
    }
}
```

- [ ] **Step 5: Update `create_render_object` to pass color through**

In `vexo/src/widgets/text_edit_content.rs:113-119`. Currently:

```rust
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        let mut ro = TextEditRenderObject::new(&self.content, self.editor.clone())
            .with_font_size(self.font_size);
        ro.set_focused(self.is_focused);
        ro.set_cursor_blink_visible(self.cursor_blink_visible);
        Box::new(ro)
    }
```

Change to:

```rust
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        let mut ro = TextEditRenderObject::new(&self.content, self.editor.clone())
            .with_font_size(self.font_size)
            .with_color(self.color);
        ro.set_focused(self.is_focused);
        ro.set_cursor_blink_visible(self.cursor_blink_visible);
        Box::new(ro)
    }
```

- [ ] **Step 6: Update `update_render_object` to sync color changes**

In `vexo/src/widgets/text_edit_content.rs:125-149`. Currently the body checks content, font_size, focused, cursor_blink_visible. Add a color check after the font_size check. The relevant block is:

```rust
            if ro.set_font_size(self.font_size) {
                result |= UpdateResult::LAYOUT;
            }
            if ro.set_focused(self.is_focused) {
                result |= UpdateResult::PAINT;
            }
```

Insert a color check between these two blocks:

```rust
            if ro.set_font_size(self.font_size) {
                result |= UpdateResult::LAYOUT;
            }
            if ro.set_color(self.color) {
                result |= UpdateResult::PAINT;
            }
            if ro.set_focused(self.is_focused) {
                result |= UpdateResult::PAINT;
            }
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::text_edit_content 2>&1 | tail -20`
Expected: PASS — all existing tests plus the 5 new ones.

- [ ] **Step 8: Build the whole crate**

Run: `cargo build -p vexo 2>&1 | tail -10`
Expected: PASS, no warnings.

- [ ] **Step 9: Commit**

```bash
git add vexo/src/widgets/text_edit_content.rs
git commit -m "feat(text_edit_content): add color field plumbed to render object

TextEditContent now carries a color (default BLACK) and passes it to
TextEditRenderObject via with_color() at create time and set_color() at
update time. Color change is paint-only invalidation. Behavior unchanged
for callers that don't set the color."
```

---

## Task 3: Add 3 color builders to `TextEdit` widget; refactor `render()`

**Files:**
- Modify: `vexo/src/widgets/text_edit.rs` (struct at `:551-555`, `new` at `:559-564`, `render` at `:581-608`, test module at `:620-1245`)
- Modify: `vexo/src/render_objects/decorated_box.rs` (add `style()` accessor after `set_style` at `:50-57`)
- Test: `vexo/src/widgets/text_edit.rs` test module

**Interfaces:**
- Consumes: `TextEditContent::with_color(Color)` from Task 2
- Produces: `TextEdit::with_background(Color)`, `with_text_color(Color)`, `with_border_color(Color)` — each `-> Self`, each sets an `Option<Color>` field. `render()` reads them via `unwrap_or` with today's hardcoded defaults. The focused-color branch is removed; focus changes only border width.

- [ ] **Step 1: Add `style()` accessor to `DecoratedBoxRenderObject`**

In `vexo/src/render_objects/decorated_box.rs`, immediately after the `set_style` method at `:50-57`, add:

```rust
    /// Get the style (read accessor, used by tests).
    pub fn style(&self) -> &Style {
        &self.style
    }
```

This is a one-line public accessor needed by the tests in Step 5 and Task 4. It exposes the existing private `style` field without changing behavior.

- [ ] **Step 2: Write the failing tests**

Add to `vexo/src/widgets/text_edit.rs` test module. Place these after the existing `test_text_edit_clone` test at `:835` (before the pipeline integration tests section at `:845`):

```rust
    use crate::core::Color;
    use crate::render_objects::DecoratedBoxRenderObject;
    use crate::RenderObjectKey;

    fn find_render_object_in_tree(
        reg: &crate::RenderObjectRegistry,
        key: RenderObjectKey,
        predicate: &dyn Fn(&dyn RenderObject) -> bool,
    ) -> Option<RenderObjectKey> {
        let ro = reg.get(key)?;
        if predicate(ro.as_ref()) {
            return Some(key);
        }
        for &child in ro.children() {
            if let Some(found) = find_render_object_in_tree(reg, child, predicate) {
                return Some(found);
            }
        }
        None
    }

    fn build_text_edit_pipeline(text_edit: TextEdit) -> ThreeTreePipeline {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(text_edit));
        let mut engine = TaffyLayoutEngine::new();
        let mut fs = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);
        pipeline
    }

    #[test]
    fn test_text_edit_default_colors_preserved() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller);
        let pipeline = build_text_edit_pipeline(text_edit);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        let decorated_key = find_render_object_in_tree(ro_reg, root, &|ro| {
            ro.as_any()
                .downcast_ref::<DecoratedBoxRenderObject>()
                .is_some()
        })
        .expect("should find a DecoratedBoxRenderObject");

        let decorated_ro = ro_reg
            .get(decorated_key)
            .and_then(|ro| ro.as_any().downcast_ref::<DecoratedBoxRenderObject>())
            .expect("downcast DecoratedBoxRenderObject");
        assert_eq!(
            decorated_ro.style().background,
            Some(Color::WHITE),
            "default background should be WHITE"
        );
        let border = decorated_ro
            .style()
            .border
            .as_ref()
            .expect("default should have a border");
        assert_eq!(
            border.color,
            Color::rgb(0.6, 0.6, 0.6),
            "default unfocused border color should be the hardcoded gray"
        );
        assert_eq!(border.width, 1.0, "default unfocused border width should be 1.0");

        let text_edit_key = find_render_object_in_tree(ro_reg, root, &|ro| {
            ro.as_any()
                .downcast_ref::<crate::render_objects::TextEditRenderObject>()
                .is_some()
        })
        .expect("should find a TextEditRenderObject");
        let text_edit_ro = ro_reg
            .get(text_edit_key)
            .and_then(|ro| {
                ro.as_any()
                    .downcast_ref::<crate::render_objects::TextEditRenderObject>()
            })
            .expect("downcast TextEditRenderObject");
        assert_eq!(
            text_edit_ro.color(),
            Color::BLACK,
            "default glyph color should be BLACK"
        );
    }

    #[test]
    fn test_text_edit_with_colors_applied() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let bg = Color::rgb(0.1, 0.1, 0.1);
        let text_color = Color::rgb(0.9, 0.9, 0.9);
        let border_color = Color::rgb(0.5, 0.5, 0.5);
        let text_edit = TextEdit::new(controller)
            .with_background(bg)
            .with_text_color(text_color)
            .with_border_color(border_color);
        let pipeline = build_text_edit_pipeline(text_edit);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        let decorated_key = find_render_object_in_tree(ro_reg, root, &|ro| {
            ro.as_any()
                .downcast_ref::<DecoratedBoxRenderObject>()
                .is_some()
        })
        .expect("should find a DecoratedBoxRenderObject");
        let decorated_ro = ro_reg
            .get(decorated_key)
            .and_then(|ro| ro.as_any().downcast_ref::<DecoratedBoxRenderObject>())
            .expect("downcast DecoratedBoxRenderObject");
        assert_eq!(decorated_ro.style().background, Some(bg));
        let border = decorated_ro
            .style()
            .border
            .as_ref()
            .expect("should have a border");
        assert_eq!(border.color, border_color);
        assert_eq!(border.width, 1.0, "unfocused border width should be 1.0");

        let text_edit_key = find_render_object_in_tree(ro_reg, root, &|ro| {
            ro.as_any()
                .downcast_ref::<crate::render_objects::TextEditRenderObject>()
                .is_some()
        })
        .expect("should find a TextEditRenderObject");
        let text_edit_ro = ro_reg
            .get(text_edit_key)
            .and_then(|ro| {
                ro.as_any()
                    .downcast_ref::<crate::render_objects::TextEditRenderObject>()
            })
            .expect("downcast TextEditRenderObject");
        assert_eq!(text_edit_ro.color(), text_color);
    }

    #[test]
    fn test_text_edit_focus_keeps_border_color_changes_width() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let border_color = Color::rgb(0.3, 0.3, 0.3);
        let text_edit = TextEdit::new(controller).with_border_color(border_color);

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(text_edit));
        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

        let unfocused_border_width = {
            let ro_reg = pipeline.render_objects();
            let root = ro_reg.root().expect("root");
            let decorated_key = find_render_object_in_tree(ro_reg, root, &|ro| {
                ro.as_any()
                    .downcast_ref::<DecoratedBoxRenderObject>()
                    .is_some()
            })
            .expect("find DecoratedBox unfocused");
            let decorated_ro = ro_reg
                .get(decorated_key)
                .and_then(|ro| ro.as_any().downcast_ref::<DecoratedBoxRenderObject>())
                .expect("downcast");
            let border = decorated_ro.style().border.as_ref().expect("border");
            assert_eq!(border.color, border_color, "unfocused color");
            border.width
        };
        assert_eq!(unfocused_border_width, 1.0, "unfocused width should be 1.0");

        use crate::core::{Logical, Point, ScaleSource};
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        let click = InputEvent::PointerButton {
            position: Point::<Logical>::new(10.0, 10.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::<Logical>::new(10.0, 10.0),
            &click,
            Modifiers::default(),
            &mut fs,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        pipeline.perform_rebuilds();

        let focused_border = {
            let ro_reg = pipeline.render_objects();
            let root = ro_reg.root().expect("root");
            let decorated_key = find_render_object_in_tree(ro_reg, root, &|ro| {
                ro.as_any()
                    .downcast_ref::<DecoratedBoxRenderObject>()
                    .is_some()
            })
            .expect("find DecoratedBox focused");
            let decorated_ro = ro_reg
                .get(decorated_key)
                .and_then(|ro| ro.as_any().downcast_ref::<DecoratedBoxRenderObject>())
                .expect("downcast");
            decorated_ro.style().border.as_ref().expect("border").clone()
        };
        assert_eq!(
            focused_border.color, border_color,
            "focused border color must equal unfocused border color (Approach B)"
        );
        assert_eq!(
            focused_border.width, 2.0,
            "focused border width should bump to 2.0"
        );
    }
```

Note: `Style`'s `border` field is `Option<Border>` and `Border` has `color: Color` and `width: f32` — verify the exact field names by checking `vexo/src/style.rs` if the test fails to compile, and adjust accordingly.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p vexo --lib widgets::text_edit::tests::test_text_edit_default_colors_preserved 2>&1 | tail -20`
Expected: FAIL with "no method named `with_background` found" or similar (compile error — the builders don't exist yet).

- [ ] **Step 4: Add the 3 `Option<Color>` fields and builders to `TextEdit`**

In `vexo/src/widgets/text_edit.rs`, edit the struct at `:551-555`. Currently:

```rust
#[derive(Clone)]
pub struct TextEdit {
    controller: TextEditingController,
    key: Option<WidgetKey>,
}
```

Change to:

```rust
#[derive(Clone)]
pub struct TextEdit {
    controller: TextEditingController,
    key: Option<WidgetKey>,
    background: Option<crate::core::Color>,
    text_color: Option<crate::core::Color>,
    border_color: Option<crate::core::Color>,
}
```

Update `new` at `:559-564` to initialize the new fields to `None`:

```rust
    pub fn new(controller: TextEditingController) -> Self {
        Self {
            controller,
            key: None,
            background: None,
            text_color: None,
            border_color: None,
        }
    }
```

Add the three builders right after `with_key` at `:567-570`:

```rust
    /// Set the background color of the text field box. Defaults to WHITE.
    pub fn with_background(mut self, color: crate::core::Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Set the text glyph color. Defaults to BLACK.
    pub fn with_text_color(mut self, color: crate::core::Color) -> Self {
        self.text_color = Some(color);
        self
    }

    /// Set the border color (applies to both focused and unfocused states;
    /// only the border WIDTH changes on focus). Defaults to gray.
    pub fn with_border_color(mut self, color: crate::core::Color) -> Self {
        self.border_color = Some(color);
        self
    }
```

- [ ] **Step 5: Refactor `render()` to use the new fields and drop the focused-color branch**

In `vexo/src/widgets/text_edit.rs:581-608`. Currently:

```rust
    fn render(&self, _state: &mut TextEditState, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_focused = ctx.is_focused();

        let border_color = if is_focused {
            crate::core::Color::rgb(0.2, 0.4, 0.8)
        } else {
            crate::core::Color::rgb(0.6, 0.6, 0.6)
        };

        let border_width = if is_focused { 2.0 } else { 1.0 };

        let content = super::TextEditContent::new(self.controller.text(), self.controller.editor())
            .with_font_size(self.controller.font_size())
            .with_focused(is_focused)
            .with_cursor_blink_visible(false);

        let styled = crate::DecoratedBox::with_style(
            crate::WithLayout::new(content, crate::Layout::default().padding(8.0)),
            crate::Style::default()
                .background(crate::core::Color::WHITE)
                .border(border_color, border_width)
                .corner_radius(4.0),
        );

        crate::widgets::MouseRegion::new(styled)
            .cursor(MouseCursor::System(SystemCursorKind::Text))
            .boxed()
    }
```

Change to:

```rust
    fn render(&self, _state: &mut TextEditState, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_focused = ctx.is_focused();

        let border_color = self
            .border_color
            .unwrap_or_else(|| crate::core::Color::rgb(0.6, 0.6, 0.6));

        let border_width = if is_focused { 2.0 } else { 1.0 };

        let text_color = self.text_color.unwrap_or(crate::core::Color::BLACK);

        let content = super::TextEditContent::new(self.controller.text(), self.controller.editor())
            .with_font_size(self.controller.font_size())
            .with_color(text_color)
            .with_focused(is_focused)
            .with_cursor_blink_visible(false);

        let background = self.background.unwrap_or(crate::core::Color::WHITE);

        let styled = crate::DecoratedBox::with_style(
            crate::WithLayout::new(content, crate::Layout::default().padding(8.0)),
            crate::Style::default()
                .background(background)
                .border(border_color, border_width)
                .corner_radius(4.0),
        );

        crate::widgets::MouseRegion::new(styled)
            .cursor(MouseCursor::System(SystemCursorKind::Text))
            .boxed()
    }
```

Key changes: (1) `border_color` no longer branches on `is_focused`; (2) `text_color` is read from the field with `BLACK` fallback and passed to `TextEditContent::with_color`; (3) `background` is read from the field with `WHITE` fallback.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::text_edit 2>&1 | tail -30`
Expected: PASS — all existing tests plus the 3 new ones.

If `test_text_edit_focus_keeps_border_color_changes_width` fails because `Style::border` field names differ, check `vexo/src/style.rs` for the exact `Border` struct definition and adjust the test's `border.color` / `border.width` field accesses.

- [ ] **Step 7: Build the whole workspace**

Run: `cargo build 2>&1 | tail -10`
Expected: PASS across all crates. (`shared_app` still compiles because `build_input_bar` hasn't changed yet — it just doesn't pass any colors, which means the input bar still renders white/black. That's fixed in Task 4.)

- [ ] **Step 8: Commit**

```bash
git add vexo/src/widgets/text_edit.rs vexo/src/render_objects/decorated_box.rs
git commit -m "feat(text_edit): add with_background/with_text_color/with_border_color

TextEdit now exposes three color builders defaulting to today's hardcoded
WHITE/BLACK/gray via Option::unwrap_or. Focus state keeps the caller's
border color and bumps only the width (1 -> 2), per Approach B in the
design spec. The text_color is plumbed to TextEditContent::with_color.
Also adds DecoratedBoxRenderObject::style() read accessor for tests."
```

---

## Task 4: Wire `build_input_bar` to pass theme colors; add integration test

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (`build_input_bar` at `:223-241`, call site at `:146`, test module at `:243-437`)
- Test: `shared_app/src/chats/chat_screen.rs` test module

**Interfaces:**
- Consumes: `TextEdit::with_background/with_text_color/with_border_color` from Task 3
- Produces: A chat input bar whose `TextEdit` renders `theme.surface` background, `theme.on_surface` text, `theme.outline` border in both light and dark mode.

- [ ] **Step 1: Update `build_input_bar` signature and body**

In `shared_app/src/chats/chat_screen.rs:223-241`. Currently:

```rust
fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    row! {
        WithLayout::new(TextEdit::new(controller), Layout::default().flex_grow(1.0)),
        Button::new("Send")
            .variant(ButtonVariant::Primary)
            .shadow(
                BoxShadow::new(Color::BLACK.with_alpha(0.25))
                    .blur(6.0)
                    .offset(0.0, 2.0),
            )
            .on_tap(on_send),
    }
    .gap(8.0)
    .padding(8.0)
    .boxed()
}
```

Change to:

```rust
fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    row! {
        WithLayout::new(
            TextEdit::new(controller)
                .with_background(theme.surface)
                .with_text_color(theme.on_surface)
                .with_border_color(theme.outline),
            Layout::default().flex_grow(1.0),
        ),
        Button::new("Send")
            .variant(ButtonVariant::Primary)
            .shadow(
                BoxShadow::new(Color::BLACK.with_alpha(0.25))
                    .blur(6.0)
                    .offset(0.0, 2.0),
            )
            .on_tap(on_send),
    }
    .gap(8.0)
    .padding(8.0)
    .boxed()
}
```

- [ ] **Step 2: Update the call site**

In `shared_app/src/chats/chat_screen.rs:146`. Currently:

```rust
        let input_bar = build_input_bar(tc, on_send_closure);
```

Change to:

```rust
        let input_bar = build_input_bar(tc, on_send_closure, &theme);
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build -p shared_app 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 4: Write the failing integration test**

Add to `shared_app/src/chats/chat_screen.rs` test module (at the end, after `test_chat_screen_input_bar_pinned_to_bottom_with_few_messages` at `:436`):

```rust
    #[test]
    fn test_chat_screen_input_bar_uses_theme_colors() {
        let messages_signal = seed_messages_signal();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
        };
        let dark_theme = vexo::ThemeData::dark();
        let themed = vexo::Theme::new(dark_theme.clone(), view);

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(themed.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        fn find_in_tree(
            reg: &RenderObjectRegistry,
            key: RenderObjectKey,
            predicate: &dyn Fn(&dyn vexo::RenderObject) -> bool,
        ) -> Option<RenderObjectKey> {
            let ro = reg.get(key)?;
            if predicate(ro.as_ref()) {
                return Some(key);
            }
            for &child in ro.children() {
                if let Some(found) = find_in_tree(reg, child, predicate) {
                    return Some(found);
                }
            }
            None
        }

        let text_edit_key = find_in_tree(ro_reg, root, &|ro| {
            ro.as_any()
                .downcast_ref::<vexo::TextEditRenderObject>()
                .is_some()
        })
        .expect("should find a TextEditRenderObject in the input bar");
        let text_edit_ro = ro_reg
            .get(text_edit_key)
            .and_then(|ro| ro.as_any().downcast_ref::<vexo::TextEditRenderObject>())
            .expect("downcast TextEditRenderObject");
        assert_eq!(
            text_edit_ro.color(),
            dark_theme.on_surface,
            "input bar text color should match dark theme on_surface"
        );

        let decorated_key = find_in_tree(ro_reg, root, &|ro| {
            ro.as_any()
                .downcast_ref::<vexo::DecoratedBoxRenderObject>()
                .is_some()
                && ro.as_any()
                    .downcast_ref::<vexo::DecoratedBoxRenderObject>()
                    .map(|d| d.style().background == Some(dark_theme.surface))
                    .unwrap_or(false)
        })
        .expect("should find the input bar's DecoratedBoxRenderObject (background == dark theme surface)");
        let decorated_ro = ro_reg
            .get(decorated_key)
            .and_then(|ro| ro.as_any().downcast_ref::<vexo::DecoratedBoxRenderObject>())
            .expect("downcast DecoratedBoxRenderObject");
        let border = decorated_ro
            .style()
            .border
            .as_ref()
            .expect("input bar DecoratedBox should have a border");
        assert_eq!(
            border.color, dark_theme.outline,
            "input bar border color should match dark theme outline"
        );
    }
```

If `vexo::DecoratedBoxRenderObject` is not re-exported from the vexo crate root, use the full path `vexo::render_objects::DecoratedBoxRenderObject` instead. Check `vexo/src/lib.rs` re-exports by running `cargo build -p shared_app` after adding the test — the compiler will tell you the correct path.

- [ ] **Step 5: Run the test to verify it fails (or passes if Step 1-3 are correct)**

Run: `cargo test -p shared_app --lib chats::chat_screen::tests::test_chat_screen_input_bar_uses_theme_colors 2>&1 | tail -30`
Expected: PASS (because Steps 1-3 already wired the colors through). If it FAILS, the wiring in Steps 1-3 is incorrect — debug before proceeding.

- [ ] **Step 6: Run the full chat_screen test module**

Run: `cargo test -p shared_app --lib chats::chat_screen 2>&1 | tail -20`
Expected: PASS — all existing tests plus the new one.

- [ ] **Step 7: Build and test the whole workspace**

Run: `cargo build 2>&1 | tail -10 && cargo test 2>&1 | tail -30`
Expected: PASS across all crates.

- [ ] **Step 8: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "feat(chat_screen): pass theme colors to input bar TextEdit

build_input_bar now takes &ThemeData and passes theme.surface,
theme.on_surface, theme.outline to TextEdit's new color builders. The
input bar now adapts to dark mode instead of rendering a hardcoded
white-on-black box. Integration test asserts the colors flow through
under a dark Theme."
```

---

## Spec Coverage Check

| Spec section | Implemented by |
|---|---|
| `vexo/src/widgets/text_edit_content.rs` — add `color` field | Task 2 |
| `vexo/src/widgets/text_edit.rs` — 3 builders, replace hardcoded colors, drop focused-color branch | Task 3 |
| `vexo/src/render_objects/text_edit.rs:321` — read color from content | Task 1 (paint uses `self.color`) + Task 2 (plumbs content → render object) |
| `shared_app/src/chats/chat_screen.rs:223-241` — pass theme colors | Task 4 |
| Default behavior preserved (None → unwrap_or fallbacks) | Task 3 Step 5 (render uses `unwrap_or`) |
| Focus state changes width 1→2, color stays constant | Task 3 Step 5 (border_color no longer branches on `is_focused`) + Task 3 Step 2 focus test |
| Test 1: default colors preserved | Task 3 Step 2 (`test_text_edit_default_colors_preserved`) |
| Test 2: with_colors applied | Task 3 Step 2 (`test_text_edit_with_colors_applied`) |
| Test 3: focus keeps color, changes width | Task 3 Step 2 (`test_text_edit_focus_keeps_border_color_changes_width`) |
| Test 4: integration — input bar uses theme colors | Task 4 Step 4 (`test_chat_screen_input_bar_uses_theme_colors`) |
| `Theme::of(ctx)` invalidation path unchanged | No change needed — `build_input_bar` runs inside `ChatScreen::render` which already reads `Theme::of(ctx)` at `:111`; toggling `is_dark` invalidates via the existing root `Theme` widget path |
| Manual smoke test (user runs `cargo run -p desktop_demo`) | Final handoff (not a task — assistant never runs the GUI per `CLAUDE.md`) |

## Manual Smoke Test (after all tasks complete)

After Task 4 commits, ask the user:

> "Implementation complete. All tests pass. Please run `cargo run -p desktop_demo`, open a chat, and toggle the appearance picker (Me tab → Appearance → Dark) to verify the input bar's text field switches to dark surface/on-surface text/outline border. The Send button already adapted; now the text field should match."

## Type Consistency Check

- `TextEditRenderObject::with_color(Color)` — Task 1 ✓ used by `TextEditContent::create_render_object` in Task 2 ✓
- `TextEditRenderObject::set_color(&mut self, Color) -> bool` — Task 1 ✓ used by `TextEditContent::update_render_object` in Task 2 ✓
- `TextEditRenderObject::color() -> Color` — Task 1 ✓ used by tests in Tasks 1, 3, 4 ✓
- `TextEditContent::with_color(Color)` — Task 2 ✓ used by `TextEdit::render` in Task 3 ✓
- `TextEditContent::color() -> Color` — Task 2 ✓ used by Task 2 tests ✓
- `TextEdit::with_background/with_text_color/with_border_color(Color)` — Task 3 ✓ used by `build_input_bar` in Task 4 ✓
- `DecoratedBoxRenderObject::style() -> &Style` — Task 3 Step 1 ✓ used by Task 3 and Task 4 tests ✓
- `ThemeData::surface`, `ThemeData::on_surface`, `ThemeData::outline` — all `pub` fields on `ThemeData` at `vexo/src/widgets/theme.rs:35-38` ✓ used by Task 4 ✓
