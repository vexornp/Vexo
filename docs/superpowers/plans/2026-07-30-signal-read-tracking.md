# Signal Read-Tracking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Signal reads in `render()` auto-register a dependency so reader elements rebuild when the Signal changes; add `Signal::derive` for slice-subscription; eliminate the `messages_reader` workaround in ChatScreen.

**Architecture:** `Signal<T>` is restructured to hold `Arc<SignalInner<T>>` where `SignalInner` contains the value (`Mutable<T>`), the owning-element callback (`Mutex<Option<Arc<dyn Fn()>>>`), a subscriber list (`Mutex<Vec<Weak<dyn Fn()>>>`), and owned subscriptions (`Mutex<Vec<Arc<dyn Fn()>>>`). `Signal::set` notifies the owner and all subscribers. `RenderContext::signal_value(&sig)` reads the value and registers the calling element's dirty_callback as a subscriber. `Signal::derive(parent, selector)` creates a derived Signal that auto-updates from a parent; the derive closure is anchored by the derived's `owned_subscriptions` and captures a `Weak` to the derived inner (no reference cycle).

**Tech Stack:** Rust, `futures-signals` `Mutable<T>`, `std::sync::{Arc, Mutex, Weak}`.

## Global Constraints

- `Signal<T>` must remain `Send + Sync` (all mutable state behind `Mutex`).
- `set_dirty_callback` mechanism stays (derive macro still wires it for owning fields).
- `Application::view(state)` signature unchanged (no `ctx` parameter).
- `InheritedWidget` (Theme, MediaQuery) untouched.
- TDD: write failing test first, run it to confirm failure, then implement.
- After every task: `cargo build` for affected crates must succeed; `cargo test` for affected tests must pass before commit.
- No comments added to code unless explicitly shown in the plan.

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `vexo/src/reactive/mod.rs` | `Signal<T>`: `SignalInner`, subscriber list, `add_subscriber`, `derive`, notify in `set`/`set_from` | Modify |
| `vexo/src/stateful_widget.rs` | `RenderContext`: add `dirty_callback` field + `signal_value` method; thread dirty_callback through `build_child_widget` | Modify |
| `shared_app/src/chats/chat_screen.rs` | Replace `messages`+`messages_reader` with `messages: Signal<Vec<Message>>`; simplify `should_rebuild`; `render` uses `ctx.signal_value` | Modify |
| `shared_app/src/chats/mod.rs` | Construct derived Signal, pass to ChatScreen | Modify |
| `shared_app/src/chats/desktop.rs` | Same as mod.rs | Modify |

---

### Task 1: Restructure Signal with subscriber list and notify-on-set

**Files:**
- Modify: `vexo/src/reactive/mod.rs`
- Test: `vexo/src/reactive/mod.rs` (new `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `Signal::add_subscriber(&self, callback: Arc<dyn Fn() + Send + Sync>)` — registers a weak to `callback` in the subscriber list. `Signal::set`/`set_from` notify both `on_change` and subscribers when the value changes.

- [ ] **Step 1: Write the failing tests**

Add a tests module at the end of `vexo/src/reactive/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn signal_add_subscriber_notified_on_set() {
        let sig = Signal::new(0u32);
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });
        sig.add_subscriber(cb);
        sig.set(1);
        assert_eq!(count.load(Ordering::SeqCst), 1, "subscriber should fire on set");
    }

    #[test]
    fn signal_subscriber_weak_dies_when_strong_dropped() {
        let sig = Signal::new(0u32);
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });
        sig.add_subscriber(cb);
        drop(cb);
        sig.set(1);
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "subscriber should not fire after its strong ref dropped"
        );
    }

    #[test]
    fn signal_set_noop_does_not_notify() {
        let sig = Signal::new(5u32);
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });
        sig.add_subscriber(cb);
        sig.set(5);
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "set with equal value should not notify subscribers"
        );
    }

    #[test]
    fn signal_clone_shares_subscribers() {
        let sig = Signal::new(0u32);
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });
        let clone = sig.clone();
        sig.add_subscriber(cb);
        clone.set(1);
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "subscriber added via one clone should fire when set via another clone"
        );
    }

    #[test]
    fn signal_set_dirty_callback_works_after_clone() {
        let mut sig = Signal::new(0u32);
        let clone = sig.clone();
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        sig.set_dirty_callback(Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        }));
        clone.set(1);
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "on_change set via one clone should fire when set via another clone"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib reactive::tests 2>&1 | tail -15`
Expected: FAIL — `add_subscriber` method not found.

- [ ] **Step 3: Implement the restructured Signal**

Replace the entire contents of `vexo/src/reactive/mod.rs` (from line 1 to the end) with:

```rust
//! Reactive state primitives for the Vexo framework.
//!
//! Provides `Signal<T>` which bridges futures-signals `Mutable<T>`
//! with the Vexo BuildOwner for automatic dirty marking when state changes.
//!
//! In addition to the owning-element notification (`on_change` / `set_dirty_callback`),
//! `Signal` supports read-tracking: `RenderContext::signal_value` registers the
//! reader element as a subscriber, and `Signal::set` notifies all subscribers.
//! `Signal::derive` creates a derived Signal that auto-updates from a parent.

