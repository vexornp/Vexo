# Unified Avatar Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the three free avatar functions and four inconsistent call sites with a single stateful `Avatar` Component that owns the clipped circle, optional ring, optional unread badge, and PNG decode cache.

**Architecture:** A new `Avatar` Component in `shared_app/src/widgets/avatar.rs` uses a builder API (`Avatar::new(source, diameter).with_ring(bool).with_unread_badge(count)`) and holds `Option<ImageData>` in its State for decode caching. `AvatarSource` (Bytes|Url) becomes the uniform input type across `Conversation`, `Contact`, and `Profile`. Cache invalidation fires in `ComponentState::on_update` when the `source` changes.

**Tech Stack:** Rust, vexo framework (Component/ComponentState traits, ThreeTreePipeline), Taffy layout, glyphon text.

## Global Constraints

- `AvatarSource` is `Bytes(Rc<[u8]>) | Url(Url)` — defined in `shared_app/src/data.rs`.
- The four avatar diameters are: conversation_list=40px, chat_screen=32px, contacts=40px, profile=56px.
- `Theme::of(ctx)` returns `ThemeData` (owned). Ring color is `theme.outline`; badge bg is `theme.error`; badge text is `theme.on_error`.
- `Avatar` is a leaf display widget — use default `should_rebuild() == true` (no manual override, no `Memo`).
- `install_test_image_cache(&mut pipeline)` from `crate::test_util` is required for any test using `NetworkImage` (installs `FakeHttpFetch`-backed `ImageCache`).
- Build: `cargo build -p shared_app` after edits. Tests: `cargo test -p shared_app`.
- `make_avatar_png(r, g, b)` in `data.rs` is `pub(crate)` and returns `Rc<[u8]>`.

---

### Task 1: Build the `Avatar` Component (additive)

Add the unified `Avatar` Component alongside the existing free functions. No call sites change yet. Includes `PartialEq` on `AvatarSource` for cache invalidation, `unread_badge` moved into avatar module, and unit tests.

**Files:**
- Modify: `shared_app/src/data.rs:18-22` (add `PartialEq` impl to `AvatarSource`)
- Modify: `shared_app/src/widgets/avatar.rs` (add `Avatar` Component, `border_ring` fn, `unread_badge` fn; keep old free fns)
- Modify: `shared_app/src/chats/conversation_list.rs:33,271-288` (import `unread_badge` from avatar module; remove local `unread_badge` fn)

**Interfaces:**
- Produces: `Avatar` struct with `Avatar::new(source: AvatarSource, diameter: f32) -> Self`, `.with_ring(bool) -> Self`, `.with_unread_badge(u32) -> Self`; `Component` impl with `type State = AvatarState`; `AvatarState` struct with `image: Option<ImageData>`.
- Produces: `AvatarSource: PartialEq` (identity comparison via `Rc::ptr_eq` for Bytes, URL equality for Url).
- Produces: `pub(crate) fn unread_badge(count: u32, theme: &ThemeData) -> Box<dyn Widget>` in avatar module (moved from conversation_list).

- [ ] **Step 1: Write the failing tests**

Add the following test module to the end of `shared_app/src/widgets/avatar.rs`:

```rust
#[cfg(test)]
static DECODE_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    use crate::data::{make_avatar_png, AvatarSource};

    #[test]
    fn avatar_renders_bytes_without_panic() {
        let bytes = make_avatar_png(255, 0, 0);
        let view = Avatar::new(AvatarSource::Bytes(bytes), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 0,
            "expected at least one element for bytes avatar"
        );
    }

    #[test]
    fn avatar_renders_url_without_panic() {
        let url = url::Url::parse("https://example.com/avatar.png").unwrap();
        let view = Avatar::new(AvatarSource::Url(url), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 0,
            "expected at least one element for url avatar"
        );
    }

    #[test]
    fn avatar_with_badge_and_ring_has_more_elements_than_bare() {
        let bytes = make_avatar_png(255, 0, 0);
        let source = AvatarSource::Bytes(bytes);

        let bare = Avatar::new(source.clone(), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(bare);
        let bare_count = pipeline.element_registry().len();

        let full = Avatar::new(source, 40.0)
            .with_ring(true)
            .with_unread_badge(5)
            .boxed();
        let mut pipeline2 = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline2);
        pipeline2.update(full);
        let full_count = pipeline2.element_registry().len();

        assert!(
            full_count > bare_count,
            "avatar with ring + badge ({}) should have more elements than bare ({})",
            full_count,
            bare_count
        );
    }

    #[test]
    fn avatar_caches_decode() {
        DECODE_COUNT.store(0, Ordering::SeqCst);
        let bytes = make_avatar_png(255, 0, 0);
        let source = AvatarSource::Bytes(bytes);

        let view = Avatar::new(source.clone(), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert_eq!(
            DECODE_COUNT.load(Ordering::SeqCst),
            1,
            "first render should decode exactly once"
        );

        let view2 = Avatar::new(source, 40.0).boxed();
        pipeline.update(view2);
        assert_eq!(
            DECODE_COUNT.load(Ordering::SeqCst),
            1,
            "second render should hit cache, not re-decode"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shared_app widgets::avatar::tests`
