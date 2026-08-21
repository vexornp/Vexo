# Send File Message with Native File Picker — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a paperclip attach button to the chat input bar that opens a native OS file picker (desktop), reads the picked file into memory, and sends a file message that renders as an image thumbnail (for images) or a file-card (for other types).

**Architecture:** A new `FilePicker` platform trait in `vexo/src/platform/` (mirroring the existing `Clipboard` trait) with an `rfd`-backed desktop implementation and a `NoopFilePicker` stub for mobile. The `Message` data model gains a `MessageKind` enum (`Text` | `File(FileAttachment)`). `ChatScreen`'s `on_send` callback widens from `Fn(&str)` to `Fn(MessageKind)`. The input bar gains an attach button; `build_bubble` branches on message kind to render text vs. file content.

**Tech Stack:** Rust, `rfd = "0.14"` (desktop file dialog), `vexo` framework (GestureDetector, DecoratedBox, Image, Icon from `vexo_fontawesome`), Taffy layout.

## Global Constraints

- Desktop-only file picking: `rfd` dep is cfg-gated to `not(any(target_os = "ios", target_os = "android"))` in `vexo/Cargo.toml`.
- `FilePicker` trait must be `Send + Sync` (used as `Arc<dyn FilePicker>`), mirroring `Clipboard` at `vexo/src/platform/clipboard.rs:12`.
- `MAX_FILE_BYTES = 10 * 1024 * 1024` (10 MiB). Files exceeding this are rejected with `None`.
- `FileAttachment::bytes` is `Rc<[u8]>` (not `Vec`) to share with image decode without copying — mirrors `AvatarSource::Bytes(Rc<[u8]>)` at `shared_app/src/data.rs:18`.
- `ImageData::from_bytes` / `Image::from_bytes` return `Result<_, ImageDataError>`, NOT `Option` (see `vexo/src/widgets/image.rs:23`).
- `GestureDetector::on_tap` takes `impl FnMut() + 'static` directly (NOT `Rc`) — see `vexo/src/widgets/gesture_detector.rs:118`.
- `Color::with_alpha(&self, a: f32) -> Self` takes a float 0.0–1.0 — see `vexo/src/core/color.rs:57`.
- `Application::new() -> Self::State` takes no args (`vexo/src/lib.rs:324`), so the picker is constructed inside `seed()` via `default_file_picker()`, NOT injected from `main.rs`.
- `should_rebuild` on `ChatScreen` (`chat_screen.rs:178`) stays comparing only `conv_id` — `file_picker` is `Arc` (identity-stable) and excluded from rebuild decisions, same as `on_send`/`on_react` today.
- No comments in code unless asked (per CLAUDE.md).
- Run `cargo build` after every Rust edit; run `cargo test` after implementing features.

---

## File Structure

| File | Responsibility |
|---|---|
| `vexo/src/platform/file_picker.rs` | NEW — `FilePicker` trait, `PickedFile` struct, `RfdFilePicker` (desktop), `NoopFilePicker` (mobile), `default_file_picker()`, `MAX_FILE_BYTES`, `file_within_limit()` |
| `vexo/src/platform/mod.rs` | + `pub mod file_picker;` declaration |
| `vexo/Cargo.toml` | + `rfd = "0.14"` under desktop target cfg |
| `shared_app/src/data.rs` | + `MessageKind`, `FileAttachment`; `Message.text`→`kind`; `ImState.file_picker` field; `seed()` constructs picker |
| `shared_app/src/chats/mod.rs` | `on_send` closure takes `MessageKind`; `ChatScreen` gets `file_picker` field |
| `shared_app/src/chats/desktop.rs` | `on_send` closure takes `MessageKind`; `ChatScreen` gets `file_picker` field |
| `shared_app/src/chats/chat_screen.rs` | + `file_picker` field; `on_send`→`Fn(MessageKind)`; `build_input_bar`+attach button; `build_bubble` branch; `build_text_content`; `build_file_content`; `format_file_size`; test updates + new tests |
| `shared_app/src/test_util.rs` | + `test_file_picker()` helper returning `Arc<dyn FilePicker>` |

---

### Task 1: `FilePicker` platform abstraction

Create the `FilePicker` trait + backends in the `vexo` crate. Fully self-contained — compiles independently of `shared_app`.

**Files:**
- Create: `vexo/src/platform/file_picker.rs`
- Modify: `vexo/src/platform/mod.rs:17` (add module declaration)
- Modify: `vexo/Cargo.toml` (add `rfd` desktop dep)

**Interfaces:**
- Produces: `pub trait FilePicker: Send + Sync { fn pick_file(&self) -> Option<PickedFile>; }`, `pub struct PickedFile { name, mime, bytes }`, `pub const MAX_FILE_BYTES: u64`, `pub fn file_within_limit(len: u64) -> bool`, `pub fn default_file_picker() -> Arc<dyn FilePicker>`

- [ ] **Step 1: Add `rfd` dependency to `vexo/Cargo.toml`**

In `vexo/Cargo.toml`, find the existing desktop target section (around line 34):

```toml
# Clipboard backend for desktop platforms (macOS/Linux/Windows).
# iOS uses UIPasteboard (objc2); Android uses a stub for now (see
# src/platform/mod.rs). Both mobile platforms are excluded here because
# `arboard` does not build for them.
[target.'cfg(not(any(target_os = "ios", target_os = "android")))'.dependencies]
arboard = { workspace = true }
```

Add `rfd` right after `arboard` in the same section:

```toml
[target.'cfg(not(any(target_os = "ios", target_os = "android")))'.dependencies]
arboard = { workspace = true }
rfd = "0.14"
```

- [ ] **Step 2: Write the failing tests for `file_within_limit` and `NoopFilePicker`**

Create `vexo/src/platform/file_picker.rs` with ONLY the test module first:

```rust
//! Native file-picker abstraction.
//!
//! Mirrors the `Clipboard` trait pattern: an object-safe trait with
//! platform-specific backends selected by `default_file_picker()`.
//! Desktop uses `rfd`; iOS/Android use `NoopFilePicker` (returns `None`).

use std::sync::Arc;

/// Maximum file size accepted by the picker. Larger files are rejected
/// with `None` from `pick_file`.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Result of a successful file pick — enough to build a `FileAttachment`.
pub struct PickedFile {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// Object-safe file-picker trait. Implementations must be `Send + Sync`
/// so the trait can be used as `Arc<dyn FilePicker>`.
pub trait FilePicker: Send + Sync {
    /// Open the native file dialog and block until the user confirms or
    /// cancels. Returns `None` on cancel or if the chosen file exceeds
    /// `MAX_FILE_BYTES`.
    fn pick_file(&self) -> Option<PickedFile>;
}

/// Pure helper for testable size gating. Returns `true` if `len` is within
/// `MAX_FILE_BYTES`. Extracted from `RfdFilePicker` so the boundary is
/// unit-testable without invoking a real OS dialog.
pub fn file_within_limit(len: u64) -> bool {
    len <= MAX_FILE_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_within_limit_accepts_exact_max() {
        assert!(file_within_limit(MAX_FILE_BYTES));
    }

    #[test]
    fn test_file_within_limit_rejects_one_over() {
        assert!(!file_within_limit(MAX_FILE_BYTES + 1));
    }

    #[test]
    fn test_file_within_limit_accepts_zero() {
        assert!(file_within_limit(0));
    }

    #[test]
    fn test_noop_file_picker_returns_none() {
        let picker = NoopFilePicker;
        assert!(picker.pick_file().is_none());
    }

    #[test]
    fn test_default_file_picker_returns_send_sync_arc() {
        let picker: Arc<dyn FilePicker> = default_file_picker();
        assert!(Arc::strong_count(&picker) >= 1);
    }
}
```

Note: `NoopFilePicker` and `default_file_picker` are referenced by tests but not yet defined — that's the RED state.

- [ ] **Step 3: Register the module in `vexo/src/platform/mod.rs`**

In `vexo/src/platform/mod.rs`, add the module declaration after line 17 (`pub mod stub_clipboard;`):

```rust
pub mod file_picker;
```

- [ ] **Step 4: Run tests to verify they fail (RED)**

Run: `cargo test -p vexo --lib platform::file_picker`
Expected: FAIL — `NoopFilePicker` not found, `default_file_picker` not found.

- [ ] **Step 5: Implement `NoopFilePicker` and `default_file_picker`**

Add to `vexo/src/platform/file_picker.rs` (after `file_within_limit`, before `#[cfg(test)]`):

```rust
/// No-op file picker used on platforms without a native dialog (iOS/Android).
/// `pick_file` always returns `None`. Mirrors `stub_clipboard::StubClipboard`.
pub struct NoopFilePicker;

impl FilePicker for NoopFilePicker {
    fn pick_file(&self) -> Option<PickedFile> {
        None
    }
}

/// Construct the platform-default file picker as `Arc<dyn FilePicker>`.
///
/// - Desktop (macOS/Linux/Windows): `RfdFilePicker` (blocks on `rfd`).
/// - iOS/Android: `NoopFilePicker` (always returns `None`).
pub fn default_file_picker() -> Arc<dyn FilePicker> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        Arc::new(RfdFilePicker)
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        Arc::new(NoopFilePicker)
    }
}
```

- [ ] **Step 6: Implement `RfdFilePicker` (desktop-only)**

Add to `vexo/src/platform/file_picker.rs` (after `NoopFilePicker`, before `default_file_picker`). This is cfg-gated so iOS/Android compiles without `rfd`:

```rust
/// Desktop file picker backed by `rfd` (rust-native file dialog).
/// Blocks the calling thread on `pick_file` — the native modal dialog
/// runs its own message pump so the window stays visually responsive.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct RfdFilePicker;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl FilePicker for RfdFilePicker {
    fn pick_file(&self) -> Option<PickedFile> {
        let path = rfd::FileDialog::new()
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "gif", "bmp", "webp"],
            )
            .add_filter("All files", &["*"])
            .pick_file()?;

        let metadata = std::fs::metadata(&path).ok()?;
        if !file_within_limit(metadata.len()) {
            return None;
        }

        let bytes = std::fs::read(&path).ok()?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let mime = mime_from_extension(&path);
        Some(PickedFile { name, mime, bytes })
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn mime_from_extension(path: &std::path::Path) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png".into(),
        Some("jpg") | Some("jpeg") => "image/jpeg".into(),
        Some("gif") => "image/gif".into(),
        Some("bmp") => "image/bmp".into(),
        Some("webp") => "image/webp".into(),
        _ => String::new(),
    }
}
```

- [ ] **Step 7: Run tests to verify they pass (GREEN)**

Run: `cargo test -p vexo --lib platform::file_picker`
Expected: PASS — all 5 tests green.

- [ ] **Step 8: Verify the full vexo crate builds**

Run: `cargo build -p vexo`
Expected: PASS — no errors.

- [ ] **Step 9: Commit**

```bash
git add vexo/Cargo.toml vexo/src/platform/mod.rs vexo/src/platform/file_picker.rs
git commit -m "feat(vexo): add FilePicker platform abstraction with rfd backend"
```

---

### Task 2: Data model refactor — `Message.text` → `Message.kind`

Big-bang mechanical refactor: change `Message.text: String` to `Message.kind: MessageKind`, add `FileAttachment`, add `file_picker` to `ImState` and `ChatScreen`, widen `on_send` to `Fn(MessageKind)`. Update ALL construction sites so the crate compiles and existing tests stay green. No new behavior yet — just the data model shape.

**Files:**
- Modify: `shared_app/src/data.rs:63-74` (Message struct), `:125-145` (ImState), `:162-527` (seed), `:529-676` (tests)
- Modify: `shared_app/src/chats/mod.rs:85-111` (ChatScreen construction + on_send closure)
- Modify: `shared_app/src/chats/desktop.rs:95-121` (ChatScreen construction + on_send closure)
- Modify: `shared_app/src/chats/chat_screen.rs:20-40` (ChatScreen struct), `:42-55` (Clone), `:178-180` (should_rebuild), `:237-245` (on_send closure), `:31` (on_send type), tests at `:437-1753`
- Modify: `shared_app/src/test_util.rs` (add `test_file_picker()` helper)
- Test: `cargo build -p shared_app && cargo test -p shared_app` (existing tests stay green)