pub use futures_signals::signal::{Mutable, ReadOnlyMutable, SignalExt};

use std::sync::{Arc, Mutex, Weak};

struct SignalInner<T> {
    value: Mutable<T>,
    on_change: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    subscribers: Mutex<Vec<Weak<dyn Fn() + Send + Sync>>>,
    owned_subscriptions: Mutex<Vec<Arc<dyn Fn() + Send + Sync>>>,
}

impl<T> SignalInner<T> {
    fn notify(&self) {
        let on_change = self.on_change.lock().unwrap().clone();
        if let Some(callback) = on_change {
            callback();
        }
        let subs = self.subscribers.lock().unwrap();
        for weak in subs.iter() {
            if let Some(cb) = weak.upgrade() {
                cb();
            }
        }
    }
}

pub struct Signal<T> {
    inner: Arc<SignalInner<T>>,
}

impl<T> Signal<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(SignalInner {
                value: Mutable::new(value),
                on_change: Mutex::new(None),
                subscribers: Mutex::new(Vec::new()),
                owned_subscriptions: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.inner.on_change.lock().unwrap() = Some(callback);
    }

    pub fn read_only(&self) -> ReadOnlyMutable<T> {
        self.inner.value.read_only()
    }

    pub fn add_subscriber(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.inner
            .subscribers
            .lock()
            .unwrap()
            .push(Arc::downgrade(&callback));
    }
}

impl<T: PartialEq + Copy> Signal<T> {
    pub fn get(&self) -> T {
        self.inner.value.get()
    }

    pub fn set(&self, value: T) {
        let old = self.inner.value.get();
        self.inner.value.set(value);
        if old != value {
            self.inner.notify();
        }
    }
}

impl<T: PartialEq + Clone> Signal<T> {
    pub fn get_cloned(&self) -> T {
        self.inner.value.get_cloned()
    }

