# Send File Message with Native File Picker

**Date:** 2026-08-21
**Status:** Approved (pre-implementation)
**Scope:** `shared_app/src/chats/chat_screen.rs` + supporting platform/data changes

## Goal

Add the ability to send a file message in the chat screen: a paperclip attach button in the input bar opens the native OS file picker (desktop only), the picked file is read into memory, and a file message is appended to the conversation. Image files render as a thumbnail bubble; non-image files render as a file-card bubble (icon + name + size).

## Non-Goals

- iOS/Android file pickers (desktop-only — `rfd` on desktop; `NoopFilePicker` stub elsewhere).
- Multiple file selection (single pick per tap).
- File captions (text + file in one message).
- Persisting files across app restarts (in-memory `Rc<[u8]>` only).
- Sending files over a real network (still mocked — `on_send` pushes to the local `Signal`).
- Thumbnail caching across renders (decode per rebuild; revisit if profiled as hot).
- Download/preview of received files (tap-to-open).
- Progress indication for large file reads (capped at 10 MiB, synchronous).

## Decisions (settled during brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| Platform scope | Desktop only | Matches the runnable demo; fastest to ship. iOS/Android get a `NoopFilePicker` stub. |
| File message rendering | Image thumbnail + file card | Images decode via existing `ImageData::from_bytes` path; non-images get an icon+name+size card. |
| Picker abstraction | `FilePicker` trait + `rfd` backend | Mirrors the existing `Clipboard` trait pattern in `vexo/src/platform/`; keeps `ChatScreen` testable. |
| Send callback wiring | Widen `on_send` to `Fn(MessageKind)` | Unified payload enum; one callback handles both text and file sends. |
| `Message` data model | `kind: MessageKind` enum (replaces `text`) | Symmetric with the payload enum; clean `match` in `build_bubble`; no nullable fields. |

## Architecture

### Data model (`shared_app/src/data.rs`)

New types alongside `Message` (currently at `data.rs:63-74`):

```rust
/// What a message carries. Mirrors the `on_send` payload exactly so the
/// send closure can forward it straight into a `Message` without conversion.
pub(crate) enum MessageKind {
    Text(String),
    File(FileAttachment),
}

/// A picked file read into memory. `bytes` is `Rc<[u8]>` (not `Vec`) so the
/// same bytes can be shared between the Message and a decoded `ImageData`
/// thumbnail without copying — mirrors `AvatarSource::Bytes(Rc<[u8]>)` at
/// `data.rs:18`.
pub(crate) struct FileAttachment {
    pub name: String,        // basename, e.g. "photo.png"
    pub mime: String,        // best-effort, e.g. "image/png"; "" if unknown
    pub size: u64,           // bytes.len() as u64
    pub bytes: Rc<[u8]>,     // full file contents in memory
}

pub(crate) struct Message {
    pub author: MessageAuthor,
    pub kind: MessageKind,          // was: text: String
    pub timestamp: u64,
    pub reactions: Vec<ReactionType>,
}
```

- `MessageKind` IS the payload type — `on_send` becomes `Rc<dyn Fn(MessageKind)>`. No separate `MessagePayload` enum.
- `FileAttachment::bytes` is `Rc<[u8]>` to share with `ImageData::from_bytes` decode (no copy), matching the `AvatarSource::Bytes` precedent at `data.rs:18`.

### Platform abstraction (`vexo/src/platform/file_picker.rs`)

New module mirroring `vexo/src/platform/clipboard.rs` + the `default_clipboard()` factory at `platform/mod.rs:32`:

```rust
/// Result of a successful pick — enough to build a `FileAttachment`.
pub struct PickedFile {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub trait FilePicker: Send + Sync {
    /// Blocks until the user confirms or cancels. Returns `None` on cancel,
    /// or `None` if the chosen file exceeds `MAX_FILE_BYTES` (10 MiB).
    fn pick_file(&self) -> Option<PickedFile>;
}

pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn default_file_picker() -> Box<dyn FilePicker> {
    Box::new(RfdFilePicker)
}
#[cfg(any(target_os = "ios", target_os = "android"))]
pub fn default_file_picker() -> Box<dyn FilePicker> {
    Box::new(NoopFilePicker)  // pick_file() always returns None
}
```

- **`RfdFilePicker`** (cfg `not(ios/android)`): wraps `rfd::FileDialog::new().add_filter(images).add_filter(all).pick_file()`. Derives `name` from path filename, `mime` from extension via a tiny inline map (`png`→`image/png`, `jpg`/`jpeg`→`image/jpeg`, `gif`→`image/gif`, else `""`). Reads bytes via `std::fs::read`. Enforces `MAX_FILE_BYTES` by `metadata().len()` before reading — returns `None` if exceeded.
- **`NoopFilePicker`** (cfg `ios/android`): `pick_file()` → `None`. Mirrors `stub_clipboard.rs` pattern. Keeps `shared_app` compiling for iOS without file-picking deps.
- **`rfd` dependency**: added to `vexo/Cargo.toml` under the `cfg(not(any(target_os = "ios", target_os = "android")))` target section (alongside `arboard`), version `0.14`. `rfd` only links on desktop builds.
- **Synchronous API**: `pick_file` blocks. Rfd's sync API is simplest and we're desktop-only; the attach button's `on_tap` runs off the render thread anyway.