Expected: FAIL — `Avatar` struct not defined, `unread_badge` not found in avatar module.

- [ ] **Step 3: Add `PartialEq` to `AvatarSource`**

In `shared_app/src/data.rs`, after the `AvatarSource` enum definition (line 22), add:

```rust
impl PartialEq for AvatarSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bytes(a), Self::Bytes(b)) => Rc::ptr_eq(a, b),
            (Self::Url(a), Self::Url(b)) => a == b,
            _ => false,
        }
    }
}
```

- [ ] **Step 4: Rewrite `shared_app/src/widgets/avatar.rs`**

Replace the entire file content with:

```rust
use std::any::Any;

use vexo::{
    AlignItems, AlignSelf, ClipRRect, Color, Component, ComponentState, DecoratedBox, Image,
    ImageData, JustifyContent, Layout, LifecycleContext, NetworkImage, Positioned, RenderContext,
    Spacer, Stack, Style, Text, Theme, ThemeData, Widget, WithLayout,
};

use crate::data::AvatarSource;

// ---------------------------------------------------------------------------
// Legacy free functions — removed in Task 5 when all callers migrate to
// `Avatar`.
// ---------------------------------------------------------------------------

pub(crate) fn avatar(image_data: ImageData, diameter: f32) -> Box<dyn Widget> {
    ClipRRect::new(
        diameter / 2.0,
        WithLayout::new(
            Image::new(image_data),
            Layout::default().width(diameter).height(diameter),
        ),
    )
    .boxed()
}

pub(crate) fn network_avatar(url: url::Url, diameter: f32) -> Box<dyn Widget> {
    let key = url.as_str().to_string();
    ClipRRect::new(
        diameter / 2.0,
        WithLayout::new(
            NetworkImage::new(url).with_key(key),
            Layout::default().width(diameter).height(diameter),
        ),
    )
    .boxed()
}

pub(crate) fn avatar_border_ring(diameter: f32, color: Color) -> Box<dyn Widget> {
    Positioned::new(DecoratedBox::with_style(
        WithLayout::new(
            Spacer::new(),
            Layout::default().width(diameter).height(diameter),
        ),
        Style::default()
            .border(color, 1.0)
            .corner_radius(diameter / 2.0),
    ))
    .top(0.0)
    .left(0.0)
    .width(diameter)
    .height(diameter)
    .boxed()
}

// ---------------------------------------------------------------------------
// Unified Avatar Component
// ---------------------------------------------------------------------------

/// Unified avatar widget: clipped circular image + optional 1px ring +
/// optional unread badge. Owns its PNG decode cache so the image is decoded
/// once and reused across renders.
///
/// Builder API mirrors `Text`/`Image` conventions:
///   `Avatar::new(source, diameter).with_ring(true).with_unread_badge(count)`
#[derive(Clone)]
pub(crate) struct Avatar {
    source: AvatarSource,
    diameter: f32,
    ring: bool,
    unread_badge: Option<u32>,
}

impl Avatar {
    pub(crate) fn new(source: AvatarSource, diameter: f32) -> Self {
        Self {
            source,
            diameter,
            ring: false,
            unread_badge: None,
        }
    }

    pub(crate) fn with_ring(mut self, ring: bool) -> Self {
        self.ring = ring;
        self
    }

    pub(crate) fn with_unread_badge(mut self, count: u32) -> Self {
        self.unread_badge = Some(count);
        self
    }
}

/// Decode cache. `image` is lazily populated on first `render()` for the
/// `Bytes` source path. `Url` sources need no decode — `NetworkImage` +
/// `ImageCache` handle fetch/decode.
pub(crate) struct AvatarState {
    image: Option<ImageData>,
}

impl Default for AvatarState {
    fn default() -> Self {
        Self { image: None }
    }
}

impl ComponentState for AvatarState {
    fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut LifecycleContext) {
        let old = old_widget
            .downcast_ref::<Avatar>()
            .expect("old widget is Avatar");
        let new = ctx
            .widget()
            .downcast_ref::<Avatar>()
            .expect("new widget is Avatar");
        if old.source != new.source {
            self.image = None;
        }
    }
}

impl Component for Avatar {
    type State = AvatarState;

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let diameter = self.diameter;

        let base: Box<dyn Widget> = match &self.source {
            AvatarSource::Bytes(bytes) => {
                let image = state.image.get_or_insert_with(|| {
                    #[cfg(test)]
                    DECODE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ImageData::from_bytes(bytes).expect("avatar bytes are valid PNG")
                });
                ClipRRect::new(
                    diameter / 2.0,
                    WithLayout::new(
                        Image::new(image.clone()),
                        Layout::default().width(diameter).height(diameter),
                    ),
                )
                .boxed()
            }
            AvatarSource::Url(url) => {
                let key = url.as_str().to_string();
                ClipRRect::new(
                    diameter / 2.0,
                    WithLayout::new(
                        NetworkImage::new(url.clone()).with_key(key),
                        Layout::default().width(diameter).height(diameter),
                    ),
                )
                .boxed()
            }
        };

        let mut stack = Stack::new()
            .with_layout(
                Layout::stack()
                    .width(diameter)
                    .height(diameter)
                    .flex_shrink(0.0),
            )
            .push(base);

        if self.ring {
            stack = stack.push(border_ring(diameter, theme.outline));
        }

        if let Some(count) = self.unread_badge {
            if count > 0 {
                let badge = Positioned::new(unread_badge(count, &theme))
                    .top(-4.0)
                    .right(-4.0)
                    .boxed();
                stack = stack.push(badge);
            }
        }

        stack.boxed()
    }
}

/// 1px circular border ring sized to `diameter`, painted in `color`.
fn border_ring(diameter: f32, color: Color) -> Box<dyn Widget> {
    Positioned::new(DecoratedBox::with_style(
        WithLayout::new(
            Spacer::new(),
            Layout::default().width(diameter).height(diameter),
        ),
        Style::default()
            .border(color, 1.0)
            .corner_radius(diameter / 2.0),
    ))
    .top(0.0)
    .left(0.0)
    .width(diameter)
    .height(diameter)
    .boxed()
}

/// Unread-count badge: red circle with white number. Moved here from
/// `conversation_list.rs` so the `Avatar` widget owns badge rendering.
pub(crate) fn unread_badge(count: u32, theme: &ThemeData) -> Box<dyn Widget> {
    DecoratedBox::with_style(
        WithLayout::new(
            Text::new(count.to_string())
                .with_font_size(11.0)
                .with_color(theme.on_error),
            Layout::default()
                .width(20.0)
                .height(20.0)
                .justify(JustifyContent::Center)
                .align(AlignItems::Center)
                .align_self(AlignSelf::Start)
                .flex_shrink(0.0),
        ),
        Style::default()
            .background(theme.error)
            .corner_radius(10.0),
    )
    .boxed()
}

#[cfg(test)]
static DECODE_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    use crate::data::{make_avatar_png, AvatarSource};

    #[test]
    fn avatar_renders_bytes_without_panic() {
        let bytes = make_avatar_png(255, 0, 0);
        let view = Avatar::new(AvatarSource::Bytes(bytes), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 0,
            "expected at least one element for bytes avatar"
        );
    }

    #[test]
    fn avatar_renders_url_without_panic() {
        let url = url::Url::parse("https://example.com/avatar.png").unwrap();
        let view = Avatar::new(AvatarSource::Url(url), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 0,
            "expected at least one element for url avatar"
        );
    }

    #[test]
    fn avatar_with_badge_and_ring_has_more_elements_than_bare() {
        let bytes = make_avatar_png(255, 0, 0);
        let source = AvatarSource::Bytes(bytes);

        let bare = Avatar::new(source.clone(), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(bare);
        let bare_count = pipeline.element_registry().len();

        let full = Avatar::new(source, 40.0)
            .with_ring(true)
            .with_unread_badge(5)
            .boxed();
        let mut pipeline2 = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline2);
        pipeline2.update(full);
        let full_count = pipeline2.element_registry().len();

        assert!(
            full_count > bare_count,
            "avatar with ring + badge ({}) should have more elements than bare ({})",
            full_count,
            bare_count
        );
    }

    #[test]
    fn avatar_caches_decode() {
        DECODE_COUNT.store(0, Ordering::SeqCst);
        let bytes = make_avatar_png(255, 0, 0);
        let source = AvatarSource::Bytes(bytes);

        let view = Avatar::new(source.clone(), 40.0).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert_eq!(
            DECODE_COUNT.load(Ordering::SeqCst),
            1,
            "first render should decode exactly once"
        );

        let view2 = Avatar::new(source, 40.0).boxed();
        pipeline.update(view2);
        assert_eq!(
            DECODE_COUNT.load(Ordering::SeqCst),
            1,
            "second render should hit cache, not re-decode"
        );
    }
}
```