    pub fn set_from(&self, value: &T) {
        let old = self.inner.value.get_cloned();
        self.inner.value.set(value.clone());
        if old != *value {
            self.inner.notify();
        }
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Default> Default for Signal<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib reactive::tests 2>&1 | tail -15`
Expected: all 5 tests PASS.

- [ ] **Step 5: Run full vexo test suite to confirm no regressions**

Run: `cargo test -p vexo --lib 2>&1 | tail -10`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/reactive/mod.rs
git commit -m "feat(signal): restructure to SignalInner with subscriber list"
```

---

### Task 2: Add `Signal::derive`

**Files:**
- Modify: `vexo/src/reactive/mod.rs`

**Interfaces:**
- Consumes: `add_subscriber`, `SignalInner`, `notify` (Task 1).
- Produces: `Signal::derive<P, F>(parent: Signal<P>, selector: F) -> Signal<T>` where `P: PartialEq + Clone + Send + Sync + 'static`, `F: Fn(&P) -> T + Send + Sync + 'static`.

- [ ] **Step 1: Write the failing tests**

Add these tests to the `tests` module in `vexo/src/reactive/mod.rs`:

```rust
    #[test]
    fn signal_derive_updates_when_parent_changes() {
        let parent = Signal::new(10u32);
        let derived = Signal::derive(parent.clone(), |p| *p + 1);
        assert_eq!(derived.get(), 11);
        parent.set(20);
        assert_eq!(derived.get(), 21, "derived should update when parent changes");
    }

    #[test]
    fn signal_derive_noop_when_slice_unchanged() {
        #[derive(PartialEq, Clone, Debug)]
        struct Data { a: u32, b: u32 }
        let parent = Signal::new(Data { a: 1, b: 100 });
        let derived = Signal::derive(parent.clone(), |p| p.b);
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        derived.add_subscriber(Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        }));
        parent.set(Data { a: 2, b: 100 });
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "derived subscribers should NOT fire when selected slice unchanged"
        );
        parent.set(Data { a: 2, b: 200 });
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "derived subscribers should fire when selected slice changes"
        );
    }

    #[test]
    fn signal_derive_no_leak_when_dropped() {
        let parent = Signal::new(10u32);
        let derived = Signal::derive(parent.clone(), |p| *p + 1);
        drop(derived);
        parent.set(20);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib reactive::tests::signal_derive 2>&1 | tail -15`
Expected: FAIL — `derive` method not found.

- [ ] **Step 3: Implement `Signal::derive`**

Add this impl block to `vexo/src/reactive/mod.rs`, after the existing `impl<T> Signal<T>` block (before the `impl<T: PartialEq + Copy>` block):

```rust
impl<T: PartialEq + Clone + Send + Sync + 'static> Signal<T> {
    pub fn derive<P, F>(parent: Signal<P>, selector: F) -> Signal<T>
    where
        P: PartialEq + Clone + Send + Sync + 'static,
        F: Fn(&P) -> T + Send + Sync + 'static,
    {
        let derived = Signal::new(selector(&parent.get_cloned()));
        let weak_inner = Arc::downgrade(&derived.inner);
        let closure: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let inner = weak_inner.upgrade();
            let inner = match inner {
                Some(i) => i,
                None => return,
            };
            let new_val = selector(&parent.get_cloned());
            let old_val = inner.value.get_cloned();
            if old_val != new_val {
                inner.value.set(new_val);
                inner.notify();
            }
        });
        parent.add_subscriber(closure.clone());
        derived
            .inner
            .owned_subscriptions
            .lock()
            .unwrap()
            .push(closure);
        derived
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib reactive::tests 2>&1 | tail -15`
Expected: all 8 tests PASS.

- [ ] **Step 5: Run full vexo test suite**

Run: `cargo test -p vexo --lib 2>&1 | tail -10`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/reactive/mod.rs
git commit -m "feat(signal): add Signal::derive for slice-subscription"
```

---

### Task 3: Add `dirty_callback` field + `signal_value` to RenderContext

**Files:**
- Modify: `vexo/src/stateful_widget.rs`
- Modify: `vexo/src/widgets/memo.rs` (test call sites only)

**Interfaces:**
- Consumes: `Signal::add_subscriber`, `Signal::get_cloned` (Tasks 1, 2).
- Produces: `RenderContext::signal_value<T>(&mut self, &Signal<T>) -> T` — reads value, registers the element's dirty_callback as a subscriber.

- [ ] **Step 1: Add `dirty_callback` field to `RenderContext` and update `new`**

In `vexo/src/stateful_widget.rs`, find the `pub struct RenderContext<'a> {` block (around line 296) and add a `dirty_callback` field. Replace:

```rust
pub struct RenderContext<'a> {
    element_id: ElementKey,
    build_owner: &'a BuildOwner,
    inherited_map: &'a InheritedMap,
    inherited_registry: &'a InheritedRegistry,
}
```

with:

```rust
pub struct RenderContext<'a> {
    element_id: ElementKey,
    build_owner: &'a BuildOwner,
    inherited_map: &'a InheritedMap,
    inherited_registry: &'a InheritedRegistry,
    dirty_callback: Arc<dyn Fn() + Send + Sync>,
}
```

Then update `RenderContext::new` (around line 328). Replace:

```rust
    pub fn new(
        element_id: ElementKey,
        build_owner: &'a BuildOwner,
        inherited_map: &'a InheritedMap,
        inherited_registry: &'a InheritedRegistry,
    ) -> Self {
        Self {
            element_id,
            build_owner,
            inherited_map,
            inherited_registry,
        }
    }
```

with:

```rust
    pub fn new(
        element_id: ElementKey,
        build_owner: &'a BuildOwner,
        inherited_map: &'a InheritedMap,
        inherited_registry: &'a InheritedRegistry,
        dirty_callback: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            element_id,
            build_owner,
            inherited_map,
            inherited_registry,
            dirty_callback,
        }
    }
```

- [ ] **Step 2: Add `signal_value` method to `RenderContext`**

Add `use crate::reactive::Signal;` to the imports at the top of `vexo/src/stateful_widget.rs` if not already present. Then add this method to the `impl<'a> RenderContext<'a>` block, after the `depend_on_inherited_widget` method (around line 370):

```rust
    pub fn signal_value<T: PartialEq + Clone + Send + Sync + 'static>(
        &mut self,
        signal: &Signal<T>,
    ) -> T {
        let value = signal.get_cloned();
        signal.add_subscriber(self.dirty_callback.clone());
        value
    }
```

- [ ] **Step 3: Update `build_child_widget` to accept and pass `dirty_callback`**

In `vexo/src/stateful_widget.rs`, the `build_child_widget` method (around line 516). Replace:

```rust
    fn build_child_widget(
        &self,
        element_id: ElementKey,
        state: &mut W::State,
        build_owner: &BuildOwner,
        inherited_map: &InheritedMap,
        inherited_registry: &InheritedRegistry,
    ) -> Box<dyn Widget> {
        let mut render_ctx =
            RenderContext::new(element_id, build_owner, inherited_map, inherited_registry);
        self.widget.render(state, &mut render_ctx)
    }
```

with:

```rust
    fn build_child_widget(
        &self,
        element_id: ElementKey,
        state: &mut W::State,
        build_owner: &BuildOwner,
        inherited_map: &InheritedMap,
        inherited_registry: &InheritedRegistry,
        dirty_callback: Arc<dyn Fn() + Send + Sync>,
    ) -> Box<dyn Widget> {
        let mut render_ctx = RenderContext::new(
            element_id,
            build_owner,
            inherited_map,
            inherited_registry,
            dirty_callback,
        );
        self.widget.render(state, &mut render_ctx)
    }
```

- [ ] **Step 4: Update the `mount` call site of `build_child_widget`**

In `vexo/src/stateful_widget.rs`, the `mount` method (around line 569). The dirty_callback is constructed at line 593 as `let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || { let _ = tx.send(element_id); });`. It's used at line 596 by `state.set_dirty_callback(dirty_callback.clone())`. The original binding is still available.

Find the `build_child_widget` call in `mount` (around line 625-634). Replace:

```rust
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
            )
        };
```

with:

```rust
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
                dirty_callback,
            )
        };
```

- [ ] **Step 5: Update the `update` call site — hoist dirty_callback**

In `vexo/src/stateful_widget.rs`, the `update` method (around line 640). The dirty_callback is currently constructed inside a block (lines 659-672) and not accessible outside. Hoist it. Replace the block starting at line 656 through line 672:

```rust
        {
            let tx = context.dirty_sender.clone();
            let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let _ = tx.send(element_id);
            });
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let mut lifecycle_ctx = LifecycleContext::new(
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.on_update(&old_widget as &dyn Any, &mut lifecycle_ctx);
        }
```

with:

```rust
        let dirty_callback: Arc<dyn Fn() + Send + Sync> = {
            let tx = context.dirty_sender.clone();
            Arc::new(move || {
                let _ = tx.send(element_id);
            })
        };
        {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let mut lifecycle_ctx = LifecycleContext::new(
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback.clone(),
                context.animation_ticker.clone(),
            );
            state_ref.on_update(&old_widget as &dyn Any, &mut lifecycle_ctx);
        }
```

Then find the `build_child_widget` call in `update` (around line 687-696). Replace:

```rust
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
            )
        };
```

with:

```rust
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
                dirty_callback,
            )
        };
```

- [ ] **Step 6: Update the `rebuild_from_state` call site — hoist dirty_callback**

In `vexo/src/stateful_widget.rs`, the `rebuild_from_state` method (around line 771). Same hoist pattern. Replace the block starting at line 778 through line 793:

```rust
        {
            let tx = context.dirty_sender.clone();
            let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let _ = tx.send(element_id);
            });
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let mut lifecycle_ctx = LifecycleContext::new(
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback,
                context.animation_ticker.clone(),
            );
            state_ref.on_rebuild(&mut lifecycle_ctx);
        }
```

with:

```rust
        let dirty_callback: Arc<dyn Fn() + Send + Sync> = {
            let tx = context.dirty_sender.clone();
            Arc::new(move || {
                let _ = tx.send(element_id);
            })
        };
        {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            let mut lifecycle_ctx = LifecycleContext::new(
                context.build_owner,
                &self.widget as &dyn Any,
                dirty_callback.clone(),
                context.animation_ticker.clone(),
            );
            state_ref.on_rebuild(&mut lifecycle_ctx);
        }
```

Then find the `build_child_widget` call in `rebuild_from_state` (around line 796-805). Replace:

```rust
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
            )
        };
```

with:

```rust
        let child_widget = {
            let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
            self.build_child_widget(
                element_id,
                state_ref,
                context.build_owner,
                context.inherited_map,
                context.inherited_registry,
                dirty_callback,
            )
        };
```

- [ ] **Step 7: Update all test call sites of `RenderContext::new`**

Run: `rg "RenderContext::new" vexo/src -n`

For each call site found (in `vexo/src/stateful_widget.rs` tests around lines 1364, 1369, 1374, 1416, 1430, 1447 and in `vexo/src/widgets/memo.rs` around lines 257, 283), add a final `std::sync::Arc::new(|| {})` argument.

Example transformation — before:
```rust
        let ctx = RenderContext::new(element_id, &build_owner, &empty_map, &inherited_registry);
```
After:
```rust
        let ctx = RenderContext::new(
            element_id,
            &build_owner,
            &empty_map,
            &inherited_registry,
            std::sync::Arc::new(|| {}),
        );
```

Apply to every call site found by the rg command above. Read each file and confirm exact line numbers before editing.

- [ ] **Step 8: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -15`
Expected: compiles successfully (warnings ok, errors not ok).

- [ ] **Step 9: Run full vexo test suite**

Run: `cargo test -p vexo --lib 2>&1 | tail -15`
Expected: all pass.

- [ ] **Step 10: Commit**

```bash
git add vexo/src/stateful_widget.rs vexo/src/widgets/memo.rs
git commit -m "feat(render-context): add signal_value for read-tracking"
```

---

### Task 4: Integration test — component rebuilds when read Signal changes

**Files:**
- Modify: `vexo/src/stateful_widget.rs` (tests module at the end of the file)

**Interfaces:**
- Consumes: `RenderContext::signal_value`, `Signal::set`, `Signal::add_subscriber` (Tasks 1, 3).

- [ ] **Step 1: Check pipeline's public rebuild API**

Run: `rg "pub fn (perform_rebuilds|drain_rebuilds|process_rebuilds)" vexo/src/pipeline.rs -n`

Note the exact method name for draining the rebuild queue — the test will call it after `signal.set()` to process the dirty marks.

- [ ] **Step 2: Write the failing integration test**

Add this test to the `tests` module at the bottom of `vexo/src/stateful_widget.rs`:

```rust
    #[test]
    fn test_signal_value_registers_dependency_and_rebuilds() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::layout::TaffyLayoutEngine;
        use vexo::ThreeTreePipeline;

        let external_signal = vexo::Signal::new(0u32);

        #[derive(Clone)]
        struct Reader {
            signal: vexo::Signal<u32>,
        }
        #[derive(Default)]
        struct ReaderState;
        impl vexo::ComponentState for ReaderState {}
        impl vexo::Component for Reader {
            type State = ReaderState;
            fn render(
                &self,
                _state: &mut Self::State,
                ctx: &mut vexo::RenderContext,
            ) -> Box<dyn vexo::Widget> {
                let val = ctx.signal_value(&self.signal);
                vexo::Text::new(format!("val={}", val)).boxed()
            }
        }

        let view = Reader {
            signal: external_signal.clone(),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);

        external_signal.set(42);
        pipeline.perform_rebuilds(&mut TaffyLayoutEngine::new());

        let ro_reg = pipeline.render_objects();
        let text_count = ro_reg
            .iter()
            .filter(|(_, ro)| {
                ro.as_any()
                    .downcast_ref::<vexo::render_objects::TextRenderObject>()
                    .map_or(false, |t| t.content().contains("val=42"))
            })
            .count();
        assert_eq!(
            text_count, 1,
            "Reader should rebuild with val=42 after signal set"
        );
    }
```

If `perform_rebuilds` is not the correct method name (check Step 1), replace it with the correct name. If `TextRenderObject` is not accessible at that path, check the correct path with `rg "pub struct TextRenderObject" vexo/src -n` and adjust.

- [ ] **Step 3: Run test to verify it fails or passes**

Run: `cargo test -p vexo --lib test_signal_value_registers_dependency_and_rebuilds 2>&1 | tail -20`

If it FAILS to compile: fix the import paths or method names based on the actual API.
If it FAILS at runtime: the subscription or rebuild isn't working — debug by checking that `signal_value` is calling `add_subscriber` and that `perform_rebuilds` drains the dirty queue.
If it PASSES: implementation from Task 3 is correct.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/stateful_widget.rs
git commit -m "test(stateful): integration test for signal read-tracking rebuild"
```

---

### Task 5: Simplify ChatScreen to use derived Signal

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs`

**Interfaces:**
- Consumes: `Signal::derive`, `RenderContext::signal_value` (Tasks 2, 3).
- Produces: `ChatScreen { conv_id: ConvId, messages: Signal<Vec<Message>>, ... }` — no more `messages_reader` or snapshot `messages`.

- [ ] **Step 1: Update the ChatScreen struct + Clone impl**

In `shared_app/src/chats/chat_screen.rs`, replace the struct + Clone impl (lines 16-36):

```rust
pub(crate) struct ChatScreen {
    pub(crate) conv_id: ConvId,
    pub(crate) messages: Vec<Message>,
    pub(crate) messages_reader: Rc<dyn Fn() -> Vec<Message>>,
    pub(crate) avatar_bytes: Rc<[u8]>,
    pub(crate) me_avatar_bytes: Rc<[u8]>,
    pub(crate) on_send: Rc<dyn Fn(&str)>,
    pub(crate) scroll_controller: ScrollController,
}

impl Clone for ChatScreen {
    fn clone(&self) -> Self {
        Self {
            conv_id: self.conv_id.clone(),
            messages: self.messages.clone(),
            messages_reader: Rc::clone(&self.messages_reader),
            avatar_bytes: Rc::clone(&self.avatar_bytes),
            me_avatar_bytes: Rc::clone(&self.me_avatar_bytes),
            on_send: Rc::clone(&self.on_send),
            scroll_controller: self.scroll_controller.clone(),
        }
    }
}
```

with:

```rust
pub(crate) struct ChatScreen {
    pub(crate) conv_id: ConvId,
    pub(crate) messages: Signal<Vec<Message>>,
    pub(crate) avatar_bytes: Rc<[u8]>,
    pub(crate) me_avatar_bytes: Rc<[u8]>,
    pub(crate) on_send: Rc<dyn Fn(&str)>,
    pub(crate) scroll_controller: ScrollController,
}

impl Clone for ChatScreen {
    fn clone(&self) -> Self {
        Self {
            conv_id: self.conv_id.clone(),
            messages: self.messages.clone(),
            avatar_bytes: Rc::clone(&self.avatar_bytes),
            me_avatar_bytes: Rc::clone(&self.me_avatar_bytes),
            on_send: Rc::clone(&self.on_send),
            scroll_controller: self.scroll_controller.clone(),
        }
    }
}
```

- [ ] **Step 2: Update imports**

Replace lines 6-10:

```rust
use vexo::{
    children, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox, FlexDirection,
    Image, ImageData, Key, Layout, LifecycleContext, MultiChild, RenderContext, ScrollController,
    ScrollView, Style, Text, TextEdit, TextEditingController, Theme, Widget, WidgetKey, WithLayout,
};
```

with (add `Signal`):

```rust
use vexo::{
    children, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox, FlexDirection,
    Image, ImageData, Key, Layout, LifecycleContext, MultiChild, RenderContext, ScrollController,
    ScrollView, Signal, Style, Text, TextEdit, TextEditingController, Theme, Widget, WidgetKey,
    WithLayout,
};
```

- [ ] **Step 3: Simplify `should_rebuild` and `render`**

Replace the `should_rebuild` + `render` methods (lines 99-166):

```rust
    fn should_rebuild(&self, old: &Self) -> bool {
        self.conv_id != old.conv_id
            || self.messages.len() != old.messages.len()
            || self.messages != old.messages
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);

