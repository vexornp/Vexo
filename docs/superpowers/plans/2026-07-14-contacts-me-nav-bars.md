# Nav Bars for Contacts & Me Tabs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the Contacts and Me tabs nav bars (matching the Chats tab) by wrapping their content in `NavigationStackView` with never-pushed controllers, and remove the outer `SafeArea` that was double-padding the top inset.

**Architecture:** Reuse `NavigationStackView` for Contacts and Me with `NavigationController<()>` ("no destinations"). `NavigationStackView` already builds a nav bar that consumes the top safe-area inset (background extends under the status bar) and wraps its content in `SafeArea(top=false)` for the remaining insets. Move the bottom safe-area (home indicator) into `TabBarView` itself, mirroring the `NavigationStackView` pattern. Drop the outer `SafeArea` from `shared_app/src/lib.rs`.

**Tech Stack:** Rust, vexo framework, vexo_uikit components, `NavigationController<()>` / `NavigationStackView` / `SafeArea` / `TabBarView`.

## Global Constraints

- Controller identity must be stable across renders: store controllers in `ImState` and `.clone()` them into closures. `NavigationStackViewState::on_update` re-wires the dirty callback only when `Rc::ptr_eq(&old.controller.path, &new.controller.path)` is false (`vexo_uikit/src/navigation.rs:449`); a fresh controller per render would never re-wire.
- `NavigationStackView::new` defaults its `destination` closure to `|_| Text::new("").boxed()` and `title` closure to `|_| String::new()` (`vexo_uikit/src/navigation.rs:307-308`). Both are only called when a destination is pushed; with a never-pushed controller, only `root_title` and the root widget are used. So `()` is the natural destination type.
- The Chats tab's `ChatScreen` input bar gets its bottom safe-area from `NavigationStackView`'s inner `SafeArea(top=false)` wrapper (`vexo_uikit/src/navigation.rs:703`), so it is unaffected by removing the outer `SafeArea`.
- Tab bar background color is **out of scope** — adding an opaque background to `TabBarView` is a separate enhancement.

---

## File Structure

- Modify: `shared_app/src/lib.rs` — add `contacts_nav` / `me_nav` controllers to `ImState`, wrap Contacts and Me screens in `NavigationStackView`, remove outer `SafeArea`.
- Modify: `vexo_uikit/src/tab_bar.rs` — wrap the tab bar row in `SafeArea(top=false)` so the home-indicator inset is handled internally after the outer `SafeArea` is removed.

No new files.

---

## Task 1: Wrap the tab bar row in `SafeArea` inside `TabBarView`

This task is sequenced first so that removing the outer `SafeArea` in Task 2
never leaves the tab bar without bottom-inset handling — at no intermediate
commit does the home indicator get ignored.

**Files:**
- Modify: `vexo_uikit/src/tab_bar.rs:16-19` (imports)
- Modify: `vexo_uikit/src/tab_bar.rs:150-188` (`TabBarView::render`)
- Test: existing tests in `vexo_uikit/src/tab_bar.rs:191-306` (no new tests; this is a structural wrapper with zero safe-area on desktop, so the existing `test_tab_bar_view_renders_active_page` covers it)

**Interfaces:**
- Consumes: `vexo::SafeArea` (already used by `NavigationStackView` at `vexo_uikit/src/navigation.rs:47`)
- Produces: `TabBarView::render` now owns the bottom safe-area internally; downstream callers no longer need to wrap `TabBarView` in a bottom-insetting `SafeArea`.

- [ ] **Step 1: Read the current `TabBarView::render` to confirm the exact lines to edit**

Run: `Read vexo_uikit/src/tab_bar.rs offset=150 limit=40`

Expected: see the `Flex::column()...push(stack.flex_grow(1.0)).push(bar).boxed()` return at lines 178-188, and the `use vexo::{...}` block at lines 16-19.

- [ ] **Step 2: Add `SafeArea` to the `use vexo::{...}` import**

In `vexo_uikit/src/tab_bar.rs`, replace:

```rust
use vexo::{
    Component, ComponentState, Flex, IndexedStack, Layout, LifecycleContext, RenderContext, Text,
    Widget,
};
```

