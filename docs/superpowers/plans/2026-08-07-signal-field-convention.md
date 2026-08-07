# Signal Field Convention (Derived-in-State) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Document the signal-field convention in `docs/rebuild-skipping-patterns.md` and refactor `ChatScreen` to follow it (derived Signal in State, not Widget).

**Architecture:** The `ChatScreen` widget currently takes the root `Signal<HashMap<ConvId, Vec<Message>>>` as a field and filters by `conv_id` in `render()`. This works but rebuilds on every foreign-conversation mutation. The refactor moves the derived `Signal<Vec<Message>>` into `ChatScreenState` (created once in `on_mount`, stable Arc identity), so the subscription survives `should_rebuild == false` and foreign-conversation mutations don't trigger rebuilds.

**Tech Stack:** Rust, Vexo framework (`Component`, `ComponentState`, `Signal`, `LifecycleContext`, `RenderContext`)

## Global Constraints

- The `ChatScreen.messages` widget field type stays `Signal<HashMap<ConvId, Vec<Message>>>` (root Signal — no change to callers in `mod.rs`/`desktop.rs` or tests).
- The derived Signal lives in `ChatScreenState`, created in `on_mount` via `Signal::derive`.
- `render()` reads the derived via `ctx.signal_value(state.derived_messages.as_ref().unwrap())`.
- No `on_update` re-derivation (root Signal identity is stable by contract).
- All 32 existing tests must pass unchanged after the refactor.
- No framework changes — documentation + code refactor only.

---

## File Structure

- **Modify:** `docs/rebuild-skipping-patterns.md` — new "Signal field rule" section after Level 3
- **Modify:** `shared_app/src/chats/chat_screen.rs` — add `derived_messages` to `ChatScreenState`, create in `on_mount`, read in `render()`
- **No change:** `shared_app/src/chats/mod.rs`, `shared_app/src/chats/desktop.rs` — already pass root Signal
- **No change:** test call sites in `chat_screen.rs` — already pass root Signal

---

### Task 1: Add "Signal field rule" section to rebuild-skipping-patterns.md

**Files:**
- Modify: `docs/rebuild-skipping-patterns.md` (insert after line 202, before the `---` at line 203)

**Interfaces:**
- Produces: documentation section describing the convention

- [ ] **Step 1: Insert the new section**

Insert the following section after the Level 3 section's closing text (line 201, after "stable props (closures/controllers differ, data doesn't).") and before the `---` separator at line 203:

```markdown

---

## Signal field rule: root Signals only, derive in State

**This rule is load-bearing for any component using Level 3
(`should_rebuild() == false`). Violating it causes silent state-driven-rebuild
failures.**

### The rule

**Signal widget fields must be root Signals (identity-stable for the element's
lifetime). Never pass `Signal::derive(...)` created in parent `render()` as a
child widget field.**

### Why

When `should_rebuild` returns `false`, the framework still replaces widget
fields (`StatefulElement::update`, `stateful_widget.rs`) but skips `render()`.
`signal_value` — which registers the dirty_callback as a weak subscriber on
the Signal — is only called during `render()`. If a parent passes a fresh
`Signal::derive(...)` each cascade, the new derived Signal never gets a
subscriber, and state-driven rebuilds silently break.

Root Signals don't have this problem: they're `Arc`-cloned (same identity), so
the subscription registered on mount persists across render-skips.

### The derived-in-State pattern

When a child needs a derived view of a root Signal (e.g. filtering a
`HashMap<ConvId, Vec<Message>>` to one conversation's slice), the derived
Signal must live in **State**, not the Widget struct:

```rust
struct ChatScreen {
    conv_id: ConvId,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,  // root, not derived
}

struct ChatScreenState {
    derived_messages: Option<Signal<Vec<Message>>>,
}

impl ComponentState for ChatScreenState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        let widget = ctx.widget().downcast_ref::<ChatScreen>().unwrap();
        let conv_id = widget.conv_id.clone();
        let root = widget.messages.clone();
        self.derived_messages = Some(Signal::derive(root, move |map| {
            map.get(&conv_id).cloned().unwrap_or_default()
        }));
    }
}

impl Component for ChatScreen {
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let messages = ctx.signal_value(state.derived_messages.as_ref().unwrap());
        // ...
    }
}
```

The derived's Arc identity is stable (created once in `on_mount`, owned by
State which persists across widget replacements), so the weak subscription
survives `should_rebuild == false`.

### Why no `on_update` re-derivation

Root Signals are created once and live for the app's lifetime. No existing
code path swaps the root source for an already-mounted element. If a future
feature needs that, `on_update` re-derivation with `Signal::ptr_eq` checks
is the trigger — not pre-building it now.
```