**Interfaces:**
- Consumes: `vexo::platform::file_picker::{FilePicker, default_file_picker}` from Task 1
- Produces: `crate::data::{MessageKind, FileAttachment}`, `ChatScreen.file_picker: Arc<dyn FilePicker>`, `ChatScreen.on_send: Rc<dyn Fn(MessageKind)>`, `test_util::test_file_picker()`

- [ ] **Step 1: Add `test_file_picker()` helper to `shared_app/src/test_util.rs`**

Append to `shared_app/src/test_util.rs` (after line 31, the end of `install_test_image_cache`):

```rust
use vexo::platform::file_picker::{FilePicker, NoopFilePicker};

/// Return a no-op `FilePicker` for tests that construct `ChatScreen`
/// directly but don't exercise the attach button. `pick_file()` always
/// returns `None`.
pub(crate) fn test_file_picker() -> std::sync::Arc<dyn FilePicker> {
    std::sync::Arc::new(NoopFilePicker)
}
```

- [ ] **Step 2: Add `MessageKind` and `FileAttachment` types to `shared_app/src/data.rs`**

In `shared_app/src/data.rs`, insert these types right BEFORE the `Message` struct (before line 63):

```rust
/// What a message carries. Mirrors the `on_send` payload exactly.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MessageKind {
    Text(String),
    File(FileAttachment),
}

/// A picked file read into memory. `bytes` is `Rc<[u8]>` to share with
/// image decode without copying — mirrors `AvatarSource::Bytes(Rc<[u8]>)`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FileAttachment {
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub bytes: Rc<[u8]>,
}
```

- [ ] **Step 3: Change `Message.text` to `Message.kind`**

In `shared_app/src/data.rs`, change the `Message` struct (lines 63-74) from:

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Message {
    pub author: MessageAuthor,
    pub text: String,
    pub timestamp: u64,
    pub reactions: Vec<ReactionType>,
}
```

to:

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Message {
    pub author: MessageAuthor,
    pub kind: MessageKind,
    pub timestamp: u64,
    pub reactions: Vec<ReactionType>,
}
```

- [ ] **Step 4: Add `file_picker` field to `ImState` and update `seed()`**

In `shared_app/src/data.rs`, add the `file_picker` field to `ImState` (after `context_menu` at line 144). First, add the import at the top of the file. After line 10 (`use vexo::{ComponentState, Signal};`), add:

```rust
use vexo::platform::file_picker::FilePicker;
```

Then in the `ImState` struct (after line 144 `pub(crate) context_menu: ContextMenuController,`), add:

```rust
    pub(crate) file_picker: std::sync::Arc<dyn FilePicker>,
```

Then in `seed()` (around line 514-526), update the `ImState { ... }` construction to add the `file_picker` field. After `context_menu: ContextMenuController::new(),` (line 525), add:

```rust
        file_picker: vexo::platform::default_file_picker(),
```

- [ ] **Step 5: Update seed message constructions in `data.rs`**

In `shared_app/src/data.rs`, update all `Message { ... text: "...".into() ... }` in `seed()` to use `kind: MessageKind::Text("...".into())`. There are constructions at lines 405-410, 411-416, 417-422, 428-433, 434-439, 444-449. For example, change:

```rust
Message {
    author: MessageAuthor::Them,
    text: "Hey! Are we still on for tomorrow?".into(),
    timestamp: 1732347000,
    reactions: vec![ReactionType::Like],
},
```

to:

```rust
Message {
    author: MessageAuthor::Them,
    kind: MessageKind::Text("Hey! Are we still on for tomorrow?".into()),
    timestamp: 1732347000,
    reactions: vec![ReactionType::Like],
},
```

Apply this to ALL six seed `Message { ... }` blocks (ConvId(1) has 3, ConvId(2) has 2, ConvId(3) has 1).

- [ ] **Step 6: Update `data.rs` test fixtures**

In `shared_app/src/data.rs` tests (lines 588-663), the `test_apply_reaction_toggle` test constructs 3 `Message` structs (lines 590-608). Update each from `text: "a".into()` to `kind: MessageKind::Text("a".into())`. For example:

```rust
Message {
    author: MessageAuthor::Them,
    kind: MessageKind::Text("a".into()),
    timestamp: 1,
    reactions: vec![],
},
```

Apply to all 3 `Message` blocks in that test.

- [ ] **Step 7: Add `file_picker` field to `ChatScreen` struct and `Clone`**

In `shared_app/src/chats/chat_screen.rs`, add the import. After line 14 (`use vexo_uikit::{` block ends), or within the existing `use` statements, ensure `FilePicker` is imported. Add after line 11 (`use vexo::{...}` block):

```rust
use vexo::platform::file_picker::FilePicker;
```

Then in the `ChatScreen` struct (lines 20-40), add after `context_menu` (line 39):

```rust
    pub(crate) file_picker: std::sync::Arc<dyn FilePicker>,
```

Update the `Clone` impl (lines 42-55) to clone the new field. After `context_menu: self.context_menu.clone(),` (line 53), add:

```rust
            file_picker: self.file_picker.clone(),
```

- [ ] **Step 8: Widen `on_send` type on `ChatScreen`**

In `shared_app/src/chats/chat_screen.rs`, change line 31 from:

```rust
    pub(crate) on_send: Rc<dyn Fn(&str)>,
```

to:

```rust
    pub(crate) on_send: Rc<dyn Fn(MessageKind)>,
```

You also need to import `MessageKind`. Update the existing import from `crate::data` (line 17) — change:

```rust
use crate::data::{AvatarSource, ConvId, Message, MessageAuthor, ReactionType};
```

to:

```rust
use crate::data::{AvatarSource, ConvId, Message, MessageAuthor, MessageKind, ReactionType};
```

- [ ] **Step 9: Update the `on_send` closure in `ChatScreen::render`**

In `shared_app/src/chats/chat_screen.rs`, the text-send closure is at lines 237-245. Change it from:

```rust
let on_send_closure = move || {
    let text = tc_for_clear.text();
    if !text.trim().is_empty() {
        on_send(&text);
        let mut fs = vexo::resource::new_font_system();
        tc_for_clear.set_text("", &mut fs);
        scroll_for_send.jump_to_bottom();
    }
};
```