with:

```rust
use vexo::{
    Component, ComponentState, Flex, IndexedStack, Layout, LifecycleContext, RenderContext,
    SafeArea, Text, Widget,
};
```

- [ ] **Step 3: Wrap the tab bar row in `SafeArea(top=false)`**

In `vexo_uikit/src/tab_bar.rs`, replace the `TabBarView::render` return block:

```rust
        Flex::column()
            .layout(
                Layout::default()
                    .flex_direction(FlexDirection::Column)
                    .width_percent(1.0)
                    .height_percent(1.0),
            )
            .push(stack.flex_grow(1.0))
            .push(bar)
            .boxed()
    }
}
```

with:

```rust
        // The tab bar row owns its bottom safe-area (home indicator) and
        // left/right insets (landscape notch), mirroring how
        // `NavigationStackView` owns the top inset for its nav bar. `top(false)`
        // because the bar is at the bottom — no status-bar inset to consume.
        let bar = SafeArea::new(bar.boxed()).top(false).boxed();

        Flex::column()
            .layout(
                Layout::default()
                    .flex_direction(FlexDirection::Column)
                    .width_percent(1.0)
                    .height_percent(1.0),
            )
            .push(stack.flex_grow(1.0))
            .push(bar)
            .boxed()
    }
}
```

Note: the inner `bar` (a `Flex`) needs `.boxed()` before `SafeArea::new` because `SafeArea::new` takes `impl Widget + 'static`, not `Flex`. The outer `.boxed()` on the `SafeArea` is then pushed into the column.

- [ ] **Step 4: Build `vexo_uikit` to confirm it compiles**

Run: `cargo build -p vexo_uikit`
Expected: compiles with no errors.

- [ ] **Step 5: Run the existing tab-bar tests to confirm no regression**

Run: `cargo test -p vexo_uikit --lib tab_bar`
Expected: all tests in `tab_bar::tests` pass (5 tests: `test_tab_controller_*` (4) and `test_tab_bar_view_renders_active_page`).

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/tab_bar.rs
git commit -m "feat(tab_bar): wrap tab bar row in SafeArea(top=false) for home-indicator inset

