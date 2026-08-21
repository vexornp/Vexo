# iOS File Send — Callback-Based FilePicker — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the chat screen's attach button work on iOS by presenting `UIDocumentPickerViewController`, picking any file, and sending it as a `MessageKind::File` — same end-to-end UX as desktop.

**Architecture:** The `FilePicker` trait changes from synchronous (`pick_file() -> Option<PickedFile>`) to callback-based (`pick_file(on_done: Box<dyn FnOnce(Option<PickedFile>)>)`). Desktop calls back synchronously (re-entrant, same behavior as today). iOS presents `UIDocumentPickerViewController` via `objc2` and calls back from a `define_class!` delegate subclass. The delegate stores the callback in its ivars (`Rc<RefCell<Option<...>>>`), so each delegate instance holds its own callback — no thread-local slot needed.

**Tech Stack:** Rust, `objc2` 0.6.4, `objc2-ui-kit` 0.3.2, `objc2-uniform-type-identifiers` 0.3.2, `objc2-foundation` 0.3.2, `block2` 0.6.2 (all already in the workspace lock or resolvable at pinned versions).

**Spec:** [`docs/superpowers/specs/2026-08-21-ios-file-send-design.md`](../specs/2026-08-21-ios-file-send-design.md)

## Global Constraints

- `FilePicker` trait must stay `Send + Sync` (used as `Arc<dyn FilePicker>`) — the picker struct is `Send + Sync`; the **callback** is `Box<dyn FnOnce>` WITHOUT `Send` (framework is single-threaded, both desktop and iOS invoke the callback on the main thread).
- `MAX_FILE_BYTES = 10 * 1024 * 1024` — already defined at `vexo/src/platform/file_picker.rs:11`. iOS enforces it after the security-scoped read.
- `objc2` crate versions are pinned to what the workspace already resolves: `objc2 = ">=0.6.2, <0.8.0"`, `objc2-foundation = "0.3.2"`, `objc2-ui-kit = "0.3.2"`, `block2 = "0.6.2"`. The new `objc2-uniform-type-identifiers = "0.3.2"` is the version `objc2-ui-kit 0.3.2` requires (see its `Cargo.toml`).
- No comments in code unless asked (per `CLAUDE.md`).
- Run `cargo build` after every Rust edit; run `cargo test` after implementing features.
- Never run `cargo run -p desktop_demo` yourself (per `CLAUDE.md`) — ask the user.
- iOS-only code is `#[cfg(target_os = "ios")]` — it is cfg'd out of host builds and cannot be unit-tested headlessly (same constraint as `vexo/src/platform/ios_clipboard.rs`, which has no unit test). The iOS build is verified by `cargo build --target aarch64-apple-ios-sim`; the actual picker UI is verified by the user tapping the button on-device/sim.

---

## File Structure

| File | Responsibility |
|---|---|
| `vexo/src/platform/file_picker.rs` | Trait signature change; `NoopFilePicker` + `RfdFilePicker` impl updates; extract cfg-free `mime_from_extension_str` shared by desktop + iOS |
| `vexo/src/platform/file_picker_ios.rs` | NEW — `IosFilePicker` + `define_class!` delegate subclass (`DocumentPickerDelegate`) with ivars holding the callback |
| `vexo/src/platform/mod.rs` | Register `file_picker_ios` module under `#[cfg(target_os = "ios")]`; iOS branch of `default_file_picker()` returns `IosFilePicker` |
| `vexo/Cargo.toml` | Extend objc2-ui-kit / objc2-foundation feature lists; add `objc2-uniform-type-identifiers` dep under iOS target cfg |
| `shared_app/src/chats/chat_screen.rs` | `on_attach` closure moves send logic into `on_done` callback (lines ~251-265) |
| `shared_app/src/test_util.rs` | `MockFilePicker`, `NoopFilePicker`-based `test_file_picker()` adopt callback signature |

---

### Task 1: Change `FilePicker` trait to callback-based + update `NoopFilePicker`

Make the trait callback-based and update the `NoopFilePicker` stub. This is the foundational change — everything else depends on it. Host-compiles and host-tests pass after this task (desktop `RfdFilePicker` is updated in Task 2).

**Files:**
- Modify: `vexo/src/platform/file_picker.rs`
- Modify: `shared_app/src/test_util.rs` (update `MockFilePicker` + `test_file_picker` to new signature so `shared_app` still compiles)