- [ ] **Step 5: Move `unread_badge` import in `conversation_list.rs`**

In `shared_app/src/chats/conversation_list.rs`:

Change the import on line 33 from:
```rust
use crate::widgets::avatar::{avatar, avatar_border_ring, network_avatar};
```
to:
```rust
use crate::widgets::avatar::{avatar, avatar_border_ring, network_avatar, unread_badge};
```

Delete the local `unread_badge` function (lines 271-288):
```rust
fn unread_badge(count: u32, theme: &ThemeData) -> Box<dyn Widget> {
    DecoratedBox::with_style(
        WithLayout::new(
            Text::new(count.to_string())
                .with_font_size(11.0)
                .with_color(theme.on_error),
            Layout::default()
                .width(20.0)
                .height(20.0)
                .justify(JustifyContent::Center)
                .align(AlignItems::Center)
                .align_self(AlignSelf::Start)
                .flex_shrink(0.0),
        ),
        Style::default().background(theme.error).corner_radius(10.0),
    )
    .boxed()
}
```

- [ ] **Step 6: Build and run tests**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: BUILD SUCCESS, all tests PASS (including the 4 new avatar tests).

- [ ] **Step 7: Commit**

```bash
git add shared_app/src/data.rs shared_app/src/widgets/avatar.rs shared_app/src/chats/conversation_list.rs
git commit -m "feat: add unified Avatar component with decode cache and badge"
```

