# shared_app Module Extraction Design

**Date:** 2026-07-15
**Status:** Approved (pending user spec review)
**Scope:** `shared_app/` (single crate; no framework changes)

## Motivation

`shared_app/src/lib.rs` is a single 1061-line file holding the entire mocked IM
app: domain types, app state, seed data, four screens, the `Application` impl,
the UniFFI `MobileApp` export, and thirteen tests. As features land, this file
keeps growing and every change forces navigating unrelated code.

This refactor splits `lib.rs` into feature modules so each screen and the data
layer can evolve independently. The goal is a clean structure **with no behavior
change** — the same widgets render the same way from the same data — plus a few
small dedup wins folded in while moving code.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Refactor depth | Extraction + light cleanup | User-selected. Gets a clean feature structure without the risk of restructuring `ImState` or introducing a data-boundary abstraction. Avatar dedup is a clear win that's trivial while moving code. |
| Module layout | Feature folders + shared `widgets/` | User-selected. Each screen gets room to grow without re-splitting later; `widgets/` captures cross-feature reusable bits. The avatar builder is duplicated across all four screens. |
| Test placement | Per-feature inline + separate `integration_tests.rs` | User-selected. Unit tests co-locate with their feature; the cross-cutting full-app pipeline tests live at the integration level. |
| Public API | Unchanged: `shared_app::ImState`, `shared_app::MobileApp` | Required by `desktop_demo` and the iOS UniFFI surface. All feature modules are `pub(crate)`. |
| Behavior | Identical (no logic changes) | Pure structural refactor + local dedup. Tests pass unmodified. |
| Commit cadence | One clean commit at the end | Default; revisitable during planning. |

## Out of Scope

- Splitting `ImState` fields or introducing a repository/data-boundary.
- Changes to `ChatScreen` Component lifecycle logic (relocated only).
- Any edit to `vexo/`, `vexo_uikit/`, or `vexo_fontawesome/`.
- New behavior, new screens, new features.

## Target Module Structure

```
shared_app/src/
├── lib.rs                    # Minimal: uniffi scaffolding, mod decls, pub re-exports
├── data.rs                   # Domain types, ImState, seed(), make_avatar_png()
├── widgets/
│   ├── mod.rs                # pub(crate) mod avatar;
│   └── avatar.rs             # avatar() — deduped circular avatar builder
├── chats/
│   ├── mod.rs                # Chats tab: nav stack wiring, build_chats_tab()
│   ├── conversation_list.rs  # build_conversation_list_screen, build_conversation_row, format_timestamp
│   └── chat_screen.rs        # ChatScreen Component, ChatScreenState, build_message_bubble, build_input_bar
├── contacts/
│   ├── mod.rs                # Contacts tab: build_contacts_tab()
│   └── contacts_screen.rs    # build_contacts_screen, build_contact_row
├── me/
│   ├── mod.rs                # Me tab: build_me_tab()
│   └── profile_screen.rs     # build_profile_screen
├── app.rs                    # impl Application for ImState, impl Default, MobileApp
└── integration_tests.rs      # Full-app pipeline tests (cross-cutting)
```

### Responsibility split

- **`data.rs`** owns the data layer only: `ConvId`, `ImTab`, `ChatsRoute`,
  `Message`, `MessageAuthor`, `Conversation`, `Contact`, `Profile`, `ImState`,
  `seed()`, `make_avatar_png()`. No UI. `ImTab`/`ChatsRoute` live here because
  they are data enums consumed by both `app.rs` and the tab modules.
- **Each `*/mod.rs`** owns its tab's `NavigationStackView` wiring, extracted out
  of the monolithic `view()` closure. Exposes a single `build_*_tab(...)`
  entry point. (`ChatsRoute` itself is defined in `data.rs` and used here.)
- **`app.rs`** holds the `Application` impl, `Default`, and `MobileApp`. Its
  `view()` shrinks to: clone shared data → build the three tab closures → hand
  them to `TabBarView`. The `TabBarView` item builder (icons + labels) stays in
  `app.rs` since it is a tab-shell concern, not a feature concern.
- **`lib.rs`** becomes ~15 lines: uniffi scaffolding, `mod` declarations, and
  `pub use app::{ImState, MobileApp};` so `desktop_demo` and iOS keep working
  unchanged.

### Public API

`shared_app::ImState` and `shared_app::MobileApp` remain the only public
surface. All feature modules are crate-private (`pub(crate)`). `desktop_demo`
(`use shared_app::ImState;`) and the iOS `MobileApp::start_app()` path are
unaffected.

## Dedup & Cleanup Wins

These are folded into the extraction while moving code. Each is local and low-risk.

### 1. Deduped `Avatar` builder

**Current:** Avatar construction is copy-pasted across all four screens with
only the diameter varying (40 / 32 / 80). Four near-identical blocks of
`Image::from_bytes(...).expect(...).width(d).height(d).corner_radius(d/2).clip()`.

**After:** one function in `widgets/avatar.rs`:

```rust
pub(crate) fn avatar(bytes: &Rc<[u8]>, diameter: f32) -> Box<dyn Widget> {
    Image::from_bytes(bytes)
        .expect("avatar bytes are valid PNG")
        .width(diameter)
        .height(diameter)
        .corner_radius(diameter / 2.0)
        .clip()
}
```

All four screens call `avatar(&conv.avatar_bytes, 40.0)` etc. Behavior
identical; ~30 lines removed.

### 2. Per-feature test co-location

**Current:** one 280-line `mod tests` block in `lib.rs` mixing data tests,
per-screen tests, and full-app pipeline tests (13 tests total).

**After:**

| Test | New home |
|---|---|
| `test_seed_has_five_conversations` | `data.rs` |
| `test_seed_messages_for_alice` | `data.rs` |
| `test_seed_contacts_count` | `data.rs` |
| `test_avatar_bytes_decode` | `data.rs` |
| `test_tab_controller_starts_on_chats` | `data.rs` |
| `test_conversation_list_renders_in_pipeline` | `chats/conversation_list.rs` |
| `test_chat_screen_renders_messages` | `chats/chat_screen.rs` |
| `test_chat_screen_input_bar_pinned_to_bottom_with_few_messages` | `chats/chat_screen.rs` |
| `test_contacts_screen_renders_in_pipeline` | `contacts/contacts_screen.rs` |
| `test_profile_screen_renders_in_pipeline` | `me/profile_screen.rs` |
| `test_full_app_view_renders_three_tabs` | `integration_tests.rs` |
| `test_tab_switch_to_contacts_renders_contacts_page` | `integration_tests.rs` |
| `test_contacts_tab_tab_bar_fits_window` | `integration_tests.rs` |

No test logic changes — only relocation. Each module owns its own
`#[cfg(test)] mod tests`. The integration tests stay `#[cfg(test)]` and are
declared via `#[cfg(test)] mod integration_tests;` in `lib.rs`.

### 3. Remove dead `messages_for_view`

**Current:** `lib.rs:647` clones `state.messages` into `messages_for_view`, then
`lib.rs:745` does `let _ = messages_for_view;` — dead code left from an earlier
iteration.

**After:** deleted. One fewer clone in `view()`.

### 4. Tab-wiring extraction from `view()`

**Current:** the `TabBarView::new(...)` closure in `view()` is ~90 lines; the
Chats branch alone is ~45 lines. Hard to read, hard to test in isolation.

**After:** each tab's closure body moves to its `mod.rs` as a focused function:

```rust
// chats/mod.rs
pub(crate) fn build_chats_tab(
    conversations: Vec<Conversation>,
    nav: NavigationController<ChatsRoute>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    me_avatar: Rc<[u8]>,
) -> Box<dyn Widget> { ... }

// contacts/mod.rs
pub(crate) fn build_contacts_tab(
    contacts: Vec<Contact>,
    nav: NavigationController<()>,
) -> Box<dyn Widget> { ... }

// me/mod.rs
pub(crate) fn build_me_tab(
    profile: &Profile,
    nav: NavigationController<()>,
) -> Box<dyn Widget> { ... }
```

`app.rs`'s `view()` clones the needed data and calls these three. The
`TabBarView` item builder (icons + labels) stays in `app.rs`.

## Migration Approach

Code moves one feature at a time, keeping the crate compiling after each step.
This avoids a big-bang refactor where everything breaks at once.

```
Step 1: Scaffold empty modules + lib.rs mod declarations
        (compiles; all code still in lib.rs)
Step 2: Extract data.rs (types + ImState + seed + make_avatar_png)
        (lib.rs uses crate::data::*)
Step 3: Extract widgets/avatar.rs; replace 4 inline avatar blocks with avatar()
Step 4: Extract chats/ (mod.rs + conversation_list.rs + chat_screen.rs)
Step 5: Extract contacts/ (mod.rs + contacts_screen.rs)
Step 6: Extract me/ (mod.rs + profile_screen.rs)
Step 7: Extract app.rs (Application impl, Default, MobileApp); slim lib.rs
Step 8: Move tests to their per-feature homes + integration_tests.rs
Step 9: Remove dead messages_for_view
```

After each step: `cargo build -p shared_app` must pass. After Step 8:
`cargo test -p shared_app` must pass.

## Verification

Per `CLAUDE.md`: run `cargo build` after edits and `cargo test` after
implementing.

- `cargo build -p shared_app` — crate compiles.
- `cargo build -p desktop_demo` — consumer still compiles (proves public API
  unchanged).
- `cargo test -p shared_app` — all 13 tests pass (no logic changes, so counts
  and assertions are identical).

`cargo run -p desktop_demo` is **not** run by the assistant (per `CLAUDE.md`:
never run the GUI demo). If visual confirmation is wanted, the command is
handed to the user.

## Risk & Rollback

- **Risk:** low. Pure structural refactor + local dedup. No behavior change, no
  public API change, no framework edits.
- **Rollback:** each step is a discrete unit of work. With the chosen
  single-commit cadence, the whole refactor reverts as one commit; if a
  per-step history is later preferred, that can be arranged during planning.