### Injection chain

1. `desktop_demo/src/main.rs` — call `vexo::platform::default_file_picker()`, wrap in `Arc<dyn FilePicker>`, pass to a new `ImState` constructor arg.
2. `shared_app/src/data.rs:144` — add `pub file_picker: Arc<dyn FilePicker>` to `ImState` (mirrors `context_menu` field).
3. `shared_app/src/app.rs:63` — `ImState::new(...)` call site updated.
4. `shared_app/src/chats/mod.rs:85` & `desktop.rs:95` — `ChatScreen { ... file_picker: state.file_picker.clone(), ... }`.
5. `shared_app/src/chats/chat_screen.rs:20-40` — add `file_picker: Arc<dyn FilePicker>` field + `Clone` impl update (`:42-55`).
6. `should_rebuild` (`:178-180`) stays comparing only `conv_id` — `file_picker` is `Arc` (identity-stable), doesn't participate in rebuild decision (same rationale as `on_send`/`on_react` being excluded today).

### Attach button & input bar (`chat_screen.rs:357-382`)

`build_input_bar` gains an attach button as a new first child in the `row!`:

```
[ AttachButton ] [ TextEdit (flex_grow 1) ] [ Send Button ]
```

- **AttachButton**: composed inline in `build_input_bar` (no new `IconButton` widget — YAGNI). Pattern matches `desktop_shell.rs:172-192` (Icon in `GestureDetector` + `DecoratedBox`):
  ```rust
  DecoratedBox::with_style(
      GestureDetector::new(
          Icon::new(Icons::Paperclip).with_color(theme.on_surface),
          Rc::new(move || on_attach()),
      ),
      Style::default()
          .corner_radius(8.0)
          .background(theme.surface)
          .border(theme.outline, 1.0),
  )
  .boxed()
  ```
- **Sizing**: no explicit height — lets flexbox size to the Icon's intrinsic size + DecoratedBox padding. Inner content uses `Layout::default().padding(10.0)` (matches `BUBBLE_CONTENT_PADDING`).
- **`build_input_bar` signature** gains `on_attach: impl FnMut() + 'static` — stays pure-UI (no `Arc<dyn FilePicker>` leak into the builder):
  ```rust
  fn build_input_bar(
      controller: TextEditingController,
      on_send: impl FnMut() + 'static,      // text send (unchanged shape)
      on_attach: impl FnMut() + 'static,   // NEW
      theme: &vexo::ThemeData,
  ) -> Box<dyn Widget>
  ```

### `on_send` closure widening

The text-send closure at `chat_screen.rs:237-245` wraps `tc_for_clear.text()` in `MessageKind::Text(text)` before calling `on_send`. So `ChatScreen::render` builds one `on_send: Rc<dyn Fn(MessageKind)>`, and two closures feed it:

- **Text send** (existing closure, updated): `on_send(MessageKind::Text(text))`
- **File send** (new `on_attach` closure): captures `Arc<dyn FilePicker>` + `on_send` + `scroll_controller`:
  ```rust
  let on_attach = move || {
      if let Some(picked) = file_picker.pick_file() {
          let attachment = FileAttachment {
              name: picked.name,
              mime: picked.mime,
              size: picked.bytes.len() as u64,
              bytes: Rc::from(picked.bytes.as_slice()),
          };
          on_send(MessageKind::File(attachment));
          scroll_for_send.jump_to_bottom();
      }
  };
  ```
  No text-controller interaction (no draft to clear). `jump_to_bottom()` mirrors the text-send closure at `chat_screen.rs:243`.

### Bubble rendering branch (`chat_screen.rs:282-306`)

`build_bubble` branches on `msg.kind`:

```rust
fn build_bubble(msg: &Message, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let content: Box<dyn Widget> = match &msg.kind {
        MessageKind::Text(text) => build_text_content(text, is_me, theme),
        MessageKind::File(file) => build_file_content(file, is_me, theme),
    };
    // wrap `content` in DecoratedBox exactly as today (corner_radius, bg, border)
}
```

**Two sub-builders** (new private fns in `chat_screen.rs`):

1. **`build_text_content`** — today's `Text::new(msg.text.as_str())` block extracted verbatim (the `WithLayout` wrapping the `Text` at `:285-299`), parameterized on `&str` instead of reading `msg.text`.

