# Signal Read-Tracking Design

Date: 2026-07-30
Status: Draft

## Problem

Vexo's `Signal<T>` only notifies its *owning* element when `set` is called
(wired via `set_dirty_callback` by `#[derive(ComponentState)]`). When a
*non-owning* element reads a Signal in `render()`, the framework has no idea
the element depends on that Signal — so it does not rebuild the element when
the Signal changes.

This forced a workaround in `ChatScreen` (`shared_app/src/chats/chat_screen.rs`):
a `messages_reader: Rc<dyn Fn() -> Vec<Message>>` closure that reaches into
root state, plus a stale `messages: Vec<Message>` snapshot used only for
`should_rebuild()` change detection. Two sources of truth, an undeclared
dependency, and a latent gap (external message changes don't trigger a
rebuild because the framework doesn't know ChatScreen depends on the messages
Signal).

The correct fix is to make Signal reads in `render()` register a dependency,
so the framework marks the reader element dirty when the Signal changes —
matching Vue's `watchEffect` / Svelte's auto-subscription semantics, adapted
to Rust via an explicit API.

## Goals

1. A component reading a Signal in `render()` rebuilds when that Signal
   changes — without manual wiring.
2. Eliminate the `messages_reader` workaround in `ChatScreen`: one source of
   truth, one field.
3. Support derived Signals so a component can subscribe to a *slice* of a
   parent Signal without over-rebuilding when unrelated slices change.
4. No memory leaks (no reference cycles, no unbounded subscriber growth).
5. Coexist with the existing `set_dirty_callback` mechanism — no breaking
   change to `#[derive(ComponentState)]` or the `Application` trait.

## Non-Goals

- Removing `set_dirty_callback`. The owning-element notification path stays;
  read-tracking is a separate, complementary path.
- Per-conversation data model split (`HashMap<ConvId, Signal<Vec<Message>>>`).
  The whole-map `Signal<HashMap<ConvId, Vec<Message>>>` stays; derived Signals
  handle per-conversation granularity.
- Replacing `InheritedWidget` (Theme, MediaQuery). It remains the mechanism
  for tree-scoped values; read-tracking is for shared mutable state.
- Transparent/thread-local read-tracking. The API is explicit
  (`ctx.signal_value(&sig)`).

## Design

### 1. Signal subscriber list

`Signal<T>` gains a subscriber list. Subscribers are `Weak<dyn Fn()>` closures
that call `BuildOwner::mark_needs_build` on the reader element.

```rust
use std::sync::{Arc, Mutex, Weak};

struct SignalInner<T> {
    value: Mutable<T>,
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Reader-element subscriptions. `Weak` so dead subscribers (element
    /// unmounted) are skipped automatically and don't leak.
    subscribers: Mutex<Vec<Weak<dyn Fn() + Send + Sync>>>,
    /// Strong refs to subscriber closures this Signal registered on *other*
    /// Signals (parent subscriptions from `Signal::derive`). Owned here so
    /// they stay alive exactly as long as this Signal.
    owned_subscriptions: Mutex<Vec<Arc<dyn Fn() + Send + Sync>>>,
}

pub struct Signal<T> {
    inner: Arc<SignalInner<T>>,
}
```

`Signal::clone` shares `inner` (same as today) — all clones share one
subscriber list and one `owned_subscriptions` list.

`Arc<Mutex<...>>` keeps `Signal` `Send + Sync` (matching the existing
`on_change` constraint).

### 2. Subscription lifecycle (weak-based, owner-driven)

Subscriptions are **never explicitly cleared.** Lifecycle is governed by
ownership of the closure, not by an unmount hook:

| Subscriber type | Strong-ref owner | Removed when |
|---|---|---|
| `ctx.signal_value` | StatefulElement's `dirty_callback: Arc<...>` | Element unmounts → dirty_callback dropped → weak dies |
| `Signal::derive` | Derived Signal's `owned_subscriptions` | Derived dropped → strong dropped → parent's weak dies |

On `Signal::set`, iterate `subscribers`, `upgrade()` each `Weak` — dead ones
(`None` from `upgrade`) are skipped. Dead weaks are not compacted in this
design; growth is bounded by the number of distinct elements that ever read
the Signal (tiny for a chat app). A future optimization may filter dead
weaks on `set`, but it is not required for correctness.

This is the same pattern as `InheritedRegistry` (sticky dependents, never
cleared per-rebuild), except weak refs make it leak-free.

### 3. New Signal methods