        let messages = (self.messages_reader)();

        let mut list = MultiChild::empty(Layout::column().gap(8.0).padding(12.0));
        for msg in &messages {
            list = list.push(build_message_bubble(
                msg,
                state.them_avatar(&self.avatar_bytes).clone(),
                state.me_avatar(&self.me_avatar_bytes).clone(),
                &theme,
            ));
        }

        let scroll_for_send = self.scroll_controller.clone();
        let on_send = Rc::clone(&self.on_send);
        let tc = state
            .text_controller
            .as_ref()
            .expect("text controller set on mount")
            .clone();
        let tc_for_clear = tc.clone();
        let on_send_closure = move || {
            let text = tc_for_clear.text();
            if !text.trim().is_empty() {
                on_send(&text);
                let mut fs = vexo::resource::new_font_system();
                tc_for_clear.set_text("", &mut fs);
                scroll_for_send.jump_to_bottom();
            }
        };

        let input_bar = build_input_bar(tc, on_send_closure);

        let content = DecoratedBox::with_style(
            MultiChild::new(
                children![
                    WithLayout::new(
                        ScrollView::new(list.boxed()).controller(self.scroll_controller.clone()),
                        Layout::flex_fill(),
                    ),
                    input_bar,
                ],
                Layout::column()
                    .flex_grow(1.0)
                    .flex_basis(0.0)
                    .min_height(0.0),
            ),
            Style::default().background(theme.background),
        )
        .boxed();

