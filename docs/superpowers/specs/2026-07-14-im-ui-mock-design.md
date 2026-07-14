# Mocked IM UI Design

**Date:** 2026-07-14
**Status:** Approved (pending user spec review)
**Scope:** `vexo/`, `vexo_uikit/`, `shared_app/` (3 crates)

## Motivation

Vexo's three-tree architecture, reconciliation, focus tree, navigation stack,
animation, theming, and text editing foundation are solid. To validate the
framework against a real-world UI pattern, we want to build an IM (chat) app UI
on top of it.

**Scope is deliberately constrained:** a **mocked** IM UI — no real network,
no real database, no push notifications. Data is seeded in-memory at app start
and mutated locally. This removes the four blocking framework gaps (networking,
storage, async runtime, push) and leaves a small, well-bounded set of
framework work.

**Goal:** a three-tab IM app shell (Chats / Contacts / Me) with a conversation
list, a chat view with plain-text messages and avatars, all rendered from
mocked data, exercising two genuine framework gaps (`TabBar`, `ScrollController`
+ touch-drag) and composing everything else from existing primitives.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| App type | Mocked IM UI (no network, no DB) | User-selected. Removes 4 blocking framework gaps; lets us focus on UI composition + 2 framework primitives. |
| App shell | Full: Chats / Contacts / Me tabs | User-selected. Exercises the most framework gaps (TabBar + NavigationStackView + state preservation across tabs). |
| Message types | Plain text only | User-selected. Tightest scope; exposes rich-text gap for future work without blocking the MVP. |
| Chat interactions | View-only: scroll + send (appends, no mock reply) | User-selected. Minimum viable; long-press / scroll-to-bottom button / pull-to-load all deferred. |
| Avatars | Yes, embedded mock placeholders | User-selected. Standard IM chrome; exercises `Image::from_bytes` path. |
| Approach | Add `TabBar`+`TabController`+`TabBarView` and `ScrollController`+touch-drag as framework primitives; build UI in `shared_app` | Fills exactly the two gaps this scope exposes as proper reusable framework features (mirroring the `NavigationStackView` pattern in `vexo_uikit`). Rejected: build tab bar in demo app (throwaway, not reusable); add virtualized `ListView` (YAGNI for 20-50 mocked messages). |
| Out of scope | Virtualized ListView, long-press gestures, rich text, IME composition, shadows, dialogs/modals, persistence, networking, push, accessibility | Real gaps, but not needed for this MVP. Documented in the gap analysis; deferred to future specs. |

## Architecture

```
ImApp (Application)
└── TabBarView<ImTab> { Chats, Contacts, Me }
    ├── ChatsTab
    │   └── NavigationStackView<ChatsRoute>
    │       ├── ConversationListScreen  (root)
    │       └── ChatScreen              (pushed)
    ├── ContactsTab
    │   └── ContactListScreen
    └── MeTab
        └── ProfileScreen
```

- `ImTab` enum: `Chats | Contacts | Me`
- `ChatsRoute` enum: `List | Chat(conversation_id)`
- Mock data lives in `ImApp::State` (conversations, contacts, messages, profile)
  — mutated in-place; no persistence

### Application Data Flow

```
Application::new()
  → ImState seeded (conversations, messages, contacts, profile)
  → TabController<ImTab>::new(ImTab::Chats)
  → NavigationController<ChatsRoute>::new(ChatsRoute::List)

view(state, font_system)
  → TabBarView<ImTab>
      controller: state.tab_controller
      tabs: [Chats, Contacts, Me]
      page_builder:
        Chats    → NavigationStackView<ChatsRoute>
                     controller: state.chats_nav
                     root: ConversationListScreen
                     destination: ChatScreen(conv_id)
        Contacts → ContactListScreen
        Me       → ProfileScreen
```

## Framework Primitives

### 1. `TabBar` + `TabController` + `TabBarView` (in `vexo_uikit/`)

Mirrors the existing `NavigationStackView` pattern at
`vexo_uikit/src/navigation.rs:97,268`.