```rust
impl<T: PartialEq + Clone + Send + Sync + 'static> Signal<T> {
    /// Register a subscriber. Called by `ctx.signal_value` and `Signal::derive`.
    /// The closure is stored as `Weak`; the caller must hold a strong ref
    /// somewhere (element dirty_callback or derived's owned_subscriptions).
    pub fn add_subscriber(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.inner.subscribers.lock().unwrap()
            .push(Arc::downgrade(&callback));
    }

    /// Create a derived Signal that auto-updates from a parent Signal via
    /// `selector`. The derived subscribes to the parent; on parent `set`,
    /// the selector re-runs and `set_from` is called (no-op if unchanged,
    /// so the derived's own subscribers are not notified for irrelevant
    /// parent changes).
    pub fn derive<P, F>(parent: Signal<P>, selector: F) -> Signal<T>
    where
        P: PartialEq + Clone + Send + Sync + 'static,
        F: Fn(&P) -> T + Send + Sync + 'static,
    {
        let derived = Signal::new(selector(&parent.get_cloned()));
        let weak_inner = Arc::downgrade(&derived.inner);
        let closure: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(inner) = weak_inner.upgrade() {
                inner.set_from(&selector(&parent.get_cloned()));
            }
        });
        parent.add_subscriber(closure.clone());
        derived.inner.owned_subscriptions.lock().unwrap().push(closure);
        derived
    }
}
```

### 4. `set` / `set_from` notify subscribers

After the existing `on_change()` call (owning element), iterate subscribers:

```rust
pub fn set_from(&self, value: &T) {
    if self.inner.value.replace_if_changed(value) {
        if let Some(on_change) = &self.inner.on_change {
            on_change();
        }
        let subs = self.inner.subscribers.lock().unwrap();
        for weak in subs.iter() {
            if let Some(cb) = weak.upgrade() {
                cb();
            }
        }
    }
}
```

The existing `old != value` check gates both `on_change` and subscriber
notification — no work if the value didn't actually change.

### 5. RenderContext::signal_value

New method on `RenderContext`:

```rust
impl<'a> RenderContext<'a> {
    /// Read a Signal's value and register a dependency: when the Signal
    /// changes, this element is marked dirty and rebuilds.
    ///
    /// The element's dirty_callback (held by StatefulElement) is the strong
    /// ref that keeps the subscription alive; when the element unmounts,
    /// the dirty_callback drops and the subscription dies.
    pub fn signal_value<T: PartialEq + Clone + Send + Sync + 'static>(
        &mut self,
        signal: &Signal<T>,
    ) -> T {
        let value = signal.get_cloned();
        signal.add_subscriber(self.dirty_callback.clone());
        value
    }
}
```

`RenderContext` gains a `dirty_callback: Arc<dyn Fn() + Send + Sync>` field,
constructed in `StatefulElement::rebuild_from_state` (the same callback
already passed to `LifecycleContext::dirty_callback()`). This is a small
additive change to `RenderContext::new`.

### 6. ChatScreen simplification

**Before** (current workaround):

```rust
struct ChatScreen {
    conv_id: ConvId,
    messages: Vec<Message>,                        // snapshot for should_rebuild
    messages_reader: Rc<dyn Fn() -> Vec<Message>>, // live data via closure
    avatar_bytes: Rc<[u8]>,
    me_avatar_bytes: Rc<[u8]>,
    on_send: Rc<dyn Fn(&str)>,
    scroll_controller: ScrollController,
}

fn should_rebuild(&self, old: &Self) -> bool {
    self.conv_id != old.conv_id
        || self.messages.len() != old.messages.len()
        || self.messages != old.messages
}

fn render(&self, state: &mut State, ctx: &mut RenderContext) -> Box<dyn Widget> {
    let messages = (self.messages_reader)();
    // ...
}
```

**After:**

```rust
struct ChatScreen {
    conv_id: ConvId,
    /// Derived signal: this conversation's messages only. Subscribes to the
    /// root messages Signal; re-evaluates on parent set, but only notifies
    /// ChatScreen if THIS conversation's slice changed.
    messages: Signal<Vec<Message>>,
    avatar_bytes: Rc<[u8]>,
    me_avatar_bytes: Rc<[u8]>,
    on_send: Rc<dyn Fn(&str)>,
    scroll_controller: ScrollController,
}

fn should_rebuild(&self, old: &Self) -> bool {
    self.conv_id != old.conv_id
}

fn render(&self, state: &mut State, ctx: &mut RenderContext) -> Box<dyn Widget> {
    let messages = ctx.signal_value(&self.messages);
    // ...
}
```

Both `messages` (snapshot) and `messages_reader` (closure) fields are
**removed.** One field, one source of truth. `should_rebuild()` simplifies
to `conv_id` only — the Signal subscription handles message-change detection.

### 7. Wiring in chats/mod.rs and chats/desktop.rs

Construct a derived Signal per ChatScreen instance:

```rust
// chats/mod.rs
let msgs_for_conv = Signal::derive(msgs.clone(), move |map| {
    map.get(&id).cloned().unwrap_or_default()
});
ChatScreen {
    conv_id: id,
    messages: msgs_for_conv,
    // ...
}
```