- [ ] **Step 2: Verify the doc reads correctly**

Run: `grep -c "##" docs/rebuild-skipping-patterns.md`
Expected: a number ≥ 6 (the original 5 sections + the new one)

- [ ] **Step 3: Commit**

```bash
git add docs/rebuild-skipping-patterns.md
git commit -m "docs: add signal field rule to rebuild-skipping-patterns

Documents the derived-in-State pattern: Signal widget fields must be
root Signals; never pass Signal::derive created in parent render as a
child widget field, or state-driven rebuilds silently break when
should_rebuild returns false."
```

---

### Task 2: Refactor ChatScreen to derived-in-State pattern

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs`

**Interfaces:**
- Consumes: `Signal::derive` (from `vexo::reactive`), `LifecycleContext::widget()` + `dirty_callback()` (existing)
- Produces: `ChatScreenState.derived_messages: Option<Signal<Vec<Message>>>` field, populated in `on_mount`, read in `render()`

- [ ] **Step 1: Add `derived_messages` field to `ChatScreenState`**

In `shared_app/src/chats/chat_screen.rs`, find the `ChatScreenState` struct (line 60) and add the field:

Replace:
```rust
#[derive(Default)]
pub(crate) struct ChatScreenState {
    text_controller: Option<TextEditingController>,
    /// Decoded avatar image data, cached so we don't re-decode the PNG on
    /// every rebuild (ChatScreen rebuilds on every MediaQuery change, i.e.
    /// every keyboard animation frame — 40+ PNG decodes/frame = 63ms).
    them_avatar_image: Option<ImageData>,
    me_avatar_image: Option<ImageData>,
}
```

With:
```rust
// Note: cannot #[derive(Default)] because `Signal<Vec<Message>>` doesn't
// impl Default. We implement Default manually below, initializing
// `derived_messages` to `None` (populated in `on_mount`).
pub(crate) struct ChatScreenState {
    text_controller: Option<TextEditingController>,
    /// Decoded avatar image data, cached so we don't re-decode the PNG on
    /// every rebuild (ChatScreen rebuilds on every MediaQuery change, i.e.
    /// every keyboard animation frame — 40+ PNG decodes/frame = 63ms).
    them_avatar_image: Option<ImageData>,
    me_avatar_image: Option<ImageData>,
    /// Derived per-conversation messages Signal, created once in `on_mount`
    /// from the root `messages` Signal + `conv_id`. Lives in State (not the
    /// Widget struct) so its Arc identity is stable across widget
    /// replacements — critical for `should_rebuild == false` (see the
    /// "Signal field rule" section in `docs/rebuild-skipping-patterns.md`).
    derived_messages: Option<Signal<Vec<Message>>>,
}

impl Default for ChatScreenState {
    fn default() -> Self {
        Self {
            text_controller: None,
            them_avatar_image: None,
            me_avatar_image: None,
            derived_messages: None,
        }
    }
}
```

- [ ] **Step 2: Create the derived Signal in `on_mount`**

Find the `on_mount` impl (line 94) and add the derivation. Replace:

```rust
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        self.sync_controller();
        if let Some(tc) = self.text_controller.as_ref() {
            tc.set_dirty_callback(ctx.dirty_callback());
        }
    }
```

With:

```rust
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        self.sync_controller();
        if let Some(tc) = self.text_controller.as_ref() {
            tc.set_dirty_callback(ctx.dirty_callback());
        }

        // Create the derived per-conversation Signal from the root messages
        // Signal + conv_id. This must live in State (not Widget) so the
        // derived's Arc identity is stable across parent cascades — when
        // should_rebuild returns false, render() is skipped and
        // signal_value is not re-called, but the subscription on the
        // State-owned derived survives. See the "Signal field rule" in
        // docs/rebuild-skipping-patterns.md.
        let widget = ctx
            .widget()
            .downcast_ref::<ChatScreen>()
            .expect("ChatScreenState::on_mount: widget must be ChatScreen");
        let conv_id = widget.conv_id.clone();
        let root = widget.messages.clone();
        self.derived_messages = Some(Signal::derive(root, move |map| {
            map.get(&conv_id).cloned().unwrap_or_default()
        }));
    }
