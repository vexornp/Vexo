# iOS File Send — Callback-Based FilePicker

**Date:** 2026-08-21
**Status:** Approved (pre-implementation)
**Scope:** `vexo/src/platform/file_picker*.rs`, `shared_app/src/chats/chat_screen.rs`, `shared_app/src/test_util.rs`, `vexo/Cargo.toml`
**Depends on:** [`2026-08-21-send-file-message-design.md`](./2026-08-21-send-file-message-design.md) (the original desktop-only design — already implemented)

## Goal

Extend file-send support to iOS. Today the attach button in `ChatScreen` is a no-op on iOS because `default_file_picker()` returns `NoopFilePicker` (the original spec explicitly listed iOS as a non-goal). This spec closes that gap: tapping the paperclip on iOS presents `UIDocumentPickerViewController`, the user picks a file, and a `MessageKind::File` is appended to the conversation — same end-to-end UX as desktop.

## Why the trait must change

The original `FilePicker::pick_file(&self) -> Option<PickedFile>` is synchronous. This is honest on desktop (rfd's `NSOpenPanel` runs its own message pump, so blocking the calling thread is fine). It is **unimplementable** on iOS:

- `UIDocumentPickerViewController` is presented modally and reports its result via a delegate callback (`documentPicker:didPickDocumentsAtURLs:` / `documentPickerDidCancel:`).
- The delegate callback is delivered on the main thread's run loop.
- Blocking the main thread waiting for that callback deadlocks: the callback needs the main thread run loop to fire, but the main thread is blocked in `pick_file`.

A run-loop pump inside `pick_file` is rejected as fragile (re-entrancy into winit's event loop, watchdog kills, fights the framework's own run loop ownership — see `keyboard_ios.rs` for the precedent of letting the OS call back rather than pumping).

**Decision:** Make the trait callback-based. The picker calls `on_done` exactly once — synchronously on desktop, asynchronously on iOS. The framework is single-threaded (`Rc`-based Signals), and both platforms invoke the callback on the main thread, so `Rc`-capturing closures work without `Send` bounds.

## Non-Goals

- Swift/UniFFI bridge for the picker — pure Rust via `objc2`, matching `ios_clipboard.rs` / `keyboard_ios.rs` precedent.
- `PHPickerViewController` (photo-library UI) — accept any file via `UTType.item`, matching desktop's accept-any-file behavior.
- Multiple file selection — single pick per tap (same as desktop).
- File captions, persistence, network upload, tap-to-open received files — same non-goals as the original spec.
- Progress indication for large reads — still capped at 10 MiB (`MAX_FILE_BYTES`), now enforced inside `IosFilePicker` after the security-scoped read.

## Decisions (settled during brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| Trait shape | Callback-based: `pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>)` | Only honest model for iOS's async delegate; desktop calls back synchronously. Minimal ripple — `chat_screen` `on_attach` + test mocks only. |
| iOS picker impl | Pure Rust via `objc2-ui-kit` (`UIDocumentPickerViewController` + `define_class!` delegate) | Matches `ios_clipboard.rs` / `keyboard_ios.rs` precedent. Self-contained, no Swift/UniFFI plumbing. |
| File scope on iOS | `UTType.item` (any file) | Matches desktop's accept-any-file behavior exactly. |
| Callback `Send` bound | None — `Box<dyn FnOnce>` without `Send` | Framework is single-threaded; both desktop and iOS fire the callback on the main thread. Avoids forcing `Arc`/`Mutex` on `Rc`-based closures. |
| Pending callback storage on iOS | `Rc<RefCell<Option<Box<dyn FnOnce...>>>>` cloned into `define_class!` delegate ivars; delegate retained by module-scope `thread_local LIVE_DELEGATE` | Each delegate instance holds its own `Rc` to the callback slot. `setDelegate:` is a weak property, so `LIVE_DELEGATE` stashes the `Retained<NSObject>` until `fire` clears it. No raw pointers across the FFI boundary. |
| `default_file_picker()` return type | Stays `Arc<dyn FilePicker>` (unchanged from original spec) | Only the trait *method* changed, not the field types. `ImState.file_picker`, `ChatScreen.file_picker`, builder args all unchanged. |

## Architecture

### Trait change (`vexo/src/platform/file_picker.rs`)

```rust
pub trait FilePicker: Send + Sync {
    /// Open the native file dialog. `on_done` is invoked exactly once:
    /// - `Some(PickedFile)` on confirm
    /// - `None` on cancel, error, or file exceeding MAX_FILE_BYTES
    ///
    /// Desktop implementations call `on_done` synchronously (re-entrant into
    /// the caller's stack). iOS calls `on_done` later from the picker
    /// delegate (main thread). Either way, exactly-once delivery.
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>);
}
```

- `PickedFile`, `MAX_FILE_BYTES`, `file_within_limit` — unchanged from the original spec.
- **`mime_from_extension` refactor:** today this helper is `#[cfg(not(any(target_os = "ios", target_os = "android")))]` and takes `&std::path::Path`. To share it with iOS (which has a URL string, not a `Path`), extract a cfg-free `pub fn mime_from_extension_str(ext: &str) -> String` containing the match body; the desktop `RfdFilePicker` calls it with `path.extension().to_str()`, the iOS picker calls it with the URL's `pathExtension`. The existing `mime_from_extension(&Path)` becomes a thin desktop-only wrapper (or is inlined). No behavior change.
- **`NoopFilePicker`**: `on_done(None)` synchronously.
- **`RfdFilePicker`** (desktop): does the existing sync rfd + `std::fs::read` + size gate, then `on_done(Some(...))` (or `None`) **synchronously inside the `pick_file` call**. Behavior identical to today, just delivered via the callback. The desktop path stays re-entrant into the tap handler — same call stack as before the trait change.
- Callback is `Box<dyn FnOnce(Option<PickedFile>)>` **without `Send`**. The picker is `Send + Sync` (it's a zero-sized struct on both platforms); the callback is boxed on the main thread, invoked on the main thread, dropped on the main thread — no cross-thread moves.

### iOS implementation (`vexo/src/platform/file_picker_ios.rs` — NEW)

Pure Rust via `objc2`, mirroring `ios_clipboard.rs` (zero-sized struct, main-thread only, no stored state).

```rust
pub struct IosFilePicker;
impl FilePicker for IosFilePicker {
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>) { ... }
}
```

**Flow:**

1. `pick_file` wraps `on_done` in `Rc<RefCell<Option<Box<dyn FnOnce...>>>>` (a `PendingCallback` slot). The slot is cloned into the delegate's ivars (so the delegate owns one `Rc`, `pick_file` drops its `Rc` after presenting). The delegate's `fire` method takes the callback out of the slot and invokes it — exactly-once delivery.
2. Builds a `UIDocumentPickerViewController` configured with `UTType.item` (any file) via `forOpeningContentTypes:`.
3. Creates a delegate instance via `define_class!` — a subclass of `NSObject` conforming to `UIDocumentPickerDelegate`. The delegate stores the `PendingCallback` slot (`Rc<RefCell<Option<...>>>`) in its ivars. The delegate `Retained<NSObject>` is stashed in a module-scope `thread_local LIVE_DELEGATE` because `setDelegate:` is a **weak** property (the delegate would be deallocated immediately without external retention). The delegate's `fire` method clears `LIVE_DELEGATE` after invoking the callback, releasing the retain.
4. Resolves the topmost `UIViewController` via `UIApplication.sharedApplication` → key window → `rootViewController`, walking `presentedViewController` to the topmost presented VC.
5. Presents the picker `animated:true`. `pick_file` returns immediately.
6. **`documentPicker:didPickDocumentsAtURLs:`** (delegate method): takes the first URL, calls `startAccessingSecurityScopedResource` (returns `bool`; if false, the read will fail and we surface `None`), reads bytes via `std::fs::read` (works because the security-scope start makes the path accessible to standard Rust file APIs — avoids an `NSData` dependency), infers name from the URL's `lastPathComponent` and MIME from the extension via the same `mime_from_extension` helper the desktop picker uses (extracted to a shared `fn`), enforces `MAX_FILE_BYTES` via `file_within_limit`, builds `PickedFile`, invokes the stashed callback with `Some`, clears the slot. A RAII guard struct pairs the security-scope start/stop so `stopAccessingSecurityScopedResource` is guaranteed even on read error.
7. **`documentPickerDidCancel:`**: invokes callback with `None`, clears slot.

**Thread-safety invariant:** `pick_file` and both delegate callbacks run on the main thread (UIKit hard requirement, same as `ios_clipboard.rs`). The `thread_local` slot sidesteps `Send` issues with the callback.

**Why this is safe:**

- `IosFilePicker` is zero-sized, trivially `Send + Sync`.
- The callback is boxed on the main thread, invoked on the main thread, dropped on the main thread — no cross-thread moves.
- **Delegate retention:** `UIDocumentPickerDelegate`'s `setDelegate:` is a weak property, so the delegate must be kept alive externally. A module-scope `thread_local LIVE_DELEGATE: RefCell<Option<Retained<NSObject>>>` stashes the delegate `Retained` on the main thread when `pick_file` presents the picker. The delegate's `fire` method clears `LIVE_DELEGATE` after invoking the callback, releasing the retain. If the user backgrounds the app mid-pick and never returns, the delegate leaks until the next `pick_file` call overwrites the slot — acceptable for a demo, and single-entry by construction.
- Security-scoped resource access is wrapped in a guard struct so `stopAccessingSecurityScopedResource` always runs.

### `default_file_picker()` update (`vexo/src/platform/mod.rs`)

```rust
pub fn default_file_picker() -> Arc<dyn FilePicker> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    { Arc::new(RfdFilePicker) }
    #[cfg(target_os = "ios")]
    { Arc::new(ios_file_picker::IosFilePicker) }   // was: NoopFilePicker
    #[cfg(target_os = "android")]
    { Arc::new(NoopFilePicker) }
}
```

Module registration under `#[cfg(target_os = "ios")]`, mirroring `ios_clipboard` / `keyboard_ios`.

### `Cargo.toml` additions (`vexo/Cargo.toml`)

Under `[target.'cfg(target_os = "ios")'.dependencies]`, extend the existing objc2 feature lists:

- `objc2-ui-kit`: add features for `UIDocumentPicker`, `UIDocumentPickerViewController`, `UIApplication`, `UIViewController` (exact feature names verified during implementation).
- `objc2-foundation`: add `NSURL` feature (likely already pulled in transitively; explicit is better). `NSData` is **not** needed — bytes are read via `std::fs::read` after `startAccessingSecurityScopedResource`.

No new crate versions — everything is pinned to what the workspace already resolves (objc2 0.6.x / objc2-foundation/ui-kit 0.3.2), so no lock churn.

### `chat_screen.rs` wiring change

The `on_attach` closure (`chat_screen.rs:254`) moves the file-send logic into the `on_done` callback:

**Before:**
```rust
let on_attach = move || {
    if let Some(picked) = file_picker_for_attach.pick_file() {
        let attachment = crate::data::FileAttachment { ... };
        on_send_for_attach(MessageKind::File(attachment));
        scroll_for_attach.jump_to_bottom();
    }
};
```

**After:**
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

**Key points:**

- The inner `on_done` closure captures the same `Rc`s the outer one used to: `on_send_for_attach`, `scroll_for_attach`. Both are `Rc` (not `Send`) — fine because **iOS invokes the callback on the main thread** (same thread the closure was created on, same thread the `Rc`s live on).
- `Box<dyn FnOnce(Option<PickedFile>)>` — no `Send` bound. The box passes through `Arc<dyn FilePicker>` (Send+Sync on the *picker*, not the callback) — the callback itself never crosses threads.
- `on_attach` itself stays `FnMut() + 'static` (consumed by `GestureDetector::on_tap`). The `pick_file` call returns immediately; the actual send happens later in the callback (iOS) or re-entrantly within the same call (desktop).
- **Desktop path:** `RfdFilePicker::pick_file` calls `on_done` synchronously inside the `pick_file` call. `on_send` fires while still inside `GestureDetector`'s tap handler — exactly the same call stack as before the trait change. No behavior change.

### Test mock updates (`shared_app/src/test_util.rs`)

Both mocks adopt the callback signature:

- **`NoopFilePicker`** (via `test_file_picker()`): `on_done(None)` synchronously.
- **`MockFilePicker`**: `on_done(self.picked.clone())` synchronously.

The two existing attach tests (`test_attach_button_sends_file_message`, `test_attach_button_picker_none_does_not_send`) assert post-tap state. Since both mocks fire synchronously, the assertions (send count, message appended, `MessageKind::File` kind) hold without test-harness changes — only the mock impls change.

## Files touched summary

| File | Change |
|---|---|
| `vexo/src/platform/file_picker.rs` | Trait signature: `pick_file` takes `Box<dyn FnOnce(Option<PickedFile>)>`. `NoopFilePicker`, `RfdFilePicker` impls updated. Extract cfg-free `mime_from_extension_str` shared by desktop + iOS. |
| `vexo/src/platform/file_picker_ios.rs` | **NEW** — `IosFilePicker` + `define_class!` delegate subclass with `Rc<RefCell<Option<...>>>` callback in ivars + `thread_local LIVE_DELEGATE` for delegate retention. |
| `vexo/src/platform/mod.rs` | Register `file_picker_ios` module under `#[cfg(target_os = "ios")]`; iOS branch of `default_file_picker()` returns `IosFilePicker`. |
| `vexo/Cargo.toml` | Extend objc2-ui-kit / objc2-foundation feature lists for `UIDocumentPickerViewController` etc. |
| `shared_app/src/chats/chat_screen.rs` | `on_attach` closure moves send logic into `on_done` callback (lines ~251-265). |
| `shared_app/src/test_util.rs` | `MockFilePicker`, `NoopFilePicker`-based `test_file_picker()` adopt callback signature. |

**Files deliberately NOT modified:**

- `shared_app/src/chats/mod.rs:111`, `desktop.rs`, `app.rs`, `data.rs` — all pass `Arc<dyn FilePicker>` opaquely; no signature leak. Only the trait *method* changed, not the field types.
- `VexoDemo/VexoDemo/main.swift` — no Swift changes. Pure Rust, matches `ios_clipboard` precedent.
- `build_for_ios.sh` — no build-script changes.

## Verification plan

1. `cargo build -p vexo` — trait + iOS module compile (host build; iOS module cfg'd out).
2. `cargo build -p shared_app` — chat_screen wiring + host bindgen binary build.
3. `cargo test -p shared_app` — all chat_screen attach tests pass with updated mocks.
4. `cargo test -p vexo` — file_picker unit tests (size gating, `NoopFilePicker` callback) pass.
5. `cargo build -p shared_app --target aarch64-apple-ios-sim` (or via `./build_for_ios.sh`) — iOS target compiles.
6. User runs `./build_for_ios.sh` and taps the attach button on iOS simulator/device to verify the picker actually presents and a file message appears. (Headless tests cannot exercise the UIKit picker — same constraint as `ios_clipboard`, which has no unit test.)

## Risk check

- **Re-entrancy on desktop:** `RfdFilePicker` calls `on_done` synchronously inside `pick_file`. The `on_done` closure calls `on_send`, which mutates the messages `Signal`, which triggers framework rebuilds. This happens inside `GestureDetector`'s tap handler — same stack as before the trait change. No new re-entrancy surface.
- **Callback lifetime on iOS:** The callback lives in the delegate's ivars (`Rc<RefCell<Option<...>>>`). The delegate is kept alive by `LIVE_DELEGATE` (module-scope `thread_local`) until `fire` clears it. If the user never picks/cancels (e.g. backgrounds the app), the delegate + callback leak until the next `pick_file` call overwrites `LIVE_DELEGATE`. Acceptable for a demo; single-entry by construction (only one picker presented at a time).
- **Security-scoped resources:** iOS-picked URLs require `startAccessingSecurityScopedResource()` before reading. Paired with `stopAccessingSecurityScopedResource()` in a RAII guard struct to guarantee release even on read error.
- **Presenting VC lookup:** `keyWindow.rootViewController` is deprecated on iOS 13+ in favor of connected scenes. For a single-window demo app this works; a multi-scene app would need scene-iteration. Documented as a known limitation; not a blocker for the demo.