Moves bottom safe-area handling into TabBarView itself, mirroring
NavigationStackView's pattern of owning its top inset. This is preparatory
work for removing the outer SafeArea in shared_app — at no intermediate
commit does the tab bar lose bottom-inset handling."
```

---

## Task 2: Add `contacts_nav` and `me_nav` controllers to `ImState`; wrap Contacts and Me in `NavigationStackView`; remove outer `SafeArea`

**Files:**
- Modify: `shared_app/src/lib.rs:10` (remove `SafeArea` from imports if unused after edit)
- Modify: `shared_app/src/lib.rs:79-87` (`ImState` struct — add two fields)
- Modify: `shared_app/src/lib.rs:258-265` (`seed()` — init the two new controllers)
- Modify: `shared_app/src/lib.rs:621-708` (`view()` — capture new controllers, wrap Contacts/Me, remove outer `SafeArea`)
- Test: existing tests in `shared_app/src/lib.rs:711-848` (the `test_full_app_view_renders_three_tabs` test at line 835 covers the full view; adding nav bars only increases element counts, so thresholds don't regress)

**Interfaces:**
- Consumes: `NavigationStackView` and `NavigationController` from `vexo_uikit` (already imported at `shared_app/src/lib.rs:14-17`).
- Produces: `ImState` now exposes `contacts_nav: NavigationController<()>` and `me_nav: NavigationController<()>`. The `view()` returns the `tab_view.boxed()` directly (no outer `SafeArea`).

- [ ] **Step 1: Add the two controller fields to `ImState`**

In `shared_app/src/lib.rs`, the `ImState` struct (lines 79-87) currently reads:

```rust
#[derive(ComponentState)]
pub struct ImState {
    conversations: Vec<Conversation>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    contacts: Vec<Contact>,
    profile: Profile,
    tab_controller: TabController<ImTab>,
    chats_nav: NavigationController<ChatsRoute>,
}
```

Replace with:

```rust
#[derive(ComponentState)]
pub struct ImState {
    conversations: Vec<Conversation>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    contacts: Vec<Contact>,
    profile: Profile,
    tab_controller: TabController<ImTab>,
    chats_nav: NavigationController<ChatsRoute>,
    contacts_nav: NavigationController<()>,
    me_nav: NavigationController<()>,
}
```

- [ ] **Step 2: Initialize the two new controllers in `seed()`**

In `shared_app/src/lib.rs`, the `seed()` return block (lines 258-265) currently reads:

```rust
    ImState {
        conversations,
        messages: Signal::new(messages),
        contacts,
        profile,
        tab_controller: TabController::new(ImTab::Chats),
        chats_nav: NavigationController::new(),
    }
}
```

Replace with:

```rust
    ImState {
        conversations,
        messages: Signal::new(messages),
        contacts,
        profile,
        tab_controller: TabController::new(ImTab::Chats),
        chats_nav: NavigationController::new(),
        contacts_nav: NavigationController::new(),
        me_nav: NavigationController::new(),
    }
}
```

Type inference resolves `NavigationController::<()>::new()` from the field's declared type — no turbofish needed.

- [ ] **Step 3: Capture the new controllers in `view()`**

In `shared_app/src/lib.rs`, the top of `view()` (around lines 622-630) currently captures clones for the closures:

```rust
    fn view(state: &mut Self::State) -> Box<dyn Widget> {
        let conversations = state.conversations.clone();
        let messages_for_view = state.messages.clone();
        let nav_for_list = state.chats_nav.clone();
        let nav_for_chat = state.chats_nav.clone();
        let convs_for_chat = state.conversations.clone();
        let messages_for_chat = state.messages.clone();
        let contacts = state.contacts.clone();
        let profile = state.profile.clone();
        let tab_controller = state.tab_controller.clone();
```

Replace with (add two new captures):

```rust
    fn view(state: &mut Self::State) -> Box<dyn Widget> {
        let conversations = state.conversations.clone();
        let messages_for_view = state.messages.clone();
        let nav_for_list = state.chats_nav.clone();
        let nav_for_chat = state.chats_nav.clone();
        let convs_for_chat = state.conversations.clone();
        let messages_for_chat = state.messages.clone();
        let contacts = state.contacts.clone();
        let profile = state.profile.clone();
        let tab_controller = state.tab_controller.clone();
        let contacts_nav = state.contacts_nav.clone();
        let me_nav = state.me_nav.clone();
```

- [ ] **Step 4: Wrap the Contacts tab content in `NavigationStackView`**

In `shared_app/src/lib.rs`, the `ImTab::Contacts` arm (line 683) currently reads:

```rust
                ImTab::Contacts => build_contacts_screen(contacts.clone()),
```

Replace with:

```rust
                ImTab::Contacts => NavigationStackView::new(
                    contacts_nav.clone(),
                    build_contacts_screen(contacts.clone()),
                )
                .root_title("Contacts")
                .boxed(),
```

`contacts_nav` is captured by the outer `move |tab|` closure (Step 3); `.clone()` shares the inner `Rc<RefCell<Vec<()>>>` so identity is preserved across rebuilds. `NavigationStackView::new` defaults `destination`/`title` closures — never invoked because `contacts_nav` is never pushed.

- [ ] **Step 5: Wrap the Me tab content in `NavigationStackView`**

In `shared_app/src/lib.rs`, the `ImTab::Me` arm (line 684) currently reads:

```rust
                ImTab::Me => build_profile_screen(&profile),
```

Replace with:

```rust
                ImTab::Me => NavigationStackView::new(
                    me_nav.clone(),
                    build_profile_screen(&profile),
                )
                .root_title("Me")
                .boxed(),
```

`me_nav` is captured by the outer `move |tab|` closure (Step 3). `build_profile_screen(&profile)` borrows `profile` for the call duration and returns an owned `Box<dyn Widget>` — safe to pass as the `NavigationStackView` root.

- [ ] **Step 6: Remove the outer `SafeArea` wrapper**

In `shared_app/src/lib.rs`, the end of `view()` (lines 706-708) currently reads:

```rust
        let _ = messages_for_view;
        SafeArea::new(tab_view.boxed()).boxed()
    }
}
```

Replace with:

```rust
        let _ = messages_for_view;
        tab_view.boxed()
    }
}
```

The outer `SafeArea` is no longer needed: each tab's `NavigationStackView` handles the top inset (nav bar) and left/right/bottom insets (its inner `SafeArea(top=false)` at `vexo_uikit/src/navigation.rs:703`); the tab bar handles its own bottom inset (Task 1).

- [ ] **Step 7: Remove `SafeArea` from the `use vexo::{...}` import if no longer used**

Run: `rg "SafeArea" shared_app/src/lib.rs`
Expected: zero matches (confirming `SafeArea` is no longer referenced in this file).

If zero matches, edit `shared_app/src/lib.rs` line 10 to remove `SafeArea` from the import list. The current line 10 reads:

```rust
    ImageData, IndexedStack, Layout, LifecycleContext, RenderContext, Row, SafeArea, ScrollView,