```

- [ ] **Step 3: Read the derived Signal in `render()`**

Find the `render` method (line 130). Replace the root-Signal filtering logic:

```rust
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);

        // Read the ROOT messages Signal (registers dirty_callback on the
        // root, which persists across widget replacements — see the field
        // comment). Filter by `conv_id` to get this conversation's messages.
        let all_messages = ctx.signal_value(&self.messages);
        let messages = all_messages.get(&self.conv_id).cloned().unwrap_or_default();
```

With:

```rust
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);

        // Read the State-owned derived Signal (not the root). The derived
        // filters to this conversation's messages and its Arc identity is
        // stable (created once in on_mount), so the subscription survives
        // should_rebuild == false. See "Signal field rule" in
        // docs/rebuild-skipping-patterns.md.
        let messages = ctx.signal_value(
            state
                .derived_messages
                .as_ref()
                .expect("derived_messages must be set in on_mount before render"),
        );
```

- [ ] **Step 4: Update the `messages` field doc comment**

The field comment on `ChatScreen.messages` (lines 22-29) currently explains why it's a root Signal with filtering in render. Update it to reflect the new pattern. Replace:

```rust
    /// Root messages Signal (NOT a derived per-conversation Signal). We filter
    /// by `conv_id` in `render()`. This is critical: a derived Signal created
    /// in `DesktopChatsPage::render` would be recreated on every parent
    /// cascade, but `should_rebuild` returns false — so the new derived
    /// Signal's dirty_callback would never be registered (render is skipped),
    /// and state-driven rebuilds would silently break. By using the root
    /// Signal directly, the dirty_callback is registered on a Signal that
    /// persists across widget replacements.
    pub(crate) messages: Signal<std::collections::HashMap<ConvId, Vec<Message>>>,
```

With:

```rust
    /// Root messages Signal (identity-stable for this element's lifetime).
    /// Must be a root Signal, NOT a `Signal::derive` created in parent
    /// render — see "Signal field rule" in
    /// `docs/rebuild-skipping-patterns.md`. The per-conversation derived
    /// Signal lives in `ChatScreenState::derived_messages` (created in
    /// `on_mount`), so its subscription survives `should_rebuild == false`.
    pub(crate) messages: Signal<std::collections::HashMap<ConvId, Vec<Message>>>,
```

- [ ] **Step 5: Update the `should_rebuild` doc comment**

The `should_rebuild` doc comment (lines 119-125) references "the `messages` Signal drives state-driven rebuilds via `RenderContext::signal_value`". Update it to reflect that the derived (in State) drives rebuilds. Replace:

```rust
    /// Level 3 rebuild-skip (see `docs/rebuild-skipping-patterns.md`).
    /// During keyboard animation, TabBar and NavigationStack cascade `update()`
    /// to ChatScreen with fresh closure fields but identical data. Only
    /// `conv_id` participates in identity — the `messages` Signal drives
    /// state-driven rebuilds via `RenderContext::signal_value`, so the parent
    /// cascade can stop here without re-rendering message bubbles.
    fn should_rebuild(&self, old: &Self) -> bool {
        self.conv_id != old.conv_id
    }
```

With:

```rust
    /// Level 3 rebuild-skip (see `docs/rebuild-skipping-patterns.md`).
    /// During keyboard animation, TabBar and NavigationStack cascade `update()`
    /// to ChatScreen with fresh closure fields but identical data. Only
    /// `conv_id` participates in identity — the derived messages Signal in
    /// State drives state-driven rebuilds via `RenderContext::signal_value`,
    /// so the parent cascade can stop here without re-rendering message
    /// bubbles. See "Signal field rule" in `rebuild-skipping-patterns.md`
    /// for why the derived must live in State, not Widget.
    fn should_rebuild(&self, old: &Self) -> bool {
        self.conv_id != old.conv_id
    }
```

- [ ] **Step 6: Build to verify compilation**

Run: `cargo build -p shared_app`
Expected: compiles with no errors (warnings ok if about unused imports)

- [ ] **Step 7: Run tests to verify behavior**

Run: `cargo test -p shared_app`
Expected: all 32 tests pass, including:
- `test_reaction_chip_renders_for_seeded_reactions`
- `test_reaction_toggle_end_to_end`
- `test_right_click_menu_contains_reactions_and_items`
- `test_metrics_match_real_sizes`

- [ ] **Step 8: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "refactor: move derived messages Signal to ChatScreenState

Follows the signal field convention: derived Signal lives in State
(created in on_mount, stable Arc identity) so its subscription
survives should_rebuild == false. Also avoids spurious rebuilds from
foreign-conversation mutations."
```