```
TabController<D: PartialEq + Clone>
  - current() -> D
  - switch_to(D)              // triggers rebuild via dirty callback
  - on_change(callback)

TabBarView<D>
  - controller: TabController<D>
  - tabs: Vec<D>
  - page_builder: Fn(D) -> Widget
  - tab_bar_builder: Fn(&D, bool) -> Widget   // dest, is_selected
```

Layout: `Column { TabBarView content (flex:1), TabBar (fixed height) }`.
Internally uses `IndexedStack` (`widgets/indexed_stack.rs:64`) so each tab page
preserves state. Dirty-callback wired on mount, same as `NavigationController`.

### 2. `ScrollController` + touch-drag (in `vexo/`)

Extends `ScrollView` (`widgets/scroll_view.rs:14`,
`elements/scroll_view.rs:179`):

```
ScrollController
  - jump_to_bottom()
  - jump_to(offset: f32)
  - current_offset() -> f32
  - on_scroll(callback)

ScrollView::new(child)
  .controller(Option<ScrollController>)   // new optional field
```

Touch-drag handling added to `ScrollViewElement::on_event`:

- `PointerButton(Pressed)` → record `drag_start_y`, `drag_active = true`
- `PointerMoved` → if `drag_active`, `offset -= (y - last_y); last_y = y`
- `PointerButton(Released)` → `drag_active = false`

Still clamps to `[0, max_scroll]` via existing `clamp_offset()`
(`scroll_view.rs:48`).

**Before → after:**
- Already works: mouse wheel, keyboard arrows/PageUp/Down/Home/End
- Added: touch-drag scroll, `jump_to_bottom()` / `jump_to(offset)`,
  `current_offset()` readback

### 3. No other framework changes

Everything in the IM UI composes from existing primitives: `Flex`, `Stack` +
`Positioned`, `DecoratedContainer`, `Image`, `Text`, `TextEdit`,
`GestureDetector`, `NavigationStackView`, `Theme`, `SafeArea`.

## UI Screens

### 3a. Conversation List Screen (Chats tab root)

```
┌─────────────────────────────────┐
│  Chats                  (title) │  ← NavBar (in NavigationStackView)
├─────────────────────────────────┤
│ ┌───┐  Alice              14:32 │
│ │ ◯ │  See you tomorrow!        │  ← ConversationRow (GestureDetector)
│ └───┘                     [2]   │     avatar | name+preview | time+badge
│                                 │
│ ┌───┐  Bob                13:10 │
│ │ ◯ │  Got it, thanks            │
│ └───┘                           │
│                                 │
│ ┌───┐  Group Chat        12:45  │
│ │ ◳ │  Charlie: sounds good     │
│ └───┘                           │
├─────────────────────────────────┤
│  💬 Chats    👤 Contacts   ⚙️ Me │  ← TabBar
└─────────────────────────────────┘
```

- Root of `NavigationStackView<ChatsRoute>`; tapping a row pushes
  `ChatsRoute::Chat(id)`
- Each row is a `Component`: `Row { avatar(40px), Column { name, preview },
  Column { time, badge } }`
- Avatar: `Image::from_bytes` with embedded placeholder bytes,
  `corner_radius: 20`
- Unread badge: `DecoratedContainer` with red bg + corner_radius, `Text`
  inside; hidden when count is 0

### 3b. Chat Screen (pushed)

```
┌─────────────────────────────────┐
│ ‹  Alice                        │  ← NavBar with back button (existing)
├─────────────────────────────────┤
│              (scrollable msg list)│
│                                 │
│ ┌───┐ ┌──────────────┐          │
│ │ ◯ │ │ Hi!          │          │  ← incoming bubble (left, avatar)
│ └───┘ └──────────────┘          │
│                                 │
│          ┌──────────────┐ ┌───┐ │
│          │ Hey there!   │ │ ◯ │ │  ← outgoing bubble (right, avatar)
│          └──────────────┘ └───┘ │
│                                 │
│ ┌───┐ ┌──────────────┐          │
│ │ ◯ │ │ How are you? │          │
│ └───┘ └──────────────┘          │
│                                 │
├─────────────────────────────────┤
│ [  Type a message...      ] [↑] │  ← InputBar (fixed at bottom)
└─────────────────────────────────┘
```