---

### Task 2: Migrate `conversation_list` to `Avatar` widget

Replace the 15-line avatar construction block + manual Stack/ring/badge wiring with a single `Avatar::new(...)` builder call.

**Files:**
- Modify: `shared_app/src/chats/conversation_list.rs:33,142-190` (imports + avatar construction block)

**Interfaces:**
- Consumes: `Avatar`, `Avatar::new`, `.with_ring`, `.with_unread_badge` from Task 1.
- Consumes: `unread_badge` now lives in avatar module (Task 1 moved it).

- [ ] **Step 1: Update imports**

In `shared_app/src/chats/conversation_list.rs`, change line 33 from:
```rust
use crate::widgets::avatar::{avatar, avatar_border_ring, network_avatar, unread_badge};
```
to:
```rust
use crate::widgets::avatar::Avatar;
```

- [ ] **Step 2: Replace the avatar construction block**

In `ConversationRow::render` (lines 142-190), replace the entire block from `let avatar: Box<dyn Widget> = match &self.avatar {` through the end of the `let avatar_with_badge = Stack::new()...` expression with:

```rust
        let avatar_with_badge = Avatar::new(self.avatar.clone(), 40.0)
            .with_ring(true)
            .with_unread_badge(self.unread_count)
            .boxed();
```

The old code being replaced (lines 142-190):
```rust
        let avatar: Box<dyn Widget> = match &self.avatar {
            AvatarSource::Bytes(bytes) => avatar(
                ImageData::from_bytes(bytes).expect("avatar bytes are valid PNG"),
                40.0,
            ),
            AvatarSource::Url(url) => network_avatar(url.clone(), 40.0),
        };

        // 1px outline ring ...
        let border_ring = avatar_border_ring(40.0, theme.outline);

        let name_color = nav_colors.row_text;
        let preview_color = nav_colors.placeholder_text;

        let name_text = Text::new(self.name.as_str())
            .with_font_size(16.0)
            .with_color(name_color);
        let preview_text = Text::new(self.preview.as_str())
            .with_font_size(13.0)
            .with_color(preview_color)
            .with_max_lines(1);

        let info_col = column! { name_text, preview_text }.gap(2.0).flex_grow(1.0);

        let time_text = Text::new(format_timestamp(self.timestamp).as_str())
            .with_font_size(12.0)
            .with_color(name_color);

        let right_col = column! { time_text }.flex_shrink(0.0);

        let badge: Option<Box<dyn Widget>> = if self.unread_count > 0 {
            Some(
                Positioned::new(unread_badge(self.unread_count, &theme))
                    .top(-4.0)
                    .right(-4.0)
                    .boxed(),
            )
        } else {
            None
        };

        let avatar_with_badge = Stack::new()
            .with_layout(Layout::stack().width(40.0).height(40.0).flex_shrink(0.0))
            .push(avatar)
            .push(border_ring)
            .push(badge)
            .boxed();
```