`msgs` is the root `Signal<HashMap<ConvId, Vec<Message>>>` from `ImState`.
The derived Signal captures `id` (the conversation) and selects only that
conversation's message list.

### 8. Notification flow (end-to-end)

User taps Send:
1. `on_send(&text)` → `Signal::set_from(&map)` on root messages Signal.
2. Root Signal's `set_from`:
   - `old != value` → true → proceed.
   - `on_change()` → root element marked dirty (existing, derive-macro-wired).
   - Iterate subscribers → derived Signal's closure fires.
3. Derived closure: `weak_inner.upgrade()` → `set_from(&selector(&map))`.
   - `selector` extracts this conv's `Vec<Message>`.
   - `old != new` → true (new message added) → proceed.
   - `on_change()` → none (derived has no `on_change`; it's not an owning
     field of a `ComponentState`).
   - Iterate subscribers → ChatScreen's dirty_callback fires.
4. ChatScreen marked dirty → `rebuild_from_state()` → `render()` →
   `ctx.signal_value(&self.messages)` re-reads → fresh message list.
5. New message bubble renders. No keyboard dismiss needed.

Other conversation's messages change (e.g. incoming message while viewing
a different chat):
1. Root `set_from(&map)`.
2. Derived closure fires → `selector` extracts this conv's slice →
   `old == new` (this conv didn't change) → `set_from` is a no-op.
3. Derived's subscribers NOT notified → ChatScreen does NOT rebuild.

### 9. What stays unchanged

- `set_dirty_callback` on Signal — derive macro still wires it for the
  owning element. Read-tracking is a separate notification path.
- `TextEditingController`'s dirty callback — separate mechanism, untouched.
- `InheritedWidget` (Theme, MediaQuery) — separate mechanism, untouched.
- `Application::view(state)` signature — root still reads Signals directly
  via `state.is_dark.get()`; the derive-macro-wired `on_change` handles
  root rebuilds. No `ctx` parameter added to `view()`.
- Keyboard animation optimization — `should_rebuild()` still blocks the
  parent-cascade; Signal subscriptions only fire on actual message changes,
  not keyboard frames.

## Testing

### Unit tests (vexo/src/reactive/mod.rs)

1. `signal_add_subscriber_notified_on_set` — add a subscriber, `set`,
   subscriber closure fires.
2. `signal_subscriber_weak_dies_when_strong_dropped` — drop the strong ref,
   `set`, subscriber is skipped (no panic, no call).
3. `signal_set_noop_does_not_notify` — `set_from` with equal value, neither
   `on_change` nor subscribers fire.
4. `signal_derive_updates_when_parent_changes` — derive from parent, change
   parent, derived value updates.
5. `signal_derive_noop_when_slice_unchanged` — derive selecting a slice,
   change parent in a way that doesn't affect the slice, derived's
   subscribers NOT notified.
6. `signal_derive_no_leak_when_dropped` — derive, drop derived, parent's
   subscriber list weak is dead (verified by `set` not panicking and not
   holding the derived alive).
7. `signal_clone_shares_subscribers` — clone Signal, add subscriber via
   one clone, `set` via the other, subscriber fires.

### Unit tests (vexo/src/stateful_widget.rs)

8. `render_context_signal_value_registers_dependency` — component reads
   Signal via `ctx.signal_value`, Signal `set` triggers component rebuild
   (state-driven, bypasses `should_rebuild`).

### Integration tests (shared_app/src/chats/chat_screen.rs)

9. `chat_screen_reads_live_messages_from_signal` — ChatScreen with a derived
   Signal; send a message via the root Signal; assert ChatScreen rebuilds
   and renders the new message. Replaces the current
   `test_chat_screen_reads_live_messages_from_reader`.

## Open questions

None. All design decisions resolved during brainstorming:
- Read API: explicit `ctx.signal_value(&sig)`.
- Coexist with `set_dirty_callback`.
- Weak-ref subscriptions (not sticky).
- `owned_subscriptions` anchors derive closures.
- Whole-map Signal + `Signal::derive` for per-conversation granularity.

## Files affected

- `vexo/src/reactive/mod.rs` — `SignalInner`, `add_subscriber`, `derive`,
  `set`/`set_from` subscriber notification, tests.
- `vexo/src/stateful_widget.rs` — `RenderContext` gains `dirty_callback`
  field + `signal_value` method.
- `vexo/src/component_state_derive/src/lib.rs` — no change (still wires
  `set_dirty_callback` for owning fields).
- `shared_app/src/chats/chat_screen.rs` — remove `messages` snapshot +
  `messages_reader`; add `messages: Signal<Vec<Message>>`; simplify
  `should_rebuild`; `render` uses `ctx.signal_value`.
- `shared_app/src/chats/mod.rs` — construct derived Signal, pass to
  ChatScreen.
- `shared_app/src/chats/desktop.rs` — same.