2. **`build_file_content`** — NEW. Two visual modes based on whether the file is an image:
   - **Image** (`mime.starts_with("image/")` AND decode succeeds): a thumbnail `Image` widget. Decode via `ImageData::from_bytes(&file.bytes)` (synchronous — same call as `avatar.rs:80-84` for `AvatarSource::Bytes`). Wrap in `WithLayout` with `max_width(180.0)` + `max_height(180.0)`. If decode fails, fall through to file-card.
   - **Non-image or failed decode**: a file card — a `column!` of `Icon::new(Icons::File)` (or `Icons::FileImage` if `mime.starts_with("image/")` but decode failed) + `Text::new(file.name)` + `Text::new(format_file_size(file.size))`. Width capped via `max_width(220.0)` to match text bubbles.

```rust
fn build_file_content(file: &FileAttachment, is_me: bool, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    if file.mime.starts_with("image/") {
        if let Some(image_data) = ImageData::from_bytes(&file.bytes) {
            return WithLayout::new(
                Image::new(image_data),
                Layout::default()
                    .max_width(180.0)
                    .max_height(180.0)
                    .flex_shrink(0.0),
            ).boxed();
        }
    }
    column! {
        Icon::new(Icons::FileImage).with_color(icon_color),
        Text::new(file.name.as_str()).with_font_size(14.0).with_color(text_color),
        Text::new(format_file_size(file.size).as_str()).with_font_size(12.0).with_color(muted_color),
    }
    .gap(4.0)
    .padding(BUBBLE_CONTENT_PADDING)
    .boxed()
}
```

- **No thumbnail caching across renders**: `ImageData::from_bytes` is called fresh in `build_bubble` on every rebuild. For a chat with few file messages this is fine; if it becomes a hot path, the `AvatarState` pattern (cache decoded `ImageData` in State, invalidate on source change) is the documented escape hatch. Out of scope here.
- **Color rules** mirror today's text bubble: `theme.on_primary` for "me", `theme.on_surface` for "them". The muted size label uses `theme.on_surface.with_alpha(0.6)`.
- **`format_file_size`**: tiny helper (e.g. `<1 KB`, `2.3 MB`). Inlined in `chat_screen.rs`.

## Construction-site updates (~20 sites)

Changing `Message.text: String` → `Message.kind: MessageKind` ripples through:

- **Seed data** (`data.rs:401-455`): wrap text in `MessageKind::Text(...)`.
- **`on_send` closures** (`mod.rs:90-101`, `desktop.rs:100-111`): receive `MessageKind`, push `Message { kind: payload, ... }`.
- **Tests** (`chat_screen.rs:437+`, ~15 sites; `data.rs:590-608` reaction-test fixtures): wrap text in `MessageKind::Text(...)`. Introduce a `test_file_picker()` helper in `test_util.rs` returning `Arc<dyn FilePicker>` (the `NoopFilePicker` stub) so existing tests don't need the real picker.

## Tests

- **Update** existing `ChatScreen` tests (`chat_screen.rs:437+`) — all construction sites pass `file_picker: Arc::new(NoopFilePicker)` (or the `test_file_picker()` helper) since they test text rendering.
- **New**: `test_attach_button_opens_picker_and_sends_file_message` — mock `FilePicker` returns canned `PickedFile { bytes: <png bytes> }`; simulate attach-button tap; assert `MessageKind::File` appears in render tree (walk for the thumbnail `Image` or the filename `Text`).
- **New**: `test_file_message_renders_image_thumbnail` — seed a `Message { kind: File(FileAttachment{ bytes: png, ... }) }`; assert an `ImageRenderObject` appears in the render tree via the `find_text_in_tree`-style recursion.
- **New**: `test_file_message_renders_file_card_for_non_image` — seed `Message { kind: File(... non-image ...) }`; assert the filename + size text appears in the render tree.
- **New**: `test_picker_returns_none_does_not_send` — mock returns `None`; assert no new message.
- **New**: `test_picker_rejects_oversize_file` — in `vexo/src/platform/file_picker.rs` unit tests, assert `pick_file` returns `None` for a >10MiB file (test `MAX_FILE_BYTES` gating directly).

## Files touched summary

| File | Change |
|---|---|
| `vexo/Cargo.toml` | + `rfd = "0.14"` (desktop target cfg) |
| `vexo/src/platform/mod.rs` | + `pub mod file_picker;` + `default_file_picker` re-export |
| `vexo/src/platform/file_picker.rs` | NEW — trait, `PickedFile`, `RfdFilePicker`, `NoopFilePicker`, `MAX_FILE_BYTES` |
| `vexo/src/lib.rs:165-167` | re-export `file_picker` module |
| `shared_app/src/data.rs` | + `MessageKind`, `FileAttachment`; change `Message.text`→`kind`; update `seed()` + `ImState` field |
| `shared_app/src/app.rs` | update `ImState` construction |
| `shared_app/src/chats/mod.rs:85` | + `file_picker` field on `ChatScreen` |
| `shared_app/src/chats/desktop.rs:95` | + `file_picker` field on `ChatScreen` |
| `shared_app/src/chats/chat_screen.rs` | + field, `on_send`→`Fn(MessageKind)`, `build_input_bar`+attach button, `build_bubble` branch, `build_file_content`, ~15 test site updates + new tests |
| `shared_app/src/test_util.rs` | + `test_file_picker()` helper |
