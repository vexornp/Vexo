# Per-Item Detail State Preservation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On desktop, preserve each sidebar item's detail-page state (text-edit content, scroll position, pushed nav stack) across sidebar toggles.

**Architecture:** Replace the desktop's single shared `NavigationController` + fresh `DetailPage` per toggle with one `IndexedStack` of 6 `NavigationStackView`s, each owning its own `NavigationController<Dest>` and `DetailPage` for a fixed item id. Toggling the sidebar flips the index — no rebuild, no `pop_to_root`. State survives because `IndexedStack` wraps each child in `Offstage`, keeping all subtrees mounted.

**Tech Stack:** Rust, vexo framework (`IndexedStack`, `Offstage`, `NavigationStackView`, `NavigationController`), `shared_app` demo crate.

## Global Constraints

- Single file modified: `shared_app/src/lib.rs`
- No framework changes — uses existing `IndexedStack` (`vexo/src/widgets/indexed_stack.rs`) and `Offstage` (`vexo/src/widgets/offstage.rs`) primitives
- Mobile path behavior unchanged (still uses a single shared nav stack with push/pop)
- `NavigationController<Dest>: Default` and `: Clone` (shares underlying `Rc<RefCell<...>>` cells) — already verified at `vexo_uikit/src/navigation.rs:228-242`
- `#[derive(ComponentState)]` only wires `Signal<T>` / `Option<Signal<T>>` fields; non-Signal fields are silently ignored (`vexo/component_state_derive/src/lib.rs:24`) — the derive stays on `State`
- `IndexedStack` is exported from `vexo` (`vexo/src/lib.rs:198`) — import already present in `lib.rs:4-8`
- Spec: `docs/superpowers/specs/2026-07-08-per-item-detail-state-preservation-design.md`
- Build verification command: `cargo build -p shared_app`
- Never run `cargo run -p desktop_demo` — ask the user to run it for GUI verification

---

## File Structure

**Single file modified:** `shared_app/src/lib.rs`

This is a wiring change in the demo app. No new files. The change is confined to:
- `State` struct definition (lines ~39-45)
- `State::new()` (lines ~50-54)
- Desktop branch of `State::view()` (lines ~65-112)
- Mobile branch of `State::view()` (lines ~113-143) — controller reference rename only
- New helper `fn selected_index(...)` added near `item_label`

Unchanged: `DetailPage`, `DetailPageState`, `build_detail_content`, `build_page_content`, `build_sidebar`, `build_item_row`, `MobileApp`.

---

### Task 1: Update `State` struct and `new()` to carry per-item controllers

**Files:**
- Modify: `shared_app/src/lib.rs:39-54`

**Interfaces:**
- Consumes: `NavigationController<Dest>` (from `vexo_uikit`, already imported), `ITEMS` constant (already defined at `lib.rs:22-29`)
- Produces: `State.nav_controllers: Vec<NavigationController<Dest>>` (desktop, per-item), `State.mobile_nav_controller: NavigationController<Dest>` (mobile, single shared) — both used by Task 2 and Task 3

- [ ] **Step 1: Update the `State` struct**

Find this block in `shared_app/src/lib.rs` (lines 39-45):

```rust
#[derive(ComponentState, Default)]
pub struct State {
    selection_log: Signal<u32>,
    /// Desktop sidebar selection (mobile uses the nav stack for everything).
    selected: Signal<Option<&'static str>>,
    nav_controller: NavigationController<Dest>,
}
```

Replace with:

```rust
#[derive(ComponentState, Default)]
pub struct State {
    selection_log: Signal<u32>,
    /// Desktop sidebar selection (mobile uses the nav stack for everything).
    selected: Signal<Option<&'static str>>,
    /// Desktop: one controller per sidebar item, indexed by `ITEMS` position.
    /// Each item's nav stack persists across sidebar toggles because the
    /// corresponding `NavigationStackView` stays mounted inside the
    /// `IndexedStack` (wrapped in `Offstage`).
    nav_controllers: Vec<NavigationController<Dest>>,
    /// Mobile: single shared nav stack. Semantically distinct from desktop's
    /// per-item stacks; must persist in `State` (not be created per `view()`)
    /// because `NavigationStackView`'s `on_mount` wires its dirty callback and
    /// its path must survive across rebuilds.
    mobile_nav_controller: NavigationController<Dest>,
}
```