**IMPORTANT:** The `name_text`, `preview_text`, `info_col`, `time_text`, `right_col` variables are defined in the middle of that block. After removing the block, you must re-add those variable definitions. The replacement should be:

```rust
        let avatar_with_badge = Avatar::new(self.avatar.clone(), 40.0)
            .with_ring(true)
            .with_unread_badge(self.unread_count)
            .boxed();

        let name_color = nav_colors.row_text;
        let preview_color = nav_colors.placeholder_text;

        let name_text = Text::new(self.name.as_str())
            .with_font_size(16.0)
            .with_color(name_color);
        let preview_text = Text::new(self.preview.as_str())
            .with_font_size(13.0)
            .with_color(preview_color)
            .with_max_lines(1);

        let info_col = column! { name_text, preview_text }.gap(2.0).flex_grow(1.0);

        let time_text = Text::new(format_timestamp(self.timestamp).as_str())
            .with_font_size(12.0)
            .with_color(name_color);

        let right_col = column! { time_text }.flex_shrink(0.0);
```

Check whether `ImageData`, `Positioned`, `Stack` are still used elsewhere in the file after the migration; remove from the `use vexo::{...}` import if unused. `AvatarSource` is still needed (`ConversationRow` has an `avatar: AvatarSource` field) — keep it.

- [ ] **Step 3: Build and run tests**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: BUILD SUCCESS, all tests PASS. If there are unused import warnings, remove the unused imports.

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/chats/conversation_list.rs
git commit -m "refactor: migrate conversation_list to unified Avatar widget"
```

---

### Task 3: Migrate `chat_screen` to `Avatar` widget + remove manual cache

Replace the 10-line avatar construction block with `Avatar::new(src, 32.0)`. Delete `them_avatar_image`/`me_avatar_image` cache fields and the `them_avatar()`/`me_avatar()` methods — the `Avatar` widget now owns its own decode cache.

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs:18,60-107,198-213` (imports, State fields + methods, avatar construction block)

**Interfaces:**
- Consumes: `Avatar` from Task 1.
- Produces: `ChatScreen` with `me_avatar_bytes: Rc<[u8]>` still (renamed to `me_avatar: AvatarSource` in Task 5). For now, wraps in `AvatarSource::Bytes(...)` at the call site.

- [ ] **Step 1: Update imports**

In `shared_app/src/chats/chat_screen.rs`, change line 18 from:
```rust
use crate::widgets::avatar::{avatar, network_avatar};
```
to:
```rust
use crate::widgets::avatar::Avatar;
```

Also remove `ImageData` from the `use vexo::{...}` block if it's no longer referenced elsewhere in the file (it was only used for `them_avatar_image`/`me_avatar_image`).

- [ ] **Step 2: Delete cache fields from `ChatScreenState`**

In `ChatScreenState` struct (lines 60-73), remove these two fields:
```rust
    them_avatar_image: Option<ImageData>,
    me_avatar_image: Option<ImageData>,
```

In `Default for ChatScreenState` (lines 75-84), remove:
```rust
            them_avatar_image: None,
            me_avatar_image: None,
```

- [ ] **Step 3: Delete cache methods from `ChatScreenState` impl**

Delete the `them_avatar` and `me_avatar` methods (lines 94-107):
```rust
    /// Lazily decode and cache the avatar images. ...
    fn them_avatar(&mut self, bytes: &Rc<[u8]>) -> &ImageData {
        self.them_avatar_image.get_or_insert_with(|| {
            ImageData::from_bytes(bytes).expect("avatar bytes are valid PNG")
        })
    }

    fn me_avatar(&mut self, bytes: &Rc<[u8]>) -> &ImageData {
        self.me_avatar_image.get_or_insert_with(|| {
            ImageData::from_bytes(bytes).expect("avatar bytes are valid PNG")
        })
    }
```

Also update the comment on `them_avatar_image` (lines 62-65) — it's being deleted, so the comment goes with it.

- [ ] **Step 4: Replace the avatar construction block in `render()`**