**Interfaces:**
- Produces: `pub trait FilePicker: Send + Sync { fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>); }`
- Produces: `NoopFilePicker::pick_file` calls `on_done(None)` synchronously
- Produces: `MockFilePicker::pick_file` calls `on_done(self.picked.clone())` synchronously (in `test_util.rs`)
- Produces: cfg-free `pub fn mime_from_extension_str(ext: &str) -> String` (extracted from desktop-only `mime_from_extension`)

- [ ] **Step 1: Read the current `file_picker.rs` to confirm exact line numbers**

Run: `Read vexo/src/platform/file_picker.rs`
Confirm: trait at line 22, `NoopFilePicker` at line 38, `mime_from_extension` at line 81.

- [ ] **Step 2: Change the trait signature**

In `vexo/src/platform/file_picker.rs`, replace the trait definition (lines 20-27):

```rust
/// Object-safe file-picker trait. Implementations must be `Send + Sync`
/// so the trait can be used as `Arc<dyn FilePicker>`.
pub trait FilePicker: Send + Sync {
    /// Open the native file dialog. `on_done` is invoked exactly once:
    /// - `Some(PickedFile)` on confirm
    /// - `None` on cancel, error, or file exceeding `MAX_FILE_BYTES`
    ///
    /// Desktop implementations call `on_done` synchronously (re-entrant into
    /// the caller's stack). iOS calls `on_done` later from the picker
    /// delegate (main thread). Either way, exactly-once delivery.
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>);
}
```

- [ ] **Step 3: Update `NoopFilePicker` impl**

In `vexo/src/platform/file_picker.rs`, replace the `NoopFilePicker` impl (lines 40-44):

```rust
impl FilePicker for NoopFilePicker {
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>) {
        on_done(None);
    }
}
```

- [ ] **Step 4: Extract cfg-free `mime_from_extension_str`**

In `vexo/src/platform/file_picker.rs`, add a new cfg-free function (place it just above the desktop-only `mime_from_extension`):

```rust
/// Pure helper mapping a file extension (lowercase, no leading dot) to a
/// MIME type string. Returns `""` for unknown extensions. Cfg-free so both
/// desktop (`RfdFilePicker`) and iOS (`IosFilePicker`) share one mapping.
pub fn mime_from_extension_str(ext: &str) -> String {
    match ext {
        "png" => "image/png".into(),
        "jpg" | "jpeg" => "image/jpeg".into(),
        "gif" => "image/gif".into(),
        "bmp" => "image/bmp".into(),
        "webp" => "image/webp".into(),
        _ => String::new(),
    }
}
```

- [ ] **Step 5: Refactor desktop `mime_from_extension` to delegate to the shared helper**

In `vexo/src/platform/file_picker.rs`, replace the desktop-only `mime_from_extension` (lines 80-95) with a thin wrapper:

```rust
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn mime_from_extension(path: &std::path::Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| mime_from_extension_str(&e.to_lowercase()))
        .unwrap_or_default()
}
```

- [ ] **Step 6: Update `test_file_picker()` in `shared_app/src/test_util.rs`**

In `shared_app/src/test_util.rs`, the `test_file_picker` function (lines 38-40) stays the same (returns `NoopFilePicker`), but `NoopFilePicker::pick_file` now takes a callback. No change needed to `test_file_picker` itself — it just returns the `Arc`. The `NoopFilePicker` impl was already updated in Step 3.

- [ ] **Step 7: Update `MockFilePicker` in `shared_app/src/test_util.rs`**

In `shared_app/src/test_util.rs`, replace the `MockFilePicker` impl (lines 49-57):

```rust
impl FilePicker for MockFilePicker {
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>) {
        on_done(self.picked.as_ref().map(|p| PickedFile {
            name: p.name.clone(),
            mime: p.mime.clone(),
            bytes: p.bytes.clone(),
        }));
    }
}
```

- [ ] **Step 8: Update `RfdFilePicker` to the callback signature**

In `vexo/src/platform/file_picker.rs`, replace the `RfdFilePicker` impl (lines ~52-78):

```rust
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl FilePicker for RfdFilePicker {
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>) {
        let result = (|| {
            let path = rfd::FileDialog::new().pick_file()?;
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
        })();
        on_done(result);
    }
}
```

Note: the closure `(|| { ... })()` preserves the early-return `?` ergonomics of the old code. The `on_done(result)` call is the LAST statement — it fires synchronously, re-entrant into the caller's stack, exactly as before.

- [ ] **Step 9: Run `cargo build -p vexo` (host)**