- [ ] **Step 2: Update `State::new()` to backfill `nav_controllers`**

Find this block in `shared_app/src/lib.rs` (lines 50-54):

```rust
fn new() -> Self::State {
    let state = Self::State::default();
    state.selected.set(Some("inbox"));
    state
}
```

Replace with:

```rust
fn new() -> Self::State {
    let mut state = Self::State::default();
    state.selected.set(Some("inbox"));
    // Backfill per-item controllers for desktop. `mobile_nav_controller` is
    // initialized by `Default` (empty path). Length is fixed at ITEMS.len().
    while state.nav_controllers.len() < ITEMS.len() {
        state.nav_controllers.push(NavigationController::new());
    }
    state
}
```

- [ ] **Step 3: Verify it compiles (mobile/desktop `view()` still reference the old field)**

Run: `cargo build -p shared_app 2>&1 | head -40`

Expected: FAIL with `no field 'nav_controller' on type 'State'` errors in both the desktop and mobile branches of `view()`. This is expected — Task 2 and Task 3 fix those references.

(If it fails for a different reason — e.g. derive-macro rejection of `Vec<NavigationController<Dest>>` — stop and investigate. The derive macro at `vexo/component_state_derive/src/lib.rs:24` only wires `Signal<T>` fields and ignores all others, so this should not happen. If it does, drop `#[derive(ComponentState)]` and implement `ComponentState` manually for `State` with an empty `set_dirty_callback` — but `State` has `Signal` fields that need wiring, so the derive must stay. Investigate before proceeding.)

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "refactor(shared_app): split State nav_controller into per-item vec + mobile controller