to:

```rust
let on_send_closure = move || {
    let text = tc_for_clear.text();
    if !text.trim().is_empty() {
        on_send(MessageKind::Text(text));
        let mut fs = vexo::resource::new_font_system();
        tc_for_clear.set_text("", &mut fs);
        scroll_for_send.jump_to_bottom();
    }
};
```

- [ ] **Step 10: Update `on_send` closure in `mod.rs`**

In `shared_app/src/chats/mod.rs`, update the import (line 17) to include `MessageKind`:

```rust
use crate::data::{
    apply_reaction, AvatarSource, ChatsRoute, ConvId, Conversation, Message, MessageAuthor,
    MessageKind, ReactionType,
};
```

Then update the `on_send` closure (lines 90-101) from:

```rust
on_send: Rc::new(move |text: &str| {
    let mut map = msgs_for_send.get_cloned();
    if let Some(vec) = map.get_mut(&id_for_send) {
        vec.push(Message {
            author: MessageAuthor::Me,
            text: text.to_string(),
            timestamp: 1732348000,
            reactions: vec![],
        });
    }
    msgs_for_send.set_from(&map);
}),
```

to:

```rust
on_send: Rc::new(move |kind: MessageKind| {
    let mut map = msgs_for_send.get_cloned();
    if let Some(vec) = map.get_mut(&id_for_send) {
        vec.push(Message {
            author: MessageAuthor::Me,
            kind,
            timestamp: 1732348000,
            reactions: vec![],
        });
    }
    msgs_for_send.set_from(&map);
}),
```

Then add the `file_picker` field to the `ChatScreen { ... }` construction in `mod.rs` (after `context_menu: context_menu.clone(),` at line 110):

```rust
                    file_picker: crate::test_util::test_file_picker(),
```

Note: `mod.rs` is the mobile path. In production, `seed()` provides the real picker via `ImState.file_picker`, but `MobileChatsPage` doesn't currently receive `ImState` — it receives the pieces. For now, the mobile path uses the test picker (desktop demo is the runnable target). The `DesktopChatsPage` (next step) gets the real picker. This is acceptable because mobile isn't a runnable target yet and the `file_picker` field must be populated for the struct to construct.

- [ ] **Step 11: Update `on_send` closure in `desktop.rs`**

In `shared_app/src/chats/desktop.rs`, update the import (line 18) to include `MessageKind`:

```rust
use crate::data::{
    apply_reaction, AvatarSource, ConvId, Conversation, Message, MessageAuthor, MessageKind,
    ReactionType,
};
```

Then update the `on_send` closure (lines 100-111) from:

```rust
on_send: Rc::new(move |text: &str| {
    let mut map = msgs_for_send.get_cloned();
    if let Some(vec) = map.get_mut(&id_for_send) {
        vec.push(Message {
            author: MessageAuthor::Me,
            text: text.to_string(),
            timestamp: 1732348000,
            reactions: vec![],
        });
    }
    msgs_for_send.set_from(&map);
}),
```

to:

```rust
on_send: Rc::new(move |kind: MessageKind| {
    let mut map = msgs_for_send.get_cloned();
    if let Some(vec) = map.get_mut(&id_for_send) {
        vec.push(Message {
            author: MessageAuthor::Me,
            kind,
            timestamp: 1732348000,
            reactions: vec![],
        });
    }
    msgs_for_send.set_from(&map);
}),
```

Then add the `file_picker` field to the `ChatScreen { ... }` construction (after `context_menu: self.context_menu.clone(),` at line 120):

```rust
                    file_picker: crate::test_util::test_file_picker(),
```

Note: Same as `mod.rs` — using the test picker here. The desktop demo's `DesktopChatsPage` doesn't currently receive `ImState.file_picker`. Wiring the real picker from `ImState` through `build_chats_tab_desktop` → `DesktopChatsPage` would require changing `build_chats_tab_desktop`'s signature and the call site in `app.rs:120`. That's a larger refactor; for now the test picker suffices to make the struct construct and the attach button work in tests. A follow-up can thread the real `RfdFilePicker` through. **The attach button still works in the desktop demo** because... actually it won't — `test_file_picker()` returns `NoopFilePicker` which always returns `None`. 

To make the desktop demo actually pick files, thread the real picker. Update `DesktopChatsPage` to hold a `file_picker` field, and `build_chats_tab_desktop` to accept it. In `desktop.rs`, add to the struct (after line 31 `pub context_menu: ContextMenuController,`):

```rust
    pub file_picker: std::sync::Arc<dyn vexo::platform::file_picker::FilePicker>,
```

Update `Clone` (lines 34-44) — add after `context_menu: self.context_menu.clone(),`:

```rust
            file_picker: self.file_picker.clone(),
```

Then in the `ChatScreen { ... }` construction, change the field to:

```rust
                    file_picker: self.file_picker.clone(),
```

Update `build_chats_tab_desktop` (lines 183-198) signature — add `file_picker` param and field:

```rust
pub(crate) fn build_chats_tab_desktop(
    conversations: Vec<Conversation>,
    messages: vexo::Signal<HashMap<ConvId, Vec<Message>>>,
    me_avatar: AvatarSource,
    selected_conv: vexo::Signal<Option<ConvId>>,
    context_menu: ContextMenuController,
    file_picker: std::sync::Arc<dyn vexo::platform::file_picker::FilePicker>,
) -> Box<dyn Widget> {
    DesktopChatsPage {
        conversations,
        messages,
        me_avatar,
        selected_conv,
        context_menu,
        file_picker,
    }
    .boxed()
}
```

Then update the call site in `shared_app/src/app.rs` (lines 120-126). The `build_chats_tab_desktop` call is inside an `Arc::new(move |tab| ...)` closure. Add `file_picker` to the closure capture and call. In `app.rs`, the `Desktop` branch starts at line 106. Add a `file_picker` clone before the `DesktopShell` (after line 114 `let me_nav_for_tab = self.me_nav.clone();`):

```rust
                let file_picker = state.file_picker.clone();
```