In `ChatScreen::render()`, replace lines 198-213:
```rust
                // Build only the avatar widget this row needs (Q20: single
                // avatar, not both). The "me" path uses the cached decoded
                // `ImageData`; the "them" path branches on `AvatarSource` —
                // `Bytes` reuses the cache, `Url` defers to `NetworkImage`
                // (which self-caches via `ImageCache`).
                let avatar_widget: Box<dyn Widget> = if is_me {
                    avatar(state.me_avatar(&self.me_avatar_bytes).clone(), 32.0)
                } else {
                    match &self.avatar {
                        AvatarSource::Bytes(bytes) => {
                            avatar(state.them_avatar(bytes).clone(), 32.0)
                        }
                        AvatarSource::Url(url) => network_avatar(url.clone(), 32.0),
                    }
                };
```

with:
```rust
                let src = if is_me {
                    AvatarSource::Bytes(self.me_avatar_bytes.clone())
                } else {
                    self.avatar.clone()
                };
                let avatar_widget: Box<dyn Widget> = Avatar::new(src, 32.0).boxed();
```

- [ ] **Step 5: Build and run tests**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: BUILD SUCCESS, all tests PASS. `AvatarSource` is still needed (used in the `is_me` branch as `AvatarSource::Bytes(...)`); only remove `ImageData` from the vexo import if no longer referenced.

- [ ] **Step 6: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "refactor: migrate chat_screen to Avatar widget, remove manual decode cache"
```

---

### Task 4: Migrate `contacts_screen` and `profile_screen` to `Avatar` widget

Replace both one-liner call sites with `Avatar::new(...)`. These are the simplest migrations.

**Files:**
- Modify: `shared_app/src/contacts/contacts_screen.rs:9,43-46` (import + avatar construction)
- Modify: `shared_app/src/me/profile_screen.rs:20,407-410` (import + avatar construction)

**Interfaces:**
- Consumes: `Avatar` from Task 1.
- Produces: both screens use `Avatar::new(AvatarSource::Bytes(...), diameter)` (the `AvatarSource::Bytes` wrapper is dropped in Task 5 when the data model unifies).

- [ ] **Step 1: Migrate `contacts_screen.rs`**

In `shared_app/src/contacts/contacts_screen.rs`:

Change import (line 9) from:
```rust
use crate::widgets::avatar::avatar;
```
to:
```rust
use crate::widgets::avatar::Avatar;
use crate::data::AvatarSource;
```

In `build_contact_row` (lines 43-46), replace:
```rust
    let avatar = avatar(
        ImageData::from_bytes(&c.avatar_bytes).expect("avatar bytes are valid PNG"),
        40.0,
    );
```
with:
```rust
    let avatar = Avatar::new(AvatarSource::Bytes(c.avatar_bytes.clone()), 40.0).boxed();
```

Remove `ImageData` from the `use vexo::{...}` import if no longer used.

- [ ] **Step 2: Migrate `profile_screen.rs`**

In `shared_app/src/me/profile_screen.rs`:

Change import (line 20) from:
```rust
use crate::widgets::avatar::avatar;
```
to:
```rust
use crate::widgets::avatar::Avatar;
use crate::data::AvatarSource;
```

In `build_header_row` (lines 407-410), replace:
```rust
    let avatar_widget = avatar(
        ImageData::from_bytes(&profile.avatar_bytes).expect("avatar bytes are valid PNG"),
        56.0,
    );
```
with:
```rust
    let avatar_widget = Avatar::new(AvatarSource::Bytes(profile.avatar_bytes.clone()), 56.0)
        .boxed();
```

Remove `ImageData` from the `use vexo::{...}` import if no longer used.

- [ ] **Step 3: Build and run tests**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: BUILD SUCCESS, all tests PASS.

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/contacts/contacts_screen.rs shared_app/src/me/profile_screen.rs
git commit -m "refactor: migrate contacts and profile screens to Avatar widget"
```

---

### Task 5: Unify data model + remove old free functions

Change `Contact`/`Profile` from `avatar_bytes: Rc<[u8]>` to `avatar: AvatarSource`. Thread `AvatarSource` through `app.rs` → `chats/mod.rs`/`desktop.rs` → `ChatScreen`. Remove the three legacy free functions. Add data-model test.

**Files:**
- Modify: `shared_app/src/data.rs:101-115,453-495,503` (struct fields + seed literals)
- Modify: `shared_app/src/app.rs:44` (me_avatar source)
- Modify: `shared_app/src/chats/mod.rs:27,37,60,88,124,131` (me_avatar type + threading)
- Modify: `shared_app/src/chats/desktop.rs:27,37,97,184,191` (me_avatar type + threading)
- Modify: `shared_app/src/chats/chat_screen.rs:30,48,204,399-400` (field rename + type + test helper)
- Modify: `shared_app/src/contacts/contacts_screen.rs:44` (drop AvatarSource::Bytes wrapper)
- Modify: `shared_app/src/me/profile_screen.rs:407-410` (drop AvatarSource::Bytes wrapper)
- Modify: `shared_app/src/widgets/avatar.rs` (remove `avatar`, `network_avatar`, `avatar_border_ring` free fns)