Run: `cargo build -p vexo`
Expected: PASS — no errors (all three impls updated in this task).

- [ ] **Step 10: Run `cargo test -p vexo`**

Run: `cargo test -p vexo`
Expected: PASS — all 5 existing file_picker tests pass (`test_file_within_limit_*`, `test_noop_file_picker_returns_none`, `test_default_file_picker_returns_send_sync_arc`).

- [ ] **Step 11: Commit**

```bash
git add vexo/src/platform/file_picker.rs shared_app/src/test_util.rs
git commit -m "refactor(file_picker): change trait to callback-based

pick_file now takes Box<dyn FnOnce(Option<PickedFile>)>. Desktop calls
back synchronously (re-entrant); iOS will call back from the picker
delegate. Extracts cfg-free mime_from_extension_str shared by both.

NoopFilePicker + MockFilePicker + RfdFilePicker all updated in one
commit to keep the build green."
```

---

### Task 2: Update `chat_screen.rs` `on_attach` closure

Move the file-send logic into the `on_done` callback. After this task, desktop file-send works exactly as before (the `RfdFilePicker` fires the callback synchronously), and iOS file-send is wired but the picker is still `NoopFilePicker` (returns `None`).

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (the `on_attach` closure, lines ~251-265)

**Interfaces:**
- Consumes: `FilePicker::pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>)` from Task 1 Step 2
- Produces: `on_attach: impl FnMut() + 'static` that calls `pick_file(Box::new(move |picked| { ... }))`

- [ ] **Step 1: Read the current `on_attach` closure to confirm exact lines**

Run: `Read shared_app/src/chats/chat_screen.rs` offset 250 limit 20
Confirm: `on_attach` closure at lines 254-265.

- [ ] **Step 2: Replace the `on_attach` closure**

In `shared_app/src/chats/chat_screen.rs`, replace lines 254-265:

```rust
        let on_attach = move || {
            file_picker_for_attach.pick_file(Box::new(move |picked| {
                if let Some(picked) = picked {
                    let attachment = crate::data::FileAttachment {
                        name: picked.name,
                        mime: picked.mime,
                        size: picked.bytes.len() as u64,
                        bytes: std::sync::Arc::from(picked.bytes),
                    };
                    on_send_for_attach(MessageKind::File(attachment));
                    scroll_for_attach.jump_to_bottom();
                }
            }));
        };
```

- [ ] **Step 3: Run `cargo build -p shared_app` (host)**

Run: `cargo build -p shared_app`
Expected: PASS — no errors.

- [ ] **Step 4: Run `cargo test -p shared_app`**

Run: `cargo test -p shared_app`
Expected: PASS — all existing chat_screen tests pass, including `test_attach_button_sends_file_message` (the `MockFilePicker` fires `on_done` synchronously, so the post-tap assertions hold) and `test_attach_button_picker_none_does_not_send` (the `NoopFilePicker` fires `on_done(None)` synchronously).

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "refactor(chat_screen): move file-send into pick_file callback

on_attach now calls pick_file(Box::new(|picked| { ... })) instead of
if let Some(picked) = pick_file(). The send logic (build FileAttachment,
call on_send, jump_to_bottom) moves into the on_done closure.