- Layout: `Column { NavBar, ScrollView(msg list, flex:1), InputBar(fixed) }`
- `ScrollController` wired to the `ScrollView`; on send, `jump_to_bottom()`
- `InputBar`: `Row { TextEdit (flex:1), Send Button }` —
  `TextEditingController` holds draft; on send, append message to mock data,
  clear controller, `jump_to_bottom()`. Enter inserts a newline (existing
  TextEdit behavior, `text_edit.rs:170`); sending is via the Send button only.
  This keeps the InputBar implementation within scope — no key-event
  interception needed.
- Date divider rows omitted (scope is plain text only)

### 3c. Message Bubbles

```
incoming:                          outgoing:
┌───┐ ┌──────────────┐            ┌──────────────┐ ┌───┐
│ ◯ │ │ Hi!          │            │ Hey there!   │ │ ◯ │
└───┘ └──────────────┘            └──────────────┘ └───┘
avatar bubble                      bubble           avatar
(left)  (white bg,                   (tinted bg,      (right)
         gray border)                 accent color)
```

- Bubble: `DecoratedContainer { background, corner_radius: 12, border }`
  wrapping `Text`
- Incoming: white bg, left-aligned, avatar on left
- Outgoing: accent-color bg (from `Theme`), right-aligned, avatar on right
- Max width: `Layout { max_width: 70% }` on the bubble so long text wraps
  before filling the row
- Row: `Flex::row()` with `justify_content: FlexStart` for incoming (avatar +
  bubble at left), `justify_content: FlexEnd` for outgoing (bubble + avatar at
  right). Vexo has no `Spacer` widget; alignment is via `justify_content`.
- Avatars: same `Image::from_bytes` placeholder approach as conversation rows

### 3d. Contacts Tab & Me Tab

```
Contacts:                         Me:
┌─────────────────────────────────┐  ┌─────────────────────────────────┐
│  Contacts                       │  │  ┌─────┐                        │
├─────────────────────────────────┤  │  │  ◯  │  Alice                  │
│ ┌───┐  Alice                    │  │  └─────┘                        │
│ │ ◯ │  Online                   │  │  alice@example.com              │
│ └───┘                           │  ├─────────────────────────────────┤
│ ┌───┐  Bob                      │  │  Settings              ›        │
│ │ ◯ │  Last seen 10:00          │  │  Notifications         ›        │
│ └───┘                           │  │  About                 ›        │
├─────────────────────────────────┤  ├─────────────────────────────────┤
│  💬 Chats  👤 Contacts   ⚙️ Me  │  │  💬 Chats  👤 Contacts   ⚙️ Me  │
└─────────────────────────────────┘  └─────────────────────────────────┘
```

- **Contacts**: `ScrollView` of `ContactRow`s (`Row { avatar, Column { name,
  status } }`). Tap does nothing (scope: view-only).
- **Me**: `Column { avatar+name header, list of settings rows (view-only) }`.
  Static.

### 3e. Mock Data

Lives in `ImApp::State`:

```rust
struct ImState {
    conversations: Vec<Conversation>,       // id, name, avatar_bytes, unread_count
    messages: HashMap<ConvId, Vec<Message>>,// id, conv_id, author, text, timestamp
    contacts: Vec<Contact>,                 // id, name, avatar_bytes, status
    profile: Profile,                       // name, email, avatar_bytes
    tab_controller: TabController<ImTab>,
    chats_nav: NavigationController<ChatsRoute>,
}
```

- Seeded at `new()` with ~5 conversations, ~20 messages each, ~8 contacts
- Sending a message: `state.messages[conv_id].push(...)`; rebuild triggered by
  `setState` / dirty callback
- No mock reply (scope: view + send only, from earlier decision)

## Data Flow & Error Handling

### State Mutations (all synchronous, no async)

1. **Tap conversation row** → `chats_nav.push(ChatsRoute::Chat(id))` →
   NavigationController fires dirty callback → rebuild → `ChatScreen` mounts
2. **Type in InputBar** → `TextEditingController` updates `glyphon::Editor` →
   fires dirty callback → `TextEditContent` repaints (existing path, no
   app-state change)