**Interfaces:**
- Produces: `Contact.avatar: AvatarSource`, `Profile.avatar: AvatarSource` (was `avatar_bytes: Rc<[u8]>`).
- Produces: `ChatScreen.me_avatar: AvatarSource` (was `me_avatar_bytes: Rc<[u8]>`).
- Produces: `build_chats_tab` / `build_chats_tab_desktop` take `me_avatar: AvatarSource` (was `Rc<[u8]>`).
- Removes: `avatar()`, `network_avatar()`, `avatar_border_ring()` free fns from `widgets/avatar.rs`.

- [ ] **Step 1: Write the failing data-model test**

In `shared_app/src/data.rs`, add this test to the existing `#[cfg(test)] mod tests` block (after `test_avatar_bytes_decode`):

```rust
    #[test]
    fn test_contact_and_profile_use_avatar_source() {
        let s = seed();
        for c in &s.contacts {
            assert!(
                matches!(c.avatar, AvatarSource::Bytes(_)),
                "contact {} should have AvatarSource::Bytes avatar",
                c.name
            );
        }
        assert!(
            matches!(s.profile.avatar, AvatarSource::Bytes(_)),
            "profile should have AvatarSource::Bytes avatar"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shared_app data::tests::test_contact_and_profile_use_avatar_source`
Expected: FAIL — `no field 'avatar' on type 'Contact'`.

- [ ] **Step 3: Update `Contact` and `Profile` struct definitions**

In `shared_app/src/data.rs`:

Change `Contact` (line 106) from:
```rust
    pub avatar_bytes: Rc<[u8]>,
```
to:
```rust
    pub avatar: AvatarSource,
```

Change `Profile` (line 114) from:
```rust
    pub avatar_bytes: Rc<[u8]>,
```
to:
```rust
    pub avatar: AvatarSource,
```

- [ ] **Step 4: Update `seed()` literals**

In `shared_app/src/data.rs` `seed()`:

For every `Contact { ... avatar_bytes: X, ... }` literal, change to `avatar: AvatarSource::Bytes(X),`. There are 8 contacts (lines 453-495). For example:
```rust
// Before:
avatar_bytes: alice_bytes.clone(),
// After:
avatar: AvatarSource::Bytes(alice_bytes.clone()),
```

For the `Profile` literal (line 503), change:
```rust
// Before:
avatar_bytes: me_bytes,
// After:
avatar: AvatarSource::Bytes(me_bytes),
```

- [ ] **Step 5: Update `app.rs`**

In `shared_app/src/app.rs` line 44, change:
```rust
// Before:
let me_avatar = profile.avatar_bytes.clone();
// After:
let me_avatar = profile.avatar.clone();
```

The type of `me_avatar` changes from `Rc<[u8]>` to `AvatarSource`. This propagates to `build_chats_tab` and `build_chats_tab_desktop` calls.

- [ ] **Step 6: Update `chats/mod.rs`**

In `shared_app/src/chats/mod.rs`:

- `MobileChatsPage` struct (line 27): `me_avatar: Rc<[u8]>` → `me_avatar: AvatarSource`
- `Clone` impl (line 37): `me_avatar: Rc::clone(&self.me_avatar)` → `me_avatar: self.me_avatar.clone()`
- `render()` (line 88): `me_avatar_bytes: me_avatar_for_dest.clone()` → `me_avatar: me_avatar_for_dest.clone()`
- `build_chats_tab` signature (line 124): `me_avatar: Rc<[u8]>` → `me_avatar: AvatarSource`
- `build_chats_tab` body (line 131): `me_avatar,` (unchanged — field shorthand)

Add `AvatarSource` to the `use crate::data::{...}` import.

- [ ] **Step 7: Update `chats/desktop.rs`**

In `shared_app/src/chats/desktop.rs`:

- `DesktopChatsPage` struct (line 27): `me_avatar: Rc<[u8]>` → `me_avatar: AvatarSource`
- `Clone` impl (line 37): `me_avatar: Rc::clone(&self.me_avatar)` → `me_avatar: self.me_avatar.clone()`
- `render()` (line 97): `me_avatar_bytes: self.me_avatar.clone()` → `me_avatar: self.me_avatar.clone()`
- `build_chats_tab_desktop` signature (line 184): `me_avatar: Rc<[u8]>` → `me_avatar: AvatarSource`
- `build_chats_tab_desktop` body (line 191): `me_avatar,` (unchanged — field shorthand)