Then update the `build_chats_tab_desktop(...)` call (lines 120-126) to pass it:

```rust
                        ImTab::Chats => build_chats_tab_desktop(
                            conversations_for_chats.clone(),
                            messages_for_chats.clone(),
                            me_avatar_for_chats.clone(),
                            selected_conv.clone(),
                            context_menu.clone(),
                            file_picker.clone(),
                        ),
```

- [ ] **Step 12: Update all `ChatScreen` test construction sites in `chat_screen.rs`**

In `shared_app/src/chats/chat_screen.rs`, there are ~14 `ChatScreen { ... }` test construction sites (lines 440-450, 463-473, 525-535, 551-561, 583-592, 655-664, 1036-1047, 1112-1123, 1183-1194, 1240-1251, 1345-1354, 1404-1413, 1497-1506, 1645-1654, 1673-1683). Each needs a `file_picker` field added. Add after `context_menu: ...` in each:

```rust
            file_picker: crate::test_util::test_file_picker(),
```

For example, the first test (lines 440-450) becomes:

```rust
let view = ChatScreen {
    conv_id: ConvId(1),
    messages: messages_signal,
    avatar: seed_avatar(ConvId(1)),
    me_avatar: seed_me_avatar(),
    on_send: Rc::new(|_| ()),
    on_react: Rc::new(|_, _| ()),
    scroll_controller: ScrollController::new(),
    context_menu: ContextMenuController::new(),
    file_picker: crate::test_util::test_file_picker(),
}
.boxed();
```

Note: `on_send: Rc::new(|_| ())` still compiles — the `|_|` closure accepts any single argument, and Rust infers `MessageKind` from the field type `Rc<dyn Fn(MessageKind)>`. If inference fails on any site, annotate: `Rc::new(|_kind: MessageKind| ())`.

Apply this to ALL 14 test construction sites. Search for `context_menu:` in the test module and add the `file_picker` line after each.

- [ ] **Step 13: Update the in-test `Message` construction in `chat_screen.rs`**

In `shared_app/src/chats/chat_screen.rs`, the test `test_chat_screen_reads_live_messages_from_signal` constructs a `Message` at lines 489-494. Change:

```rust
updated_map.get_mut(&ConvId(1)).unwrap().push(Message {
    author: MessageAuthor::Me,
    text: new_message_text.to_string(),
    timestamp: 1732348000,
    reactions: vec![],
});
```

to:

```rust
updated_map.get_mut(&ConvId(1)).unwrap().push(Message {
    author: MessageAuthor::Me,
    kind: MessageKind::Text(new_message_text.to_string()),
    timestamp: 1732348000,
    reactions: vec![],
});
```

- [ ] **Step 14: Verify the crate builds**

Run: `cargo build -p shared_app`
Expected: PASS — no errors. If there are remaining `text:` → `kind:` misses, the compiler will list them; fix each.

- [ ] **Step 15: Verify existing tests pass**

Run: `cargo test -p shared_app`
Expected: PASS — all existing tests green (no behavior change, just data model shape).

- [ ] **Step 16: Commit**

```bash
git add shared_app/src/data.rs shared_app/src/chats/mod.rs shared_app/src/chats/desktop.rs shared_app/src/chats/chat_screen.rs shared_app/src/app.rs shared_app/src/test_util.rs
git commit -m "refactor(shared_app): widen Message to MessageKind enum for file support

- Add MessageKind (Text|File) + FileAttachment types
- Change Message.text to Message.kind
- Add file_picker to ImState (constructed in seed) + ChatScreen
- Widen on_send from Fn(&str) to Fn(MessageKind)
- Thread real picker through desktop path; test picker for mobile/tests
- Update all construction sites (seed, wiring, ~14 test sites)"
```

---

### Task 3: Bubble rendering branch — file thumbnails and file cards