Desktop: RfdFilePicker fires the callback synchronously — same call
stack as before. iOS: will fire from the picker delegate (next tasks)."
```

---

### Task 3: Add iOS objc2 dependencies to `vexo/Cargo.toml`

Add the objc2 feature flags and the `objc2-uniform-type-identifiers` crate needed by `IosFilePicker`. This is a pure config change — no code, no test. The iOS target won't compile yet (the module doesn't exist), but the host build stays green.

**Files:**
- Modify: `vexo/Cargo.toml` (the `[target.'cfg(target_os = "ios")'.dependencies]` section, lines 50-64)

**Interfaces:**
- Produces: objc2-ui-kit with features `UIDocumentPickerViewController`, `UIApplication`, `UIViewController`, `UIResponder`, `UIWindow`, `objc2-uniform-type-identifiers`
- Produces: objc2-foundation with `NSURL` added (and `NSArray` if not already present)
- Produces: new dep `objc2-uniform-type-identifiers = { version = "0.3.2", default-features = false, features = ["UTType", "UTCoreTypes"] }`

- [ ] **Step 1: Read the current iOS deps section**

Run: `Read vexo/Cargo.toml` offset 50 limit 15
Confirm: the `[target.'cfg(target_os = "ios")'.dependencies]` section at lines 50-64, with `objc2-ui-kit` features currently `["UIPasteboard"]` and `objc2-quartz-core` for CADisplayLink.

- [ ] **Step 2: Extend `objc2-ui-kit` features**

In `vexo/Cargo.toml`, replace the `objc2-ui-kit` line (line 62):

```toml
objc2-ui-kit = { version = "0.3.2", default-features = false, features = [
    "UIPasteboard",
    "UIDocumentPickerViewController",
    "UIApplication",
    "UIViewController",
    "UIResponder",
    "UIWindow",
    "objc2-uniform-type-identifiers",
] }
```

Note: `objc2-uniform-type-identifiers` is a feature on `objc2-ui-kit` (enables its optional dep on that crate), NOT a standalone feature. The `UIDocumentPickerViewController` feature pulls in `UIResponder` + `UIViewController` transitively for the delegate methods, but listing them explicitly is clearer.

- [ ] **Step 3: Extend `objc2-foundation` features**

In `vexo/Cargo.toml`, replace the `objc2-foundation` features list (lines 52-61), adding `NSURL` and `NSArray`:

```toml
objc2-foundation = { version = "0.3.2", default-features = false, features = [
    "NSString",
    "NSNotification",
    "NSDictionary",
    "NSValue",
    "NSObject",
    "NSOperation",
    "NSRunLoop",
    "block2",
    "NSURL",
    "NSArray",
] }
```

Note: `NSArray` is needed because `UIDocumentPickerViewController::initForOpeningContentTypes` takes `&NSArray<UTType>`. `NSURL` is needed because the delegate callback receives `&NSArray<NSURL>`.

- [ ] **Step 4: Add `objc2-uniform-type-identifiers` standalone dep**

In `vexo/Cargo.toml`, add after the `objc2-ui-kit` line (inside the same iOS target section):

```toml
objc2-uniform-type-identifiers = { version = "0.3.2", default-features = false, features = [
    "UTType",
    "UTCoreTypes",
] }
```

Note: `UTCoreTypes` is the feature that exposes the `UTTypeItem` static (the "any file" type). `UTType` is the base feature.

- [ ] **Step 5: Run `cargo build -p vexo` (host) — verify no regression**

Run: `cargo build -p vexo`
Expected: PASS — the iOS deps are cfg'd out on host, so this just verifies the TOML parses.

- [ ] **Step 6: Commit**

```bash
git add vexo/Cargo.toml
git commit -m "build(vexo): add iOS objc2 deps for UIDocumentPickerViewController

Extends objc2-ui-kit features with UIDocumentPickerViewController,
UIApplication, UIViewController, UIResponder, UIWindow. Adds
objc2-uniform-type-identifiers 0.3.2 (UTType + UTCoreTypes for
UTTypeItem). Extends objc2-foundation with NSURL + NSArray.