        KeyboardAvoider::new(content).boxed()
    }
```

with:

```rust
    fn should_rebuild(&self, old: &Self) -> bool {
        self.conv_id != old.conv_id
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);

        let messages = ctx.signal_value(&self.messages);

        let mut list = MultiChild::empty(Layout::column().gap(8.0).padding(12.0));
        for msg in &messages {
            list = list.push(build_message_bubble(
                msg,
                state.them_avatar(&self.avatar_bytes).clone(),
                state.me_avatar(&self.me_avatar_bytes).clone(),
                &theme,
            ));
        }

        let scroll_for_send = self.scroll_controller.clone();
        let on_send = Rc::clone(&self.on_send);
        let tc = state
            .text_controller
            .as_ref()
            .expect("text controller set on mount")
            .clone();
        let tc_for_clear = tc.clone();
        let on_send_closure = move || {
            let text = tc_for_clear.text();
            if !text.trim().is_empty() {
                on_send(&text);
                let mut fs = vexo::resource::new_font_system();
                tc_for_clear.set_text("", &mut fs);
                scroll_for_send.jump_to_bottom();
            }
        };

        let input_bar = build_input_bar(tc, on_send_closure);

        let content = DecoratedBox::with_style(
            MultiChild::new(
                children![
                    WithLayout::new(
                        ScrollView::new(list.boxed()).controller(self.scroll_controller.clone()),
                        Layout::flex_fill(),
                    ),
                    input_bar,
                ],
                Layout::column()
                    .flex_grow(1.0)
                    .flex_basis(0.0)
                    .min_height(0.0),
            ),
            Style::default().background(theme.background),
        )
        .boxed();

        KeyboardAvoider::new(content).boxed()
    }