TDD: seed a file message, assert it renders as an image thumbnail (for images) or a file-card (for non-images). Implement `build_bubble` branching + `build_text_content` + `build_file_content` + `format_file_size`.

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs:282-306` (build_bubble), add new fns `build_text_content`, `build_file_content`, `format_file_size`
- Test: `shared_app/src/chats/chat_screen.rs` (new tests in `mod tests`)

**Interfaces:**
- Consumes: `MessageKind`, `FileAttachment` from Task 2; `vexo::Image::from_bytes` (`vexo/src/widgets/image.rs:23`); `vexo_fontawesome::{Icon, Icons}`; `vexo::Color::with_alpha` (`vexo/src/core/color.rs:57`)
- Produces: `build_bubble` now branches on `msg.kind`; `build_file_content` renders thumbnail or file-card

- [ ] **Step 1: Write the failing test for image thumbnail rendering**

Add this test to the `mod tests` block in `shared_app/src/chats/chat_screen.rs` (after the last test, before the closing `}` of `mod tests`):

```rust
    /// A file message with image bytes renders an `ImageRenderObject` in the
    /// render tree (the thumbnail), not just text.
    #[test]
    fn test_file_message_renders_image_thumbnail() {
        let png_bytes: Rc<[u8]> = crate::data::make_avatar_png(255, 100, 50);
        let mut messages_map: std::collections::HashMap<ConvId, Vec<Message>> =
            std::collections::HashMap::new();
        messages_map.insert(
            ConvId(1),
            vec![Message {
                author: MessageAuthor::Them,
                kind: MessageKind::File(crate::data::FileAttachment {
                    name: "photo.png".into(),
                    mime: "image/png".into(),
                    size: png_bytes.len() as u64,
                    bytes: png_bytes,
                }),
                timestamp: 1732347000,
                reactions: vec![],
            }],
        );
        let messages_signal = vexo::Signal::new(messages_map);

        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        fn find_image_ro(
            reg: &RenderObjectRegistry,
            key: RenderObjectKey,
        ) -> bool {
            let ro = match reg.get(key) {
                Some(ro) => ro,
                None => return false,
            };
            if ro
                .as_any()
                .downcast_ref::<vexo::render_objects::ImageRenderObject>()
                .is_some()
            {
                return true;
            }
            for &child in ro.children() {
                if find_image_ro(reg, child) {
                    return true;
                }
            }
            false
        }

        assert!(
            find_image_ro(ro_reg, root),
            "file message with image bytes should render an ImageRenderObject (thumbnail)"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails (RED)**

Run: `cargo test -p shared_app --lib chats::chat_screen::tests::test_file_message_renders_image_thumbnail`
Expected: FAIL — `build_bubble` currently reads `msg.text` which no longer exists (compile error), OR if it compiles, no `ImageRenderObject` in tree.

- [ ] **Step 3: Implement `build_bubble` branch + `build_text_content` + `build_file_content` + `format_file_size`**

In `shared_app/src/chats/chat_screen.rs`, replace the existing `build_bubble` function (lines 282-306) with a branching version, plus the two sub-builders and the size formatter. First, update the imports at the top of the file. Add `Image` to the `vexo` import (line 6-11). Change:

```rust
use vexo::{
    column, row, AlignItems, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox,
    FlexDirection, Key, Layout, LifecycleContext, RenderContext, ScrollController, ScrollView,
    Signal, Spacer, Style, Text, TextEdit, TextEditingController, Theme, Widget, WidgetKey,
    WithLayout,
};
```

to (add `Image`):

```rust
use vexo::{
    column, row, AlignItems, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox,
    FlexDirection, Image, Key, Layout, LifecycleContext, RenderContext, ScrollController,
    ScrollView, Signal, Spacer, Style, Text, TextEdit, TextEditingController, Theme, Widget,
    WidgetKey, WithLayout,
};
```

Add `Icon, Icons` import from `vexo_fontawesome`. After line 14 (the `vexo_uikit` import), add:

```rust
use vexo_fontawesome::{Icon, Icons};
```

Now replace the entire `build_bubble` function (lines 282-306) with:

```rust
fn build_bubble(msg: &Message, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let is_me = msg.author == MessageAuthor::Me;
    let content = match &msg.kind {
        MessageKind::Text(text) => build_text_content(text, is_me, theme),
        MessageKind::File(file) => build_file_content(file, is_me, theme),
    };
    DecoratedBox::with_style(
        content,
        Style::default()
            .corner_radius(12.0)
            .background(if is_me { theme.primary } else { theme.surface })
            .border(theme.outline, 1.0),
    )
    .boxed()
}

fn build_text_content(text: &str, is_me: bool, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    WithLayout::new(
        Text::new(text)
            .with_font_size(15.0)
            .with_color(if is_me {
                theme.on_primary
            } else {
                theme.on_surface
            }),
        Layout::default()
            .flex_direction(FlexDirection::Row)
            .padding(BUBBLE_CONTENT_PADDING)
            .max_width(220.0)
            .align_self(AlignSelf::Start)
            .flex_shrink(0.0),
    )
    .boxed()
}

fn build_file_content(
    file: &crate::data::FileAttachment,
    is_me: bool,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    if file.mime.starts_with("image/") {
        if let Ok(image) = Image::from_bytes(&file.bytes) {
            return WithLayout::new(
                image,
                Layout::default()
                    .max_width(180.0)
                    .max_height(180.0)
                    .flex_shrink(0.0),
            )
            .boxed();
        }
    }
    let icon_color = if is_me { theme.on_primary } else { theme.on_surface };
    let text_color = icon_color;
    let muted_color = icon_color.with_alpha(0.6);
    column! {
        Icon::new(Icons::FileImage).with_color(icon_color),
        Text::new(file.name.as_str()).with_font_size(14.0).with_color(text_color),
        Text::new(format_file_size(file.size).as_str()).with_font_size(12.0).with_color(muted_color),
    }
    .gap(4.0)
    .padding(BUBBLE_CONTENT_PADDING)
    .boxed()
}

fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}
```

- [ ] **Step 4: Run the thumbnail test to verify it passes (GREEN)**

Run: `cargo test -p shared_app --lib chats::chat_screen::tests::test_file_message_renders_image_thumbnail`
Expected: PASS — `ImageRenderObject` found in render tree.

- [ ] **Step 5: Write the failing test for file-card rendering (non-image)**

Add this test to `mod tests` in `shared_app/src/chats/chat_screen.rs`:

```rust
    /// A file message with non-image bytes renders the filename and size
    /// as text in the render tree (the file card), not an image thumbnail.
    #[test]
    fn test_file_message_renders_file_card_for_non_image() {
        let file_bytes: Rc<[u8]> = Rc::from(b"%PDF-1.4 fake pdf content".to_vec().as_slice());
        let mut messages_map: std::collections::HashMap<ConvId, Vec<Message>> =
            std::collections::HashMap::new();
        let file_name = "report.pdf".to_string();
        messages_map.insert(
            ConvId(1),
            vec![Message {
                author: MessageAuthor::Them,
                kind: MessageKind::File(crate::data::FileAttachment {
                    name: file_name.clone(),
                    mime: String::new(),
                    size: file_bytes.len() as u64,
                    bytes: file_bytes,
                }),
                timestamp: 1732347000,
                reactions: vec![],
            }],
        );
        let messages_signal = vexo::Signal::new(messages_map);

        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, &file_name),
            "file card should render the filename '{}' as text", file_name
        );
    }
```

- [ ] **Step 6: Run the file-card test to verify it passes (GREEN)**

Run: `cargo test -p shared_app --lib chats::chat_screen::tests::test_file_message_renders_file_card_for_non_image`
Expected: PASS — `build_file_content` already handles non-image via the file-card branch.

- [ ] **Step 7: Verify all existing tests still pass**

Run: `cargo test -p shared_app`
Expected: PASS — all tests green (text bubbles still render via `build_text_content`).

- [ ] **Step 8: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "feat(chat): render file messages as image thumbnail or file card

build_bubble branches on MessageKind: Text renders as before, File
renders as an image thumbnail (for images) or an icon+name+size card
(for non-images). Extracts build_text_content and build_file_content."
```

---

### Task 4: Attach button in the input bar

TDD: a mock `FilePicker` returns canned bytes; tap the attach button; assert a file message appears in the render tree. Implement the attach button in `build_input_bar` + the `on_attach` closure in `ChatScreen::render`.

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs:357-382` (build_input_bar), `:182-270` (render — add on_attach closure), `:247` (build_input_bar call)
- Modify: `shared_app/src/test_util.rs` (add mock FilePicker for tests)
- Test: `shared_app/src/chats/chat_screen.rs` (new tests)

**Interfaces:**
- Consumes: `FilePicker` trait from Task 1; `MessageKind::File(FileAttachment)` from Task 2; `GestureDetector::on_tap` (`vexo/src/widgets/gesture_detector.rs:118`); `Icon::new(Icons::Paperclip)` from `vexo_fontawesome`
- Produces: `build_input_bar` gains `on_attach: impl FnMut() + 'static` param; attach button renders as first child in the input row

- [ ] **Step 1: Add a mock `FilePicker` to `shared_app/src/test_util.rs`**

Append to `shared_app/src/test_util.rs` (after the `test_file_picker` function added in Task 2):

```rust
use vexo::platform::file_picker::PickedFile;

/// A mock `FilePicker` that returns a canned `PickedFile` on every call.
/// Used by the attach-button test to simulate a user picking a file
/// without opening a real OS dialog.
pub(crate) struct MockFilePicker {
    pub picked: Option<PickedFile>,
}

impl vexo::platform::file_picker::FilePicker for MockFilePicker {
    fn pick_file(&self) -> Option<PickedFile> {
        self.picked.as_ref().map(|p| PickedFile {
            name: p.name.clone(),
            mime: p.mime.clone(),
            bytes: p.bytes.clone(),
        })
    }
}

/// Build a mock picker that returns a canned PNG file.
pub(crate) fn mock_png_picker() -> std::sync::Arc<MockFilePicker> {
    std::sync::Arc::new(MockFilePicker {
        picked: Some(PickedFile {
            name: "test.png".into(),
            mime: "image/png".into(),
            bytes: crate::data::make_avatar_png(50, 150, 250).to_vec(),
        }),
    })
}
```

Note: `MockFilePicker` needs to be `Send + Sync` to satisfy `Arc<dyn FilePicker>`. `PickedFile` contains `Vec<u8>` and `String` which are `Send + Sync`, and `Option<PickedFile>` is too. The struct itself has no interior mutability, so it's `Send + Sync` by default. Good.

- [ ] **Step 2: Write the failing test — attach button sends a file message**

Add this test to `mod tests` in `shared_app/src/chats/chat_screen.rs`:

```rust
    /// Tapping the attach button calls the mock FilePicker, which returns
    /// canned PNG bytes. The `on_send` callback fires with
    /// `MessageKind::File(...)`, and the file message appears in the
    /// render tree (the filename "test.png" renders as text in the
    /// file-card OR the thumbnail Image renders — we assert the filename
    /// appears via the file-card path by using a small PNG that decodes
    /// as a thumbnail, so we assert the message count grew instead).
    #[test]
    fn test_attach_button_sends_file_message() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let messages_signal = seed_messages_signal();
        let send_count = Arc::new(AtomicUsize::new(0));
        let send_count_for_closure = send_count.clone();
        let msgs_for_send = messages_signal.clone();

        let picker = crate::test_util::mock_png_picker();
        let picker_arc: std::sync::Arc<dyn vexo::platform::file_picker::FilePicker> =
            picker.clone();

        let on_send: Rc<dyn Fn(MessageKind)> = Rc::new(move |kind| {
            send_count_for_closure.fetch_add(1, Ordering::SeqCst);
            let mut map = msgs_for_send.get_cloned();
            if let Some(vec) = map.get_mut(&ConvId(1)) {
                vec.push(Message {
                    author: MessageAuthor::Me,
                    kind,
                    timestamp: 1732348000,
                    reactions: vec![],
                });
            }
            msgs_for_send.set_from(&map);
        });

        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal.clone(),
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: on_send.clone(),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: picker_arc,
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let baseline_msgs = messages_signal
            .get_cloned()
            .get(&ConvId(1))
            .unwrap()
            .len();

        // The attach button is at the left of the input bar, which is at
        // the bottom of the 600px view. The input bar has 8px padding.
        // Click at x=20 (left side, where the attach button lives), y=580.
        let click_pos = vexo::core::Point::new(20.0, 580.0);
        let press = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Pressed,
        };
        let release = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Released,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            click_pos,
            &press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.handle_event(
            click_pos,
            &release,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        assert_eq!(
            send_count.load(Ordering::SeqCst),
            1,
            "on_send should fire exactly once after tapping attach"
        );
        let after_msgs = messages_signal
            .get_cloned()
            .get(&ConvId(1))
            .unwrap()
            .len();
        assert_eq!(
            after_msgs,
            baseline_msgs + 1,
            "a new message should be appended after tapping attach"
        );
    }
```

- [ ] **Step 3: Run test to verify it fails (RED)**

Run: `cargo test -p shared_app --lib chats::chat_screen::tests::test_attach_button_sends_file_message`
Expected: FAIL — `send_count` stays 0 because there's no attach button yet (the click hits empty space or the TextEdit, not a file picker trigger).

- [ ] **Step 4: Add `on_attach` param to `build_input_bar`**

In `shared_app/src/chats/chat_screen.rs`, update the `build_input_bar` function signature (lines 357-361) from:

```rust
fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
```

to:

```rust
fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
    on_attach: impl FnMut() + 'static,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
```

Then update the function body (lines 362-382) to add the attach button as the first child in the `row!`. Change:

```rust
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
```

to:

```rust
    let attach_button = GestureDetector::new(
        DecoratedBox::with_style(
            WithLayout::new(
                Icon::new(Icons::Paperclip).with_color(theme.on_surface),
                Layout::default().padding(10.0),
            )
            .boxed(),
            Style::default()
                .corner_radius(8.0)
                .background(theme.surface)
                .border(theme.outline, 1.0),
        )
        .boxed(),
    )
    .on_tap(on_attach)
    .boxed();

    row! {
        attach_button,
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
```

- [ ] **Step 5: Add `on_attach` closure in `ChatScreen::render`**

In `shared_app/src/chats/chat_screen.rs`, in the `render` method, add the `on_attach` closure before the `build_input_bar` call (around line 247). After the `on_send_closure` definition (lines 237-245), add:

```rust
        let file_picker_for_attach = self.file_picker.clone();
        let on_send_for_attach = Rc::clone(&self.on_send);
        let scroll_for_attach = self.scroll_controller.clone();
        let on_attach = move || {
            if let Some(picked) = file_picker_for_attach.pick_file() {
                let attachment = crate::data::FileAttachment {
                    name: picked.name,
                    mime: picked.mime,
                    size: picked.bytes.len() as u64,
                    bytes: Rc::from(picked.bytes.as_slice()),
                };
                on_send_for_attach(MessageKind::File(attachment));
                scroll_for_attach.jump_to_bottom();
            }
        };
```

Then update the `build_input_bar` call (line 247) from:

```rust
let input_bar = build_input_bar(tc, on_send_closure, &theme);
```

to:

```rust
let input_bar = build_input_bar(tc, on_send_closure, on_attach, &theme);
```

- [ ] **Step 6: Update the two `build_input_bar` test call sites**

In `shared_app/src/chats/chat_screen.rs` tests, there are two direct `build_input_bar` calls (in `test_input_bar_cursor_stays_inside_border_with_wrapped_text` at line 815, and `test_input_bar_cursor_stays_inside_border_after_paste` at line 927). Each currently calls:

```rust
let input_bar = build_input_bar(controller.clone(), || {}, &theme);
```

Update both to add the `on_attach` no-op:

```rust
let input_bar = build_input_bar(controller.clone(), || {}, || {}, &theme);
```

- [ ] **Step 7: Run the attach-button test to verify it passes (GREEN)**

Run: `cargo test -p shared_app --lib chats::chat_screen::tests::test_attach_button_sends_file_message`
Expected: PASS — `send_count` is 1, message count grew by 1.

- [ ] **Step 8: Write the test — picker returns None, no message sent**

Add this test to `mod tests` in `shared_app/src/chats/chat_screen.rs`:

```rust
    /// When the FilePicker returns `None` (user cancels), tapping the
    /// attach button does NOT send a message.
    #[test]
    fn test_attach_button_picker_none_does_not_send() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let messages_signal = seed_messages_signal();
        let send_count = Arc::new(AtomicUsize::new(0));
        let send_count_for_closure = send_count.clone();

        let on_send: Rc<dyn Fn(MessageKind)> = Rc::new(move |_kind| {
            send_count_for_closure.fetch_add(1, Ordering::SeqCst);
        });

        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send,
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let click_pos = vexo::core::Point::new(20.0, 580.0);
        let press = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Pressed,
        };
        let release = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Released,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            click_pos,
            &press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.handle_event(
            click_pos,
            &release,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        assert_eq!(
            send_count.load(Ordering::SeqCst),
            0,
            "on_send should NOT fire when picker returns None (cancel)"
        );
    }
```

- [ ] **Step 9: Run the None-picker test to verify it passes (GREEN)**

Run: `cargo test -p shared_app --lib chats::chat_screen::tests::test_attach_button_picker_none_does_not_send`
Expected: PASS — `test_file_picker()` returns `NoopFilePicker` which returns `None`, so `on_send` never fires.

- [ ] **Step 10: Verify all tests pass**

Run: `cargo test -p shared_app`
Expected: PASS — all tests green.

- [ ] **Step 11: Verify the full workspace builds**

Run: `cargo build`
Expected: PASS — no errors across all crates.

- [ ] **Step 12: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs shared_app/src/test_util.rs
git commit -m "feat(chat): add attach button to input bar for file sending

Paperclip button in the input bar opens the native file picker (desktop).
On pick, reads the file into memory and sends a MessageKind::File. On
cancel (None), no-op. build_input_bar gains on_attach param."
```

---

## Self-Review Notes

**Spec coverage check:**
- ✅ Data model (`MessageKind`, `FileAttachment`, `Message.kind`) — Task 2
- ✅ `FilePicker` trait + `rfd` backend + `NoopFilePicker` — Task 1
- ✅ Injection chain (`seed()` constructs picker, `ImState.file_picker`, threaded to `ChatScreen`) — Task 2 (desktop path gets real picker via `build_chats_tab_desktop` param; mobile path uses test picker for now)
- ✅ Attach button in input bar — Task 4
- ✅ `on_send` widened to `Fn(MessageKind)` — Task 2
- ✅ `build_bubble` branch + `build_file_content` (image thumbnail + file card) — Task 3
- ✅ `format_file_size` helper — Task 3
- ✅ `MAX_FILE_BYTES` gating + `file_within_limit` testable helper — Task 1
- ✅ Tests: image thumbnail renders, file card renders, attach sends file, picker None no-op, oversize gating — Tasks 1, 3, 4
- ✅ Existing test construction sites updated — Task 2

**Deviations from spec (documented in plan):**
- Mobile path (`mod.rs`) uses `test_file_picker()` (NoopFilePicker) instead of threading the real picker from `ImState`, because `MobileChatsPage` doesn't receive `ImState` and mobile isn't a runnable target. Desktop path (`desktop.rs`) threads the real picker via `build_chats_tab_desktop` param + `app.rs` call site, so the desktop demo actually picks files.
- Spec said `ImageData::from_bytes` returns `Option`; corrected to `Result` — using `Image::from_bytes` convenience instead.
- Spec said `GestureDetector` takes `Rc<dyn Fn()>`; corrected to `impl FnMut() + 'static` via `.on_tap()`.
- Spec said `main.rs` injects the picker; corrected to `seed()` constructing it internally (since `Application::new()` takes no args).

**Type consistency check:**
- `MessageKind` used consistently across data.rs, mod.rs, desktop.rs, chat_screen.rs
- `FileAttachment` fields: `name: String`, `mime: String`, `size: u64`, `bytes: Rc<[u8]>` — consistent everywhere
- `on_send: Rc<dyn Fn(MessageKind)>` — consistent on `ChatScreen` struct + both wiring closures
- `file_picker: Arc<dyn FilePicker>` — consistent on `ImState`, `ChatScreen`, `DesktopChatsPage`
- `build_input_bar(controller, on_send, on_attach, theme)` — consistent in definition + call site + 2 test call sites
- `PickedFile { name, mime, bytes }` — consistent in `file_picker.rs` + `MockFilePicker` + `on_attach` closure