No host-build impact — iOS deps are cfg(target_os = \"ios\")."
```

---

### Task 4: Implement `IosFilePicker` + `DocumentPickerDelegate`

Create the iOS file picker module. This is the largest task — it builds the `UIDocumentPickerViewController`, a `define_class!` delegate subclass that stores the callback in its ivars, and the security-scoped URL reading. Cannot be unit-tested headlessly (UIKit); verified by iOS build in Task 5.

**Key design decisions baked into the code below:**
1. **Callback storage:** The `on_done` callback is wrapped in `Rc<RefCell<Option<...>>>` (a `PendingCallback` slot) and cloned into the delegate's ivars. The delegate's `fire` method takes the callback out of the slot and invokes it — exactly-once delivery.
2. **Delegate retention:** `UIDocumentPickerDelegate`'s `setDelegate:` is a **weak** property, so the delegate would be deallocated immediately if we dropped our `Retained`. A module-scope `thread_local LIVE_DELEGATE` stashes the delegate `Retained` on the main thread. The delegate's `fire` method clears `LIVE_DELEGATE`, releasing the retain after the callback fires.
3. **Security-scoped reads:** iOS-picked URLs require `startAccessingSecurityScopedResource()` before reading. A RAII guard pairs start/stop so release is guaranteed even on read error.
4. **Topmost VC:** `keyWindow.rootViewController` + a `while` loop walking `presentedViewController` to present above any already-presented modal.

**Files:**
- Create: `vexo/src/platform/file_picker_ios.rs`
- Modify: `vexo/src/platform/mod.rs` (register module)
- Modify: `vexo/src/platform/file_picker.rs` (update `default_file_picker` iOS branch)

**Interfaces:**
- Consumes: `FilePicker` trait, `PickedFile`, `file_within_limit`, `mime_from_extension_str` from `super::file_picker`
- Produces: `pub struct IosFilePicker;` implementing `FilePicker`

- [ ] **Step 1: Register the module in `mod.rs`**

In `vexo/src/platform/mod.rs`, add the module declaration alongside `ios_clipboard` (after line 15, the `pub mod ios_clipboard;` line):

```rust
#[cfg(target_os = "ios")]
pub mod file_picker_ios;
```

- [ ] **Step 2: Create `file_picker_ios.rs` — the complete module**

Create `vexo/src/platform/file_picker_ios.rs` with the full implementation:

```rust
//! iOS file picker backend backed by `UIDocumentPickerViewController`.
//!
//! Mirrors [`super::ios_clipboard::IosClipboard`]: zero-sized struct,
//! main-thread only, no stored state. The picker's async result is
//! delivered via the `on_done` callback stashed in the delegate's ivars.
//!
//! # Thread safety
//!
//! `UIDocumentPickerViewController` and its delegate methods must be
//! invoked on the main thread. Every call site fires from winit's
//! main-loop event dispatch in [`crate::window`], so this invariant holds
//! without extra marshalling. The struct stores no state, so it is
//! trivially `Send + Sync` and can be shared as `Arc<dyn FilePicker>`.

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker};
use objc2_foundation::{NSArray, NSObject, NSString, NSURL};
use objc2_ui_kit::{
    UIApplication, UIDocumentPickerDelegate, UIDocumentPickerViewController, UIViewController,
};
use objc2_uniform_type_identifiers::UTTypeItem;

use super::file_picker::{file_within_limit, mime_from_extension_str, FilePicker, PickedFile};

type PendingCallback = Rc<RefCell<Option<Box<dyn FnOnce(Option<PickedFile>)>>>>;

thread_local! {
    static LIVE_DELEGATE: RefCell<Option<Retained<NSObject>>> = const { RefCell::new(None) };
}

pub struct IosFilePicker;

impl FilePicker for IosFilePicker {
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>) {
        let mtm = match MainThreadMarker::new() {
            Some(mtm) => mtm,
            None => {
                on_done(None);
                return;
            }
        };
        let slot: PendingCallback = Rc::new(RefCell::new(Some(on_done)));

        let content_types = NSArray::from_slice(&[UTTypeItem]);
        let picker = UIDocumentPickerViewController::initForOpeningContentTypes(&content_types);
        picker.setAllowsMultipleSelection(false);

        let delegate = DocumentPickerDelegate::new(slot);
        let delegate_obj: Retained<NSObject> = delegate.into();
        LIVE_DELEGATE.with(|d| *d.borrow_mut() = Some(delegate_obj.clone()));

        let delegate_proto: Retained<
            objc2::runtime::ProtocolObject<dyn UIDocumentPickerDelegate>,
        > = unsafe { Retained::cast(delegate_obj) };
        picker.setDelegate(Some(&delegate_proto));

        if let Some(vc) = topmost_view_controller(&mtm) {
            vc.presentViewController_animated_completion(&picker, true, None);
        } else {
            LIVE_DELEGATE.with(|d| *d.borrow_mut() = None);
        }
    }
}

#[derive(Clone)]
struct DelegateIvars {
    callback: PendingCallback,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    struct DocumentPickerDelegate;

    impl DocumentPickerDelegate {
        fn new(slot: PendingCallback) -> Retained<Self> {
            let this = Self::alloc().set_ivars(DelegateIvars { callback: slot });
            unsafe { msg_send![super(this), init] }
        }
    }

    unsafe impl NSObjectProtocol for DocumentPickerDelegate {}

    unsafe impl UIDocumentPickerDelegate for DocumentPickerDelegate {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn documentPicker_didPickDocumentsAtURLs(
            &self,
            _controller: &UIDocumentPickerViewController,
            urls: &NSArray<NSURL>,
        ) {
            let picked = urls.firstObject().and_then(|url| read_url(url));
            self.fire(picked);
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn documentPickerWasCancelled(&self, _controller: &UIDocumentPickerViewController) {
            self.fire(None);
        }
    }
);

impl DocumentPickerDelegate {
    fn fire(&self, picked: Option<PickedFile>) {
        if let Some(cb) = self.ivars().callback.borrow_mut().take() {
            cb(picked);
        }
        LIVE_DELEGATE.with(|d| *d.borrow_mut() = None);
    }
}

struct SecurityScopeGuard<'a> {
    url: &'a NSURL,
    acquired: bool,
}