3. **Send message** →
   - `controller.text()` read out
   - `state.messages[conv_id].push(Message::outgoing(text, now))`
   - `controller.set_text("")` (clears input)
   - `state.scroll_controller.jump_to_bottom()` (if controller exists)
   - `setState` triggers full rebuild → new message bubble appears in
     `ScrollView`
4. **Tap back** → `chats_nav.pop()` → NavigationController fires dirty callback
   → `ChatScreen` unmounts, `ConversationListScreen` shown
5. **Tap tab** → `tab_controller.switch_to(ImTab::Contacts)` → TabController
   fires dirty callback → `TabBarView` rebuilds with new active index in
   `IndexedStack`

### Controller Wiring (dirty-callback pattern)

Every external controller follows the same pattern as `TextEditingController`
(`text_edit.rs:308-339`) and `NavigationController` (`navigation.rs:97`):

```rust
// On mount:
controller.set_dirty_callback(Arc::new(move || {
    build_owner.mark_needs_build(element_id);
}));

// On unmount:
controller.set_dirty_callback(None);
```

Controllers in this design:
- `TabController<ImTab>` — fires on `switch_to()`
- `NavigationController<ChatsRoute>` — fires on `push/pop/replace` (already
  exists)
- `ScrollController` — fires `on_scroll` callback when offset changes
  (programmatic or drag)
- `TextEditingController` — fires on text change (already exists)

### Error Handling

Scope is mocked-data-only, so the error surface is minimal:

| Scenario | Handling |
|---|---|
| Send empty message | InputBar's send button disabled when `controller.text().is_empty()`; tapping does nothing |
| ScrollController not yet mounted (send during initial mount) | `jump_to_bottom()` is a no-op if controller has no element wired; on next rebuild the new `ScrollView` reads `max_scroll` and `ScrollController` will apply pending offset |
| Conversation not found in `messages` map | Render empty `ScrollView` (no bubbles). Should not happen with seeded data, but defensive. |
| `Image::from_bytes` fails on embedded placeholder | Returns empty `ImageData`; `ImageRenderObject` paints nothing. Embedded bytes are known-good, so this is defensive only. |
| Navigation stack empty (pop from root) | `NavigationController::pop()` is a no-op when stack has only root (existing behavior, `navigation.rs`) |

No panics, no unwraps on user data. All `HashMap` lookups use `.get()` /
`.entry()` patterns.

### Rebuild Granularity

- **Local widget state** (hover, button press, text cursor): handled by
  `Signal<T>` inside `Component`s — no app-state change, no full rebuild
- **App data change** (send message, switch tab, navigate): `setState` on
  `ImApp` → full tree rebuild. The three-tree reconciler (`reconciler.rs`)
  keeps DOM-diff cheap; existing primitives reuse via `can_update()`.
- **Scroll offset change**: `ScrollController` updates render object directly
  via `svro.set_scroll_offset()` (`scroll_view.rs:74`), no rebuild — just
  `request_frame()` for repaint

This is the same granularity model the framework already uses. No new patterns
introduced.

## Testing

Using the existing inline `#[cfg(test)] mod tests` pattern
(`text_edit.rs:619`, `navigation.rs` tests). Drive `ThreeTreePipeline`
headlessly via `MockBackend` (`render/mock_backend.rs:10`).

### Framework Tests (in `vexo/` and `vexo_uikit/`)

**`ScrollController` + touch-drag:**
- `test_scroll_controller_jump_to_bottom` — mount `ScrollView` with tall
  content, call `jump_to_bottom()`, assert `current_offset() == max_scroll`
- `test_scroll_controller_jump_to_offset_clamps` — `jump_to(-100)` clamps to
  0; `jump_to(99999)` clamps to `max_scroll`
- `test_touch_drag_scrolls` — synthesize `PointerButton(Pressed)` →
  `PointerMoved` → `PointerButton(Released)` events, assert offset changed by
  expected delta
- `test_touch_drag_clamps_at_edges` — drag past top/bottom, assert offset
  stays in `[0, max_scroll]`
- `test_scroll_controller_without_element_is_noop` — call `jump_to_bottom()`
  before mount, assert no panic; mount later, assert offset applied
- `test_mouse_wheel_still_works` — regression: existing `Scroll` event path
  unchanged