Add `AvatarSource` to the `use crate::data::{...}` import.

- [ ] **Step 8: Update `chat_screen.rs`**

In `shared_app/src/chats/chat_screen.rs`:

- `ChatScreen` struct (line 30): `me_avatar_bytes: Rc<[u8]>` → `me_avatar: AvatarSource`
- `Clone` impl (line 48): `me_avatar_bytes: Rc::clone(&self.me_avatar_bytes)` → `me_avatar: self.me_avatar.clone()`
- `render()` (lines 203-205): change the `is_me` branch from:
  ```rust
  let src = if is_me {
      AvatarSource::Bytes(self.me_avatar_bytes.clone())
  } else {
      self.avatar.clone()
  };
  ```
  to:
  ```rust
  let src = if is_me {
      self.me_avatar.clone()
  } else {
      self.avatar.clone()
  };
  ```
- Test helper `seed_me_avatar()` (lines 399-400): change return type and body:
  ```rust
  // Before:
  fn seed_me_avatar() -> Rc<[u8]> {
      crate::data::seed().profile.avatar_bytes.clone()
  }
  // After:
  fn seed_me_avatar() -> AvatarSource {
      crate::data::seed().profile.avatar.clone()
  }
  ```
- All test `ChatScreen { ... }` literals: change `me_avatar_bytes: seed_me_avatar()` to `me_avatar: seed_me_avatar()`. There are approximately 12 occurrences (search for `me_avatar_bytes:` in this file).

- [ ] **Step 9: Update `contacts_screen.rs`**

In `shared_app/src/contacts/contacts_screen.rs` line 44, change:
```rust
// Before:
let avatar = Avatar::new(AvatarSource::Bytes(c.avatar_bytes.clone()), 40.0).boxed();
// After:
let avatar = Avatar::new(c.avatar.clone(), 40.0).boxed();
```

Remove the `use crate::data::AvatarSource;` import (no longer needed here).

- [ ] **Step 10: Update `profile_screen.rs`**

In `shared_app/src/me/profile_screen.rs` lines 407-410, change:
```rust
// Before:
let avatar_widget = Avatar::new(AvatarSource::Bytes(profile.avatar_bytes.clone()), 56.0)
    .boxed();
// After:
let avatar_widget = Avatar::new(profile.avatar.clone(), 56.0).boxed();
```

Remove the `use crate::data::AvatarSource;` import (no longer needed here).

- [ ] **Step 11: Remove old free functions from `avatar.rs`**

In `shared_app/src/widgets/avatar.rs`, delete the three legacy functions and their section comment:

```rust
// ---------------------------------------------------------------------------
// Legacy free functions — removed in Task 5 when all callers migrate to
// `Avatar`.
// ---------------------------------------------------------------------------

pub(crate) fn avatar(image_data: ImageData, diameter: f32) -> Box<dyn Widget> {
    ...
}

pub(crate) fn network_avatar(url: url::Url, diameter: f32) -> Box<dyn Widget> {
    ...
}

pub(crate) fn avatar_border_ring(diameter: f32, color: Color) -> Box<dyn Widget> {
    ...
}
```

Also remove now-unused imports that were only used by the old free functions. Check: `Image` (still used by `Avatar::render`), `NetworkImage` (still used by `Avatar::render`), `Color` (used by `border_ring`). Most imports are shared between old and new code — verify by building. Also make `unread_badge` private (remove `pub(crate)`) since no external caller remains after Task 2.

- [ ] **Step 12: Build and run all tests**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: BUILD SUCCESS, all tests PASS (including `test_contact_and_profile_use_avatar_source`).

- [ ] **Step 13: Commit**

```bash
git add -A
git commit -m "refactor: unify avatar data model to AvatarSource, remove legacy free fns"
```

---

## Post-Implementation Verification

After all 5 tasks are complete:

- [ ] Run full workspace build: `cargo build`
- [ ] Run full workspace tests: `cargo test`
- [ ] Verify no `avatar_bytes` references remain: search for `avatar_bytes` in `shared_app/src/` — should be zero matches.
- [ ] Verify no calls to old free fns remain: search for `avatar(`, `network_avatar(`, `avatar_border_ring(` in `shared_app/src/` — should be zero matches (only `Avatar::new(` calls).