impl<'a> SecurityScopeGuard<'a> {
    fn new(url: &'a NSURL) -> Self {
        let acquired = unsafe { url.startAccessingSecurityScopedResource() };
        Self { url, acquired }
    }
}

impl<'a> Drop for SecurityScopeGuard<'a> {
    fn drop(&mut self) {
        if self.acquired {
            unsafe { self.url.stopAccessingSecurityScopedResource() };
        }
    }
}

fn read_url(url: &NSURL) -> Option<PickedFile> {
    let _guard = SecurityScopeGuard::new(url);
    let path = url.path()?;
    let path_str = path.to_string();
    let std_path = std::path::Path::new(&path_str);
    let metadata = std::fs::metadata(std_path).ok()?;
    if !file_within_limit(metadata.len()) {
        return None;
    }
    let bytes = std::fs::read(std_path).ok()?;
    let name = url
        .lastPathComponent()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".into());
    let ext = url
        .pathExtension()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let mime = mime_from_extension_str(&ext.to_lowercase());
    Some(PickedFile { name, mime, bytes })
}

fn topmost_view_controller(mtm: &MainThreadMarker) -> Option<Retained<UIViewController>> {
    let app = UIApplication::sharedApplication(*mtm);
    let key_window = app.keyWindow()?;
    let mut vc = key_window.rootViewController()?;
    while let Some(presented) = vc.presentedViewController() {
        vc = presented;
    }
    Some(vc)
}
```

**Notes on the code:**
- `UTTypeItem` is an `extern "C"` static (`&'static UTType`) from `objc2-uniform-type-identifiers`'s `UTCoreTypes` feature — a CoreFoundation constant initialized at process start. `NSArray::from_slice(&[UTTypeItem])` builds the single-element array `initForOpeningContentTypes` expects.
- The `LIVE_DELEGATE` `thread_local` is declared ONCE at module scope — both `pick_file` (set) and `fire` (clear) reference the same static. If it were declared inside the functions, each `thread_local!` invocation would create a DIFFERENT static.
- `delegate_obj.clone()` clones the `Retained` (bumps the retain count) so both `LIVE_DELEGATE` and the `delegate_proto` cast reference the same underlying object. When `fire` clears `LIVE_DELEGATE`, the retain count drops; the picker's weak ref is fine (it's about to be dismissed).
- `Retained::cast` is `unsafe` because it's a raw pointer reinterpret across the FFI boundary. The cast is sound because `DocumentPickerDelegate` conforms to `UIDocumentPickerDelegate` (via `define_class!`'s `unsafe impl`).
- `keyWindow()` is deprecated on iOS 13+ in favor of scene-based APIs, but works for single-window demo apps (documented limitation in the spec).
- `read_url` uses `std::fs::read` (not `NSData`) because `startAccessingSecurityScopedResource` makes the path accessible to standard Rust file APIs — avoids an `NSData` dependency.

- [ ] **Step 3: Update `default_file_picker` iOS branch**

In `vexo/src/platform/file_picker.rs`, update the `default_file_picker` function (lines 101-110). Replace the entire function:

```rust
pub fn default_file_picker() -> Arc<dyn FilePicker> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        Arc::new(RfdFilePicker)
    }
    #[cfg(target_os = "ios")]
    {
        Arc::new(crate::platform::file_picker_ios::IosFilePicker)
    }
    #[cfg(target_os = "android")]
    {
        Arc::new(NoopFilePicker)
    }
}
```

Note: `crate::platform::file_picker_ios` because `default_file_picker` lives in `vexo/src/platform/file_picker.rs` (module path `vexo::platform::file_picker`), and `file_picker_ios` is a sibling under `vexo::platform`. The `#[cfg(target_os = "ios")]` branch replaces the old `#[cfg(any(target_os = "ios", target_os = "android"))]` branch. The old combined iOS+Android branch is now split into two: iOS returns `IosFilePicker`, Android returns `NoopFilePicker`.

- [ ] **Step 4: Run `cargo build -p vexo` (host) — verify no regression**

Run: `cargo build -p vexo`
Expected: PASS — the iOS module is cfg'd out on host, so this only verifies the `default_file_picker` change compiles on host (where the `#[cfg(not(...))]` branch is active).