```

- [ ] **Step 4: Do not commit yet — proceed to Task 6**

---

### Task 6: Wire derived Signal in chats/mod.rs and chats/desktop.rs

**Files:**
- Modify: `shared_app/src/chats/mod.rs`
- Modify: `shared_app/src/chats/desktop.rs`

**Interfaces:**
- Consumes: `Signal::derive`, new `ChatScreen` struct (Task 5).

- [ ] **Step 1: Update `chats/mod.rs` construction**

In `shared_app/src/chats/mod.rs`, update the import on line 11 to add `Signal`:

```rust
use vexo::{Component, RenderContext, Signal, SimpleState, Text, Theme, Widget};
```

Then replace the destination closure body (lines 67-104). Replace:

```rust
            .destination(move |d| match d {
                ChatsRoute::Chat(id) => {
                    let m = msgs.get_cloned().get(id).cloned().unwrap_or_default();
                    let avatar = convs
                        .iter()
                        .find(|c| c.id == *id)
                        .map(|c| Rc::clone(&c.avatar_bytes))
                        .unwrap_or_else(|| Rc::from([0u8; 0]));
                    let msgs_for_send = msgs.clone();
                    let id_for_send = id.clone();
                    let msgs_for_reader = msgs.clone();
                    let id_for_reader = id.clone();
                    chat_screen::ChatScreen {
                        conv_id: id_for_send.clone(),
                        messages: m,
                        messages_reader: Rc::new(move || {
                            msgs_for_reader
                                .get_cloned()
                                .get(&id_for_reader)
                                .cloned()
                                .unwrap_or_default()
                        }),
                        avatar_bytes: avatar,
                        me_avatar_bytes: me_avatar_for_dest.clone(),
                        on_send: Rc::new(move |text: &str| {
                            let mut map = msgs_for_send.get_cloned();
                            if let Some(vec) = map.get_mut(&id_for_send) {
                                vec.push(Message {
                                    author: MessageAuthor::Me,
                                    text: text.to_string(),
                                    timestamp: 1732348000,
                                });
                            }
                            msgs_for_send.set_from(&map);
                        }),
                        scroll_controller: vexo::ScrollController::new(),
                    }
                    .boxed()
                }
                _ => Text::new("").boxed(),
            })