```

Replace with:

```rust
    ImageData, IndexedStack, Layout, LifecycleContext, RenderContext, Row, ScrollView,
```

If matches remain (e.g. a test uses it), leave the import.

- [ ] **Step 8: Build `shared_app` to confirm it compiles**

Run: `cargo build -p shared_app`
Expected: compiles with no errors. Watch in particular for: (a) the `NavigationController<()>` type inference at `seed()`, (b) borrow-checker acceptance of `build_profile_screen(&profile)` inside the `move |tab|` closure (the closure already captures `profile` by move from line 629's `let profile = state.profile.clone();`, so the borrow is local to the arm body — fine).

- [ ] **Step 9: Run the shared_app test suite**

Run: `cargo test -p shared_app`
Expected: all tests pass. In particular:
- `test_full_app_view_renders_three_tabs` (line 835) — asserts `element_registry().len() > 15`. Adding nav bars increases the count (each `NavigationStackView` adds a nav bar `Flex`, title `Text`, leading/trailing segments, plus the inner `SafeArea`), so the threshold still holds.
- `test_contacts_screen_renders_in_pipeline` (line 803) and `test_profile_screen_renders_in_pipeline` (line 819) — these call `build_contacts_screen` / `build_profile_screen` directly (not through `NavigationStackView`), so they're unaffected.
- `test_conversation_list_renders_in_pipeline` (line 749) — unaffected.

- [ ] **Step 10: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat(shared_app): add nav bars to Contacts and Me tabs via NavigationStackView

Wraps Contacts and Me screen content in NavigationStackView with
never-pushed NavigationController<()>, giving each tab a nav bar whose
background extends under the status bar (matching the Chats tab). Removes
the outer SafeArea that was double-padding the top inset — each tab and
the tab bar now own their own safe-area regions."
```

---

## Task 3: Final verification

**Files:** None modified.

- [ ] **Step 1: Build the entire workspace**

Run: `cargo build`
Expected: all crates compile with no errors or warnings (warnings would indicate an unused import or variable from the refactor — fix before completing).

- [ ] **Step 2: Run the entire workspace test suite**

Run: `cargo test`
Expected: all tests pass across `vexo`, `vexo_uikit`, `shared_app`, `desktop_demo`.

- [ ] **Step 3: Manual visual verification (hand off to user)**

Tell the user:

> Please run `cargo run -p desktop_demo` and verify:
> 1. The Chats tab shows a "Chats" nav bar with no gap between the bar and the top of the window.
> 2. The Contacts tab shows a "Contacts" nav bar with no gap above it.
> 3. The Me tab shows a "Me" nav bar with no gap above it.
> 4. Switching between tabs preserves the nav bar on each tab.
> 5. The tab bar at the bottom still renders correctly (on desktop the safe-area is zero, so no visual change there — the structural change matters for iOS).

Do **not** run `cargo run -p desktop_demo` yourself (per CLAUDE.md: "Never run `cargo run -p desktop_demo` yourself"). Wait for the user's confirmation.