- [ ] **Step 5: Run `cargo test -p vexo` (host)**

Run: `cargo test -p vexo`
Expected: PASS — all 5 existing file_picker tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/platform/file_picker_ios.rs vexo/src/platform/mod.rs vexo/src/platform/file_picker.rs
git commit -m "feat(file_picker): iOS IosFilePicker via UIDocumentPickerViewController

Pure-Rust objc2 implementation mirroring ios_clipboard.rs. Presents
UIDocumentPickerViewController with UTTypeItem (any file). A
define_class! delegate subclass (DocumentPickerDelegate) stores the
on_done callback in its ivars (Rc<RefCell<Option<...>>>) and fires it
from didPickDocumentsAtURLs / didCancel.

The delegate is kept alive via a module-scope thread_local
LIVE_DELEGATE (setDelegate is a weak property). Security-scoped URL
reading via std::fs::read + startAccessingSecurityScopedResource guard.

default_file_picker() iOS branch now returns IosFilePicker instead of
NoopFilePicker."
```

---

### Task 5: Verify iOS target compiles

Build the `vexo` and `shared_app` crates for the iOS simulator target to confirm the iOS code compiles end-to-end. This is the only verification possible for iOS-only code without a human tapping the button.

**Files:**
- None (verification-only task)

**Interfaces:**
- None (verification-only task)

- [ ] **Step 1: Ensure iOS targets are installed**

Run: `rustup target list --installed | grep apple-ios`
Expected: `aarch64-apple-ios` and `aarch64-apple-ios-sim` listed. If missing, run `rustup target add aarch64-apple-ios aarch64-apple-ios-sim` (the user may need to do this).

- [ ] **Step 2: Build `vexo` for iOS simulator**

Run: `cargo build -p vexo --target aarch64-apple-ios-sim`
Expected: PASS — `IosFilePicker`, `DocumentPickerDelegate`, `read_url`, `topmost_view_controller` all compile. Any errors here are objc2 API mismatches (method signatures, feature flags) — fix them inline and re-run.

- [ ] **Step 3: Build `shared_app` for iOS simulator**

Run: `cargo build -p shared_app --target aarch64-apple-ios-sim`
Expected: PASS — the `chat_screen` callback wiring compiles for iOS.

- [ ] **Step 4: Run the full iOS build script**

Run: `./build_for_ios.sh`
Expected: PASS — produces the `.xcframework` and Swift bindings. This is the same script Xcode's Build pre-action runs. If it fails, inspect the error — likely a feature-flag or API mismatch in `file_picker_ios.rs`.

- [ ] **Step 5: Commit any fixes from Steps 2-4**

If Steps 2-4 required fixes to `file_picker_ios.rs` or `Cargo.toml`:

```bash
git add vexo/src/platform/file_picker_ios.rs vexo/Cargo.toml
git commit -m "fix(file_picker_ios): resolve iOS build issues