**`TabController` + `TabBarView`:**
- `test_tab_switch_rebuilds` — `TabController::switch_to(Contacts)` fires
  dirty callback, `IndexedStack` shows contacts page
- `test_tab_pages_preserve_state` — type in a `TextEdit` on page A, switch to
  B, switch back, assert text preserved (verifies `IndexedStack` + `Offstage`
  keep state)
- `test_tab_controller_dirty_callback_wired_on_mount` — controller set before
  mount is no-op; after mount, switch fires rebuild
- `test_tab_controller_cleared_on_unmount` — switch after unmount is a no-op,
  no panic

### App UI Tests (in `shared_app/`)

**Conversation list:**
- `test_conversation_list_renders_rows` — seed 5 conversations, pump, assert
  5 row elements mounted
- `test_tap_row_pushes_chat_route` — synthesize `PointerButton` on row 0,
  assert `chats_nav.current() == ChatsRoute::Chat(id_0)`
- `test_unread_badge_hidden_when_zero` — conversation with `unread_count: 0`,
  assert no badge element

**Chat screen:**
- `test_chat_screen_renders_messages` — seed 3 messages, pump, assert 3
  bubble elements
- `test_send_message_appends_and_clears` — set `TextEditingController` text
  to "hi", synthesize send button press, assert
  `state.messages[conv_id].len()` increased by 1 and controller text is empty
- `test_send_scrolls_to_bottom` — with 20 messages, send a new one, assert
  `ScrollController::current_offset() == max_scroll`
- `test_send_disabled_when_input_empty` — empty controller, assert send
  button `disabled: true`
- `test_incoming_vs_outgoing_bubble_alignment` — assert incoming row avatar
  is left, outgoing row avatar is right (via element/widget tree inspection)

**Tabs:**
- `test_tab_switch_chats_to_contacts` —
  `tab_controller.switch_to(Contacts)`, pump, assert contacts page visible in
  `IndexedStack`
- `test_tab_switch_preserves_nav_stack` — push a `ChatScreen` on Chats tab,
  switch to Contacts and back, assert `chats_nav` still on `Chat(id)`

### What's NOT Tested (out of scope)

- Golden/pixel tests — `MockBackend` records command strings, not pixels; no
  golden infra exists
- Accessibility — no a11y tree exists to test against
- Performance — no benchmark harness; 20 mocked messages won't stress the
  non-virtualized `ScrollView`
- Network/storage — none in this scope
- iOS-specific touch — tested via synthesized `InputEvent`s; real iOS touch
  routing is a manual check

### Manual Verification

Since the GUI can't be run in-session, the user runs:
- `cargo run -p desktop_demo` — verify all three tabs, navigation, sending
  messages, scroll-to-bottom, mouse-wheel scroll
- iOS build via `build_for_ios.sh` + Xcode — verify touch-drag scroll, soft
  keyboard, safe area insets

## Out-of-Scope Gaps (documented for future specs)

These framework gaps were identified during the gap analysis but are
deliberately deferred. Each is a candidate for its own spec → plan →
implementation cycle:

- **Virtualized `ListView`** — needed for real conversations with 10k+
  messages
- **Long-press / context-menu gesture** — needed for reply/copy/delete on
  messages
- **Rich text / styled spans** — needed for @mentions, links, bold
- **IME composition (CJK preedit/marked text)** — needed for
  Chinese/Japanese/Korean input on iOS
- **Image loading from network + cache** — needed for real avatars and image
  messages
- **Push notifications** — needed for new-message delivery
- **Emoji font** — needed for emoji in messages
- **Text alignment** (center/right) — needed for chat bubbles with centered
  text
- **Dialogs / modals / snackbars** — needed for confirm dialogs and toast
  feedback
- **Shadows** — needed for chat bubble depth
- **Tab traversal policy** — focus tree has the `skip_traversal` flag but no
  policy implements it
- **Accessibility** — entirely absent; App Store blocker
- **State preservation across restart** — no serde, no persistence layer
- **Networking (WebSocket + HTTP)** — needed for real IM
- **Persistent storage (SQLite)** — needed for chat history
- **Async runtime integration** — needed for network/DB results to drive
  rebuilds