Desktop needs one NavigationController per sidebar item (for IndexedStack
keep-alive); mobile needs a single shared controller. This is the struct
shape change only — view() references are fixed in the following commits."
```

---

### Task 2: Add `selected_index` helper

**Files:**
- Modify: `shared_app/src/lib.rs` (add helper after `item_label`, around line 37)

**Interfaces:**
- Consumes: `ITEMS` constant (already defined at `lib.rs:22-29`)
- Produces: `fn selected_index(selected: Option<&'static str>) -> usize` — used by Task 3

- [ ] **Step 1: Add the helper function**

Find this block in `shared_app/src/lib.rs` (lines 31-37):

```rust
fn item_label(id: &str) -> String {
    ITEMS
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, l)| l.to_string())
        .unwrap_or_else(|| id.to_string())
}
```

Add immediately after it:

```rust
/// Map the desktop sidebar selection to an `IndexedStack` child index.
/// Falls back to `0` (Inbox) if `selected` is `None` — unreachable on
/// desktop in practice (`new()` sets `Some("inbox")`, sidebar only ever
/// sets `Some(id)`), but defensive.
fn selected_index(selected: Option<&'static str>) -> usize {
    selected
        .and_then(|id| ITEMS.iter().position(|(i, _)| *i == id))
        .unwrap_or(0)
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p shared_app 2>&1 | head -20`

Expected: Same errors as Task 1 Step 3 (`no field 'nav_controller'` in `view()`). The new helper itself compiles cleanly — unused-function warnings are suppressed because `cargo build` does not fail on warnings, and the helper is `pub`-visible at module level.

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat(shared_app): add selected_index helper for IndexedStack index derivation"
```

---

### Task 3: Rewrite desktop `view()` branch to use `IndexedStack` of per-item `NavigationStackView`s

**Files:**
- Modify: `shared_app/src/lib.rs:60-146` (the `view()` function body)

**Interfaces:**
- Consumes: `State.nav_controllers` (from Task 1), `selected_index` (from Task 2), `build_detail_content`, `build_page_content`, `build_sidebar` (existing helpers, unchanged), `IndexedStack` (already imported at `lib.rs:4-8`)
- Produces: A desktop view that preserves per-item state across sidebar toggles

- [ ] **Step 1: Rewrite the desktop branch of `view()`**

Find the desktop branch in `shared_app/src/lib.rs`. The current block starts at line 66 (`Platform::Desktop => {`) and ends at line 112 (`}`), inside the `match Platform::current()` at line 65. The full current `view()` function is lines 60-146.

Replace the entire `view()` function body (lines 60-146) with:

```rust
fn view(state: &mut Self::State) -> Box<dyn Widget> {
    let selected_signal = state.selected.clone();
    let selection_count = state.selection_log.clone();

    match Platform::current() {
        Platform::Desktop => {
            let current = selected_signal.get_cloned();
            let index = selected_index(current);

            // Sidebar: callback now just sets selection — no nav mutation.
            // The IndexedStack index flip is the only effect; each item's
            // nav stack is untouched on toggle.
            let selected_for_cb = selected_signal.clone();
            let sidebar = build_sidebar(
                current,
                Rc::new(move |id| {
                    selected_for_cb.set(Some(id));
                }),
                false,
            );

            // IndexedStack: one child per sidebar item, each with its own
            // NavigationStackView + NavigationController. All children stay
            // mounted (wrapped in Offstage by IndexedStack); toggling the
            // sidebar flips offstage flags, preserving each item's detail
            // state (text-edit content, scroll position) and pushed nav
            // stack.
            let mut stack = IndexedStack::new(index);
            for (i, (id, label)) in ITEMS.iter().enumerate() {
                let ctrl = state.nav_controllers[i].clone();
                let detail = build_detail_content(id, selection_count.clone(), ctrl.clone());
                let nav_for_dest = ctrl.clone();
                stack = stack.push(
                    NavigationStackView::new(ctrl, detail)
                        .root_title(label.to_string())
                        .title(|d| match d {
                            Dest::Page(n) => format!("Page: {}", n),
                            _ => String::new(),
                        })
                        .destination(move |d| match d {
                            Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                            _ => Text::new("").boxed(),
                        })
                        .boxed(),
                );
            }

            SafeArea::new(
                Flex::row()
                    .background(Color::WHITE)
                    .push(sidebar)
                    .push(stack.flex_grow(1.0)),
            )
            .boxed()
        }
        Platform::Mobile => {
            let nav_for_select = state.mobile_nav_controller.clone();
            let sidebar = build_sidebar(
                None,
                Rc::new(move |id| {
                    nav_for_select.push(Dest::Item(id));
                }),
                true,
            );

            let nav_for_dest = state.mobile_nav_controller.clone();
            let count_for_dest = selection_count.clone();

            SafeArea::new(
                NavigationStackView::new(state.mobile_nav_controller.clone(), sidebar)
                    .root_title("Navigation")
                    .title(|d| match d {
                        Dest::Item(id) => item_label(*id),
                        Dest::Page(n) => format!("Page: {}", n),
                    })
                    .destination(move |d| match d {
                        Dest::Item(id) => build_detail_content(
                            *id,
                            count_for_dest.clone(),
                            nav_for_dest.clone(),
                        ),
                        Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                    }),
            )
            .boxed()
        }
    }
}
```

Key changes from the original:
1. Removed `let nav_controller = state.nav_controller.clone();` at the top of `view()` (line 62) — desktop now iterates `state.nav_controllers`, mobile uses `state.mobile_nav_controller`.
2. Desktop sidebar callback no longer calls `nav_for_cb.pop_to_root()` — that was the state-destroyer.
3. Desktop builds an `IndexedStack` of 6 `NavigationStackView`s instead of one.
4. Mobile branch: `state.nav_controller` → `state.mobile_nav_controller` (three references: `nav_for_select`, `nav_for_dest`, and the `NavigationStackView::new` first arg).

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p shared_app 2>&1 | tail -20`

Expected: PASS (no errors). The build may emit warnings — note any `unused import` warnings; if present, remove the unused imports from the `use vexo::{...}` / `use vexo_uikit::{...}` blocks at the top of `lib.rs` before committing (Step 5). The original `use vexo::{... NavigationStackView...}` line may no longer need `NavigationController` if it was imported separately — check both import blocks.

If it fails:
- `no field 'nav_controller'` → you missed a reference; search for `state.nav_controller` and replace with the appropriate field (`mobile_nav_controller` for mobile, `state.nav_controllers[i]` for desktop).
- `mismatched types` on `IndexedStack::new(index)` → `index` is `usize`, `IndexedStack::new` takes `usize` (`indexed_stack.rs:68`); should not happen.
- `cannot borrow 'state' as mutable more than once` → the `for` loop reads `state.nav_controllers[i]` immutably; no mutable borrow needed. If this appears, check that `view()` takes `&mut Self::State` (it does, per the `Application` trait) and that the borrow of `state.nav_controllers` is released before any mutable call (there are none in the desktop branch).

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -p shared_app 2>&1 | tail -20`

Expected: PASS, or warnings only (no errors). Fix any errors clippy raises.

- [ ] **Step 4: Run the framework test suite to confirm no regressions**

Run: `cargo test -p vexo 2>&1 | tail -10`

Expected: PASS. These tests cover `IndexedStack`/`Offstage` state preservation (`stateful_integration_test.rs:1689`) and `NavigationStackView` (`navigation_stack_tests.rs`). The app change should not affect them, but confirm.

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat(shared_app): preserve per-item detail state across sidebar toggles

Desktop view() now builds an IndexedStack of 6 NavigationStackViews (one per
sidebar item, each with its own NavigationController). Toggling the sidebar
flips the IndexedStack index — all subtrees stay mounted via Offstage, so
each item's text-edit content, scroll position, and pushed nav stack survive.

Sidebar callback no longer calls pop_to_root(); mobile behavior unchanged."
```

---

### Task 4: Manual GUI verification

**Files:**
- None (verification only)

**Interfaces:**
- Consumes: The completed implementation from Tasks 1-3

- [ ] **Step 1: Ask the user to run the desktop demo**

Do NOT run `cargo run -p desktop_demo` yourself (per CLAUDE.md — you can't interact with the GUI and your terminal may be on a different display producing misleading results).

Present this to the user:

```
Please run the desktop demo and verify the following checklist:

    cargo run -p desktop_demo

Checklist:
1. Select Inbox, type in the text edit, scroll down.
2. Click Starred, push "Next page" twice (now on Page 2).
3. Click Inbox — text edit content preserved, scroll position preserved.
4. Click Starred — still on Page 2 (per-item nav stack preserved).
5. Click Drafts, push "Next page" once.
6. Cycle Inbox → Starred → Drafts → Inbox → Starred → Drafts — each
   item's state and nav depth intact.
7. "Bump counter" on any item still updates the shared counter display
   on all items.

Report any failures (which step, what you observed vs. expected).
```

- [ ] **Step 2: Triage failures (if any)**

If the user reports a failure, isolate before theorizing (per CLAUDE.md first principles):

- **State not preserved on toggle (text-edit reset, scroll reset):** Likely the `IndexedStack` is not being reconciled in place. Check that `IndexedStack::new(index)` is called with the correct index, and that the children vector is stable across `view()` calls (same order, same item ids). Add `log::debug!` with a unique prefix in `State::view()` logging the index and children count, have the user re-run with `RUST_LOG=debug | grep <prefix> | tee`, and verify the index flips but children count stays 6.

- **Pushed nav stack reset on toggle:** Likely the `NavigationController` clone identity broke — check that `state.nav_controllers[i].clone()` is taken from the same `state.nav_controllers` Vec each `view()` call (it is — `state` persists across calls because it's the `Application::State`). Add logging in the sidebar callback to confirm `push`/`pop` are not being called unexpectedly.

- **Mobile regression:** The mobile branch should be unchanged in behavior. If mobile breaks, check that all `state.nav_controller` references were replaced with `state.mobile_nav_controller` (there are three: `nav_for_select`, `nav_for_dest`, and the `NavigationStackView::new` first arg).

- **Build failure on mobile:** `state.mobile_nav_controller` must be used in the mobile branch (it's dead code on desktop, but that's fine — desktop doesn't reference it). If the compiler complains about unused `mobile_nav_controller`, that means the mobile branch wasn't updated; re-check Task 3 Step 1.

If a fix is needed, apply it, rebuild (`cargo build -p shared_app`), re-run clippy, and re-ask the user to verify.

- [ ] **Step 3: Final commit (only if Step 2 produced fixes)**

If Step 2 required fixes, commit them:

```bash
git add shared_app/src/lib.rs
git commit -m "fix(shared_app): <specific fix description>"
```

If no fixes were needed, this step is skipped — Task 3's commit is the final state.