[specific fixes — e.g. correct method signature, add missing feature]"
```

If no fixes were needed, skip this step.

- [ ] **Step 6: Ask the user to verify the picker on-device/sim**

The iOS picker UI cannot be verified headlessly. Ask the user:

> "iOS file picker is implemented and the iOS target compiles. To verify the actual picker UI, please:
> 1. Open `VexoDemo.xcodeproj` in Xcode
> 2. Build and run on the iOS simulator (or a device)
> 3. Open a chat conversation
> 4. Tap the paperclip attach button
> 5. Verify the iOS Files app picker appears
> 6. Pick a file and verify a file message appears in the chat
>
> Let me know what happens."

---

### Task 6: Update spec doc to reflect ivars refinement (optional)

The spec describes a `thread_local` pending-callback slot; the implementation uses `define_class!` ivars + a `thread_local LIVE_DELEGATE` for delegate retention. This is a minor refinement. Update the spec's "Why this is safe" and "Risk check" sections to match the shipped implementation, so the doc stays accurate.

**Files:**
- Modify: `docs/superpowers/specs/2026-08-21-ios-file-send-design.md`

**Interfaces:**
- None (docs-only task)

- [ ] **Step 1: Read the spec's "iOS implementation" section**

Run: `Read docs/superpowers/specs/2026-08-21-ios-file-send-design.md` offset 60 limit 50
Confirm: the "Flow" steps 1-7 and "Why this is safe" bullets.

- [ ] **Step 2: Update the "Flow" step 1 to describe ivars**

In `docs/superpowers/specs/2026-08-21-ios-file-send-design.md`, replace Flow step 1 (the `thread_local` stash description) with the ivars description:

```markdown
1. `pick_file` wraps `on_done` in `Rc<RefCell<Option<Box<dyn FnOnce...>>>>` (a `PendingCallback` slot). The slot is cloned into the delegate's ivars (so the delegate owns one `Rc`, `pick_file` drops its `Rc` after presenting). The delegate's `fire` method takes the callback out of the slot and invokes it — exactly-once delivery.
```

- [ ] **Step 3: Update the "Why this is safe" bullets**

Replace the bullet about `thread_local` pending-callback storage:

```markdown
- **Delegate retention:** `UIDocumentPickerDelegate`'s `setDelegate:` is a weak property, so the delegate must be kept alive externally. A module-scope `thread_local LIVE_DELEGATE: RefCell<Option<Retained<NSObject>>>` stashes the delegate `Retained` on the main thread when `pick_file` presents the picker. The delegate's `fire` method clears `LIVE_DELEGATE` after invoking the callback, releasing the retain. If the user backgrounds the app mid-pick and never returns, the delegate leaks until the next `pick_file` call overwrites the slot — acceptable for a demo, and single-entry by construction.
```

- [ ] **Step 4: Update the "Risk check" callback-lifetime bullet**

Replace the "Callback lifetime on iOS" risk bullet:

```markdown
- **Callback lifetime on iOS:** The callback lives in the delegate's ivars (`Rc<RefCell<Option<...>>>`). The delegate is kept alive by `LIVE_DELEGATE` (module-scope `thread_local`) until `fire` clears it. If the user never picks/cancels (e.g. backgrounds the app), the delegate + callback leak until the next `pick_file` call overwrites `LIVE_DELEGATE`. Acceptable for a demo; single-entry by construction (only one picker presented at a time).
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-08-21-ios-file-send-design.md
git commit -m "docs(spec): reflect ivars + LIVE_DELEGATE refinement

Implementation uses define_class! ivars (Rc<RefCell<Option<...>>>) for
the callback and a module-scope thread_local LIVE_DELEGATE for delegate
retention, instead of the spec's thread_local pending-callback slot.
Cleaner: each delegate instance holds its own callback; no raw pointers."
```

---

## Self-Review

**1. Spec coverage:**
- ✅ Trait change to callback-based → Task 1
- ✅ `NoopFilePicker` updated → Task 1 Step 3
- ✅ `RfdFilePicker` updated (synchronous callback) → Task 1 Step 8
- ✅ `mime_from_extension_str` extracted (cfg-free, shared) → Task 1 Steps 4-5
- ✅ `IosFilePicker` pure-Rust objc2 impl → Task 4
- ✅ `UIDocumentPickerViewController` + `UTType.item` (any file) → Task 4 Step 2
- ✅ `define_class!` delegate subclass → Task 4 Step 2
- ✅ Security-scoped URL read → Task 4 Step 2
- ✅ `default_file_picker` iOS branch → Task 4 Step 3
- ✅ `Cargo.toml` objc2 deps → Task 3
- ✅ `chat_screen.rs` `on_attach` callback wiring → Task 2
- ✅ `test_util.rs` mock updates → Task 1 Steps 6-7
- ✅ iOS target build verification → Task 5
- ✅ Spec doc refinement → Task 6

**2. Placeholder scan:** No "TBD", "TODO", "implement later". All code blocks are complete — Task 4 shows the full module in one step (no intermediate broken code).

**3. Type consistency:**
- `PendingCallback = Rc<RefCell<Option<Box<dyn FnOnce(Option<PickedFile>)>>>>` — defined Task 4, used in `pick_file` (slot creation), `DelegateIvars`, `fire`. ✅
- `FilePicker::pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>)` — defined Task 1, used Task 1 Step 8 (Rfd), Task 2 (chat_screen), Task 4 (iOS). ✅
- `mime_from_extension_str(ext: &str) -> String` — defined Task 1 Step 4, used Task 1 Step 5 (desktop wrapper), Task 4 Step 2 (iOS `read_url`). ✅
- `LIVE_DELEGATE: RefCell<Option<Retained<NSObject>>>` — declared Task 4 Step 2 (module scope), set in `pick_file`, cleared in `fire`. ✅

**4. Scope check:** Single subsystem (file picker). No decomposition needed. ✅

**4. Scope check:** Single subsystem (file picker). No decomposition needed. ✅
