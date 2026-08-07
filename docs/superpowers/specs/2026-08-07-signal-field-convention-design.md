# Signal Field Convention (Derived-in-State Pattern)

**Status:** Convention spec
**Scope:** `vexo/` framework + all app/widget authors who use `should_rebuild`

## Problem

A silent state-driven-rebuild failure was discovered in `ChatScreen` after adding
the reaction-toggle feature. Tapping a reaction icon correctly mutated the root
messages `Signal` (verified via logs), but the bubble's reaction chip never
updated — the state-driven rebuild never fired.

### Root cause

`ChatScreen` received a `Signal::derive(...)` created fresh in
`DesktopChatsPage::render` as its `messages` widget field. When
`should_rebuild` returned `false` (keyboard animation cascade), the framework:

1. **Replaced the widget struct** (`StatefulElement::update`,
   `stateful_widget.rs:694`) — the old derived `Signal` (with its registered
   weak subscription) was dropped.
2. **Skipped `render()`** (`stateful_widget.rs:726`) — so `signal_value`
   (which re-registers the dirty_callback as a weak subscriber on the Signal)
   was never called on the new derived `Signal`.

The new derived `Signal` had no subscribers. `Signal::set_from` fired, the
derived's closure computed the new value, but `notify()` had no live weak refs
to upgrade — the dirty_callback was never sent. The mutation was invisible.

### Why root Signals don't have this problem

Root Signals are `Arc`-based and created once (in `ImState`/`seed`). When a
parent clones and passes a root Signal as a widget field, the Arc identity is
stable across parent cascades. The subscription registered via `signal_value`
on mount (or last successful render) persists across `should_rebuild == false`
because the Signal's Arc doesn't change — the weak ref points to the same
`SignalInner` whether or not `render()` runs.

Derived Signals created in parent `render()` are **new `Arc`s each cascade**.
If `render()` is skipped, the new Arc never gets a subscriber.

## The Convention

### Rule

**Signal widget fields must be root Signals (identity-stable for the element's
lifetime). Never pass `Signal::derive(...)` created in parent `render()` as a
child widget field.**

### Why this is a convention, not a framework guard

A framework-level `re_subscribe` hook (called on every `update()` regardless of
`should_rebuild`) would technically fix the bug for derived Signal fields. But:

1. It adds hot-path overhead (the whole point of `should_rebuild == false` is
   skipping work in keyboard-animation frames).
2. It relocates the failure mode rather than eliminating it (authors must
   remember to implement `re_subscribe`; same bug class, new location).
3. The convention has **zero runtime cost** and **no failure mode once
   followed** — the subscription is registered once on mount, never refreshed.

The convention follows the proven Flutter/React model: subscriptions live in
State, which persists across widget replacement.

## The Pattern (when derivation is needed)

When a child needs a derived view of a root Signal (e.g. filtering a
`HashMap<ConvId, Vec<Message>>` to one conversation's slice), the derived
Signal must live in **State**, not the Widget struct.

### Structure

```rust
// Widget field: ROOT Signal (identity-stable)
struct ChatScreen {
    conv_id: ConvId,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,  // root, not derived
    // ...
}

// State field: derived Signal (created once, stable Arc identity)
struct ChatScreenState {
    derived_messages: Option<Signal<Vec<Message>>>,
    // ...
}

impl ComponentState for ChatScreenState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        let widget = ctx.widget().downcast_ref::<ChatScreen>().unwrap();
        let conv_id = widget.conv_id.clone();      // clone out before closure
        let root = widget.messages.clone();         // Arc clone, stable identity
        self.derived_messages = Some(Signal::derive(root, move |map| {
            map.get(&conv_id).cloned().unwrap_or_default()
        }));
    }

    // No on_update re-derivation needed — root Signal identity is stable for
    // the element's lifetime (by contract). If a future feature genuinely
    // swaps root sources mid-lifetime, that's the trigger for revisiting.
}

impl Component for ChatScreen {
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        // signal_value registers the dirty_callback as a weak subscriber on
        // the State-owned derived Signal. The derived's Arc identity is stable
        // (created once in on_mount), so the subscription survives
        // should_rebuild == false.
        let messages = ctx.signal_value(state.derived_messages.as_ref().unwrap());
        // ... build bubbles from `messages` ...
    }
}
```

### Why this is safe under `should_rebuild == false`

| Step | What happens | Subscription state |
|------|-------------|-------------------|
| `on_mount` | Derived Signal created (Arc #1) | dirty_callback subscribed to Arc #1 |
| Parent cascade, `should_rebuild == false` | Widget fields replaced, `render()` skipped | Arc #1 **still alive** (owned by State); subscription intact |
| `Signal::set_from` fires | Derived recomputes, calls `notify()` | Weak ref upgrades successfully → dirty_callback fires → `rebuild_from_state()` runs |

The key: **State owns the derived Signal's Arc, not the Widget.** The Widget
field is the root Signal (stable Arc), which the State reads in `on_mount` to
create the derived. The derived's Arc identity never changes across widget
replacements because State persists.

### Why no `on_update` re-derivation

Root Signals are created once in `ImState`/`seed()` and live for the app's
lifetime. `ChatScreen` receives `state.messages.clone()` — an Arc clone, same
identity. No existing code path swaps the root Signal source for an
already-mounted element.

If a caller violated this contract (swapped root source on a live element),
the bug would be at the call site, not in the State. The visible symptom —
"state-driven rebuilds stop working" — points directly at the violated
contract.

If a future feature genuinely needs to swap root sources mid-lifetime (e.g.
multi-account switching), that's the trigger for adding `on_update`
re-derivation with `Signal::ptr_eq` identity checks — not pre-building
machinery for it now.

## Deliverables

1. **New section in `docs/rebuild-skipping-patterns.md`** documenting the rule,
   rationale, and the derived-in-State pattern with the code example above.

2. **Refactor `ChatScreen`** from the current quick-fix (root Signal as widget
   field, filter in `render()`) to the derived-in-State pattern. The quick-fix
   was shipped first to unblock the reaction feature; this refactor aligns the
   code with the convention and restores the efficiency the derived Signal was
   designed for (foreign-conversation mutations don't trigger rebuilds of
   unrelated chat screens).

   Files affected:
   - `shared_app/src/chats/chat_screen.rs` — add `derived_messages` to State,
     create in `on_mount`, read via `signal_value` in `render()`
   - `shared_app/src/chats/mod.rs` — no change (already passes root Signal)
   - `shared_app/src/chats/desktop.rs` — no change (already passes root Signal)
   - Test call sites in `chat_screen.rs` — no change (already pass root Signal)

3. **No framework changes.** The convention is documentation + a code pattern.
   No new traits, no new lifecycle hooks, no runtime checks.

## Testing

The existing `test_reaction_toggle_end_to_end` test covers the core behavior
(tap → mutate → rebuild → chip appears → tap again → chip disappears). After
the refactor, this test must still pass — proving the derived-in-State pattern
preserves the fix.

No new tests are needed for the convention itself — it's a documentation +
refactor task, not a feature. The refactor's correctness is verified by the
existing test suite (32 tests).