```

with:

```rust
            .destination(move |d| match d {
                ChatsRoute::Chat(id) => {
                    let avatar = convs
                        .iter()
                        .find(|c| c.id == *id)
                        .map(|c| Rc::clone(&c.avatar_bytes))
                        .unwrap_or_else(|| Rc::from([0u8; 0]));
                    let msgs_for_send = msgs.clone();
                    let id_for_send = id.clone();
                    let id_for_derive = id.clone();
                    let msgs_for_derive = msgs.clone();
                    let messages = Signal::derive(msgs_for_derive, move |map| {
                        map.get(&id_for_derive).cloned().unwrap_or_default()
                    });
                    chat_screen::ChatScreen {
                        conv_id: id_for_send.clone(),
                        messages,
                        avatar_bytes: avatar,
                        me_avatar_bytes: me_avatar_for_dest.clone(),
                        on_send: Rc::new(move |text: &str| {
                            let mut map = msgs_for_send.get_cloned();
                            if let Some(vec) = map.get_mut(&id_for_send) {
                                vec.push(Message {
                                    author: MessageAuthor::Me,
                                    text: text.to_string(),
                                    timestamp: 1732348000,
                                });
                            }
                            msgs_for_send.set_from(&map);
                        }),
                        scroll_controller: vexo::ScrollController::new(),
                    }
                    .boxed()
                }
                _ => Text::new("").boxed(),
            })
```

- [ ] **Step 2: Update `chats/desktop.rs` construction**

In `shared_app/src/chats/desktop.rs`, add `Signal` to the import on line 7:

```rust
use vexo::{
    children, AlignItems, Component, DecoratedBox, JustifyContent, Layout, MultiChild,
    RenderContext, ScrollController, Signal, SimpleState, Style, Text, Theme, Widget, WithLayout,
};
```

Then replace lines 66-113 (the `let col3 = match selected { ... }` block). Replace:

```rust
        let col3 = match selected {
            Some(id) => {
                let msgs = messages_map.get(&id).cloned().unwrap_or_default();
                let avatar = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| Rc::clone(&c.avatar_bytes))
                    .unwrap_or_else(|| Rc::from([0u8; 0]));
                let conv_name = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| format!("Chat {}", id.0));

                let msgs_for_send = self.messages.clone();
                let id_for_send = id.clone();
                let msgs_for_reader = self.messages.clone();
                let id_for_reader = id.clone();
                let chat = ChatScreen {
                    conv_id: id_for_send.clone(),
                    messages: msgs,
                    messages_reader: Rc::new(move || {
                        msgs_for_reader
                            .get_cloned()
                            .get(&id_for_reader)
                            .cloned()
                            .unwrap_or_default()
                    }),
                    avatar_bytes: avatar,
                    me_avatar_bytes: self.me_avatar.clone(),
                    on_send: Rc::new(move |text: &str| {
                        let mut map = msgs_for_send.get_cloned();
                        if let Some(vec) = map.get_mut(&id_for_send) {
                            vec.push(Message {
                                author: MessageAuthor::Me,
                                text: text.to_string(),
                                timestamp: 1732348000,
                            });
                        }
                        msgs_for_send.set_from(&map);
                    }),
                    scroll_controller: ScrollController::new(),
                };

                titled_container(conv_name, chat.boxed(), &nav_colors)
            }
            None => build_empty_placeholder(&nav_colors),
        };
```

with:

```rust
        let col3 = match selected {
            Some(id) => {
                let avatar = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| Rc::clone(&c.avatar_bytes))
                    .unwrap_or_else(|| Rc::from([0u8; 0]));
                let conv_name = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| format!("Chat {}", id.0));

                let msgs_for_send = self.messages.clone();
                let id_for_send = id.clone();
                let id_for_derive = id.clone();
                let msgs_for_derive = self.messages.clone();
                let messages = Signal::derive(msgs_for_derive, move |map| {
                    map.get(&id_for_derive).cloned().unwrap_or_default()
                });
                let chat = ChatScreen {
                    conv_id: id_for_send.clone(),
                    messages,
                    avatar_bytes: avatar,
                    me_avatar_bytes: self.me_avatar.clone(),
                    on_send: Rc::new(move |text: &str| {
                        let mut map = msgs_for_send.get_cloned();
                        if let Some(vec) = map.get_mut(&id_for_send) {
                            vec.push(Message {
                                author: MessageAuthor::Me,
                                text: text.to_string(),
                                timestamp: 1732348000,
                            });
                        }
                        msgs_for_send.set_from(&map);
                    }),
                    scroll_controller: ScrollController::new(),
                };

                titled_container(conv_name, chat.boxed(), &nav_colors)
            }
            None => build_empty_placeholder(&nav_colors),
        };
