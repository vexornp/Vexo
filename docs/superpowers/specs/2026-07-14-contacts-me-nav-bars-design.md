# Nav Bars for Contacts & Me Tabs — Design

**Date:** 2026-07-14
**Status:** Approved (pending spec review)
**Scope:** `shared_app`, `vexo_uikit/src/tab_bar.rs`

## Problem

The IM mock app has three tabs (Chats / Contacts / Me). Only the Chats tab has a
nav bar — provided by `NavigationStackView`, which extends its bar background
under the status bar. The Contacts and Me tabs render their content directly
with no chrome.

Additionally, `shared_app/src/lib.rs:707` wraps the entire `TabBarView` in a
`SafeArea::new(...)` (default `top: true`). This conflicts with the
`NavigationStackView` on the Chats tab, whose own nav bar is already designed
to consume the top safe-area inset (`navigation.rs:796-798` adds `safe.top` to
the bar's height so its background covers the status bar). The result: a
double-padded gap of empty space between the nav bar and the status bar.

## Goal

1. Give the Contacts and Me tabs nav bars matching the Chats tab's visual
   style.
2. Eliminate the top-inset double-padding by letting each component own its
   safe-area region instead of an outer `SafeArea` wrapping everything.

## Approach

Reuse `NavigationStackView` for Contacts and Me with a controller that is
never pushed. `NavigationStackView::new` defaults its `destination` closure to
`|_| Text::new("").boxed()` and its `title` closure to `|_| String::new()`
(`navigation.rs:307-308`). Both closures are only called when a destination is
pushed; with a never-pushed controller, only `root_title` and the root widget
are used. So `NavigationController<()>` ("no destinations") is the natural
controller type for these tabs.

`NavigationStackView` already:
- Builds a nav bar whose height includes `safe.top` (background extends under
  the status bar — Flutter `AppBar` behavior).
- Wraps its content in `SafeArea::new(content).top(false)`
  (`navigation.rs:703`) so left/right/bottom insets still apply.

So wrapping Contacts/Me in `NavigationStackView` gives them nav bars and
correct safe-area handling with no new framework code.

The outer `SafeArea` in `shared_app/src/lib.rs:707` was also the only thing
handling the **bottom** safe-area (home indicator) for the tab bar. Removing
it requires the bottom inset to be handled inside `TabBarView` itself —
mirroring the pattern `NavigationStackView` uses for the top inset.

## Changes

### `shared_app/src/lib.rs`

1. **Add two controllers to `ImState`** (annotated by `#[derive(ComponentState)]`):
   ```rust
   contacts_nav: NavigationController<()>,
   me_nav: NavigationController<()>,
   ```
   Initialize them in `seed()`.

   **Why stable identity matters:** `NavigationStackViewState::on_update`
   (`navigation.rs:443-453`) re-wires the dirty callback only when
   `Rc::ptr_eq(&old.controller.path, &new.controller.path)` is false. If a
   fresh controller were created per render, the dirty callback would never
   re-wire correctly. Storing the controller in `ImState` (and `.clone()`-ing
   it, which shares the inner `Rc<RefCell<Vec<Dest>>>`) preserves identity
   across renders.

2. **Wrap Contacts in `NavigationStackView`** in the `ImTab::Contacts` arm of
   `view()`:
   ```rust
   ImTab::Contacts => NavigationStackView::new(
       contacts_nav.clone(),
       build_contacts_screen(contacts.clone()),
   )
   .root_title("Contacts")
   .boxed(),
   ```

3. **Wrap Me in `NavigationStackView`** in the `ImTab::Me` arm:
   ```rust
   ImTab::Me => NavigationStackView::new(
       me_nav.clone(),
       build_profile_screen(&profile),
   )
   .root_title("Me")
   .boxed(),
   ```
   The `profile` variable is already an owned `Profile` (cloned from
   `state.profile` at the top of `view()`, line 629) moved into the
   `move |tab|` closure. `build_profile_screen(&profile)` borrows it for the
   duration of the call, then returns an owned `Box<dyn Widget>` — safe to
   pass as the `NavigationStackView` root. A fresh `me_nav.clone()` must be
   captured before the closure (mirroring `nav_for_chat` at line 625):
   ```rust
   let me_nav = state.me_nav.clone();
   let contacts_nav = state.contacts_nav.clone();
   ```
   captured alongside the existing `nav_for_chat`, `convs_for_chat`, etc.

4. **Remove the outer `SafeArea`** at line 707:
   ```rust
   // Before:
   SafeArea::new(tab_view.boxed()).boxed()
   // After:
   tab_view.boxed()
   ```
   Remove `SafeArea` from the `use vexo::{...}` import if no other call site
   in this file uses it (grep confirms only line 707 uses it).

### `vexo_uikit/src/tab_bar.rs`

1. **Import `SafeArea`** from `vexo` (add to the existing `use vexo::{...}`).
2. **Wrap the tab bar row in `SafeArea`** inside `TabBarView::render`
   (`tab_bar.rs:178-188`):
   ```rust
   let bar = SafeArea::new(bar.boxed()).top(false).boxed();

   Flex::column()
       .layout(/* unchanged */)
       .push(stack.flex_grow(1.0))
       .push(bar)
       .boxed()
   ```
   `top(false)` because the tab bar lives at the bottom — only
   left/right/bottom insets apply (bottom = home indicator, left/right =
   landscape notch). This mirrors `NavigationStackView`'s
   `SafeArea::new(content).top(false)` at `navigation.rs:703`.

## Why `NavigationController<()>` (not a generic placeholder enum)

`()` satisfies `Hash + Eq + Clone + 'static`. With a never-pushed controller,
no code ever constructs or matches on the destination type, so `()` is the
minimal honest type — no `PhantomRoute` placeholder enum needed.

## Out of Scope

- **Tab bar background.** `TabBarView` currently sets no background on the
  bar row. After this change, the home-indicator region below the tab items
  will show whatever's behind (page content color bleeding through). Adding
  an opaque tab-bar background that extends to the screen edge is a separate
  enhancement and not required for the nav-bar / safe-area fix.
- **`ChatScreen` input bar.** The Chats tab's `ChatScreen` input bar already
  gets its bottom safe-area from the `NavigationStackView`'s inner
  `SafeArea(top=false)` wrapper (`navigation.rs:703`), so it's unaffected by
  removing the outer `SafeArea`.

## Verification

- `cargo build` after edits — confirms types compile (especially the
  `NavigationController<()>` generic and the borrow on `&profile` in the
  `move |tab|` closure).
- `cargo test` — existing tests in `shared_app/src/lib.rs` assert
  `pipeline.element_registry().len() > N`. Adding nav bars and SafeArea
  wrappers only increases element counts; no thresholds regress. No snapshot
  tests exist.
- Manual: run the desktop demo and confirm (a) Contacts and Me show nav bars
  with "Contacts" / "Me" titles, (b) no white gap between any tab's nav bar
  and the top of the window, (c) tab bar still sits above the bottom edge
  (desktop safe-area is zero, so no visual change there; the structural
  change matters for iOS).