```

Also, the `messages_map` variable (line 48: `let messages_map = self.messages.get_cloned();`) is no longer used in the `Some(id)` arm. Check if it's used elsewhere in the render method; if not, remove it.

- [ ] **Step 3: Build shared_app to verify compilation**

Run: `cargo build -p shared_app 2>&1 | tail -15`
Expected: compiles successfully. If there are errors about unused `messages_map`, remove that line.

- [ ] **Step 4: Do not commit yet — proceed to Task 7**

---

### Task 7: Update ChatScreen tests

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (tests module)

**Interfaces:**
- Consumes: new `ChatScreen` struct (Task 5).

- [ ] **Step 1: Replace the entire tests module**

In `shared_app/src/chats/chat_screen.rs`, replace the entire `#[cfg(test)] mod tests` block (from line 248 to the end of the file) with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::layout::TaffyLayoutEngine;
    use vexo::{RenderObjectRegistry, Signal, ThreeTreePipeline};

    fn seed_messages_signal() -> Signal<std::collections::HashMap<ConvId, Vec<Message>>> {
        crate::data::seed().messages.clone()
    }

    fn seed_avatar(conv_id: ConvId) -> Rc<[u8]> {
        crate::data::seed()
            .conversations
            .iter()
            .find(|c| c.id == conv_id)
            .unwrap()
            .avatar_bytes
            .clone()
    }

    fn seed_me_avatar() -> Rc<[u8]> {
        crate::data::seed().profile.avatar_bytes.clone()
    }

    #[test]
    fn test_chat_screen_renders_messages() {
        let messages_signal = seed_messages_signal();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 3 messages + input bar"
        );
    }

    #[test]
    fn test_chat_screen_reads_live_messages_from_signal() {
        let messages_signal = seed_messages_signal();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 3 messages read from derived signal, got {}",
            pipeline.element_registry().len()
        );
    }

    #[test]
    fn test_chat_screen_input_bar_pinned_to_bottom_with_few_messages() {
        let empty_map: std::collections::HashMap<ConvId, Vec<Message>> =
            std::collections::HashMap::new();
        let empty_signal = Signal::new(empty_map);
        let chat = ChatScreen {
            conv_id: ConvId(4),
            messages: Signal::derive(empty_signal, |_| Vec::new()),
            avatar_bytes: seed_avatar(ConvId(4)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
        };

        let view = MultiChild::new(children![chat], Layout::column().height(600.0)).boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        fn find_child(
            ro_reg: &RenderObjectRegistry,
            id: vexo::RenderObjectKey,
            index: usize,
        ) -> Option<vexo::RenderObjectKey> {
            ro_reg.get(id)?.children().get(index).copied()
        }

        let proxy = find_child(ro_reg, root, 0).expect("proxy");
        let mut current = proxy;
        let chat_decorated = loop {
            let child = find_child(ro_reg, current, 0).expect("child of proxy");
            if let Some(grandchild) = find_child(ro_reg, child, 0) {
                if ro_reg
                    .get(grandchild)
                    .and_then(|ro| ro.children().len().into())
                    .unwrap_or(0)
                    >= 2
                {
                    break child;
                }
            }
            current = child;
        };
        let chat_col = find_child(ro_reg, chat_decorated, 0).expect("chat column");
        let input_wrapper = find_child(ro_reg, chat_col, 1).expect("input bar wrapper");
        let input_bounds = ro_reg
            .get(input_wrapper)
            .and_then(|ro| ro.computed_bounds())
            .expect("input bar bounds");

        let input_bottom = input_bounds.top + input_bounds.height();
        assert!(
            input_bottom >= 599.0,
            "input bar bottom ({}) should be at the view bottom (600). \
             Top={}, Height={}",
            input_bottom,
            input_bounds.top,
            input_bounds.height()
        );
    }
}
```

- [ ] **Step 2: Build and run shared_app tests**

Run: `cargo test -p shared_app 2>&1 | tail -20`
Expected: all tests PASS.

- [ ] **Step 3: Run full workspace test suite**

Run: `cargo test 2>&1 | grep "test result:"`
Expected: all test results show "ok" with 0 failures.

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs shared_app/src/chats/mod.rs shared_app/src/chats/desktop.rs
git commit -m "refactor(chat-screen): use derived Signal, remove messages_reader workaround"
```

---

### Task 8: Manual verification on iOS

**Files:** none (verification only)

- [ ] **Step 1: Build for iOS**

Run: `./build_for_ios.sh`
Expected: succeeds, generates Swift bindings.

- [ ] **Step 2: Ask the user to test on the iOS simulator**

Tell the user: "Please run the VexoDemo scheme from Xcode and verify:
1. Open a conversation.
2. Type a message and tap Send.
3. The message should appear immediately (no keyboard dismiss needed).
4. Send a second message — should also appear immediately, no crash.
5. Switch to a different conversation — its messages should show.
6. Send a message in the second conversation — should appear immediately."

- [ ] **Step 3: Do not commit — verification only**
