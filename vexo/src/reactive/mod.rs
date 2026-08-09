//! Reactive state primitives for the Vexo framework.
//!
//! Provides `Signal<T>` which bridges futures-signals `Mutable<T>`
//! with the Vexo BuildOwner for automatic dirty marking when state changes.
//!
//! In addition to the owning-element notification (`on_change` / `set_dirty_callback`),
//! `Signal` supports read-tracking: `RenderContext::depend_on_signal` registers the
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
        // Snapshot live subscribers under the lock, then drop the lock before
        // invoking callbacks. This prevents re-entrant `set`/`set_from` on the
        // same Signal (e.g. a derived-Signal closure that re-reads the parent)
        // from deadlocking — Rust's `Mutex` is non-reentrant. Dead weaks are
        // naturally filtered out since `Weak::upgrade` returns `None`.
        let live: Vec<Arc<dyn Fn() + Send + Sync>> = {
            let subs = self.subscribers.lock().unwrap();
            subs.iter().filter_map(|w| w.upgrade()).collect()
        };
        for cb in &live {
            cb();
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

    pub fn add_subscriber(&self, callback: &Arc<dyn Fn() + Send + Sync>) {
        self.inner
            .subscribers
            .lock()
            .unwrap()
            .push(Arc::downgrade(callback));
    }
}

impl<T: PartialEq + Clone + Send + Sync + 'static> Signal<T> {
    pub fn derive<P, F>(parent: Signal<P>, selector: F) -> Signal<T>
    where
        P: PartialEq + Clone + Send + Sync + 'static,
        F: Fn(&P) -> T + Send + Sync + 'static,
    {
        let derived = Signal::new(selector(&parent.get_cloned()));
        let weak_inner = Arc::downgrade(&derived.inner);
        let parent_for_closure = parent.clone();
        let closure: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let inner = weak_inner.upgrade();
            let inner = match inner {
                Some(i) => i,
                None => return,
            };
            let new_val = selector(&parent_for_closure.get_cloned());
            let old_val = inner.value.get_cloned();
            if old_val != new_val {
                inner.value.set(new_val);
                inner.notify();
            }
        });
        parent.add_subscriber(&closure);
        derived
            .inner
            .owned_subscriptions
            .lock()
            .unwrap()
            .push(closure);
        derived
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
        sig.add_subscriber(&cb);
        sig.set(1);
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "subscriber should fire on set"
        );
    }

    #[test]
    fn signal_subscriber_weak_dies_when_strong_dropped() {
        let sig = Signal::new(0u32);
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });
        sig.add_subscriber(&cb);
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
        sig.add_subscriber(&cb);
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
        sig.add_subscriber(&cb);
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

    #[test]
    fn signal_derive_updates_when_parent_changes() {
        let parent = Signal::new(10u32);
        let derived = Signal::derive(parent.clone(), |p| *p + 1);
        assert_eq!(derived.get(), 11);
        parent.set(20);
        assert_eq!(
            derived.get(),
            21,
            "derived should update when parent changes"
        );
    }

    #[test]
    fn signal_derive_noop_when_slice_unchanged() {
        #[derive(PartialEq, Clone, Copy, Debug)]
        struct Data {
            a: u32,
            b: u32,
        }
        let parent = Signal::new(Data { a: 1, b: 100 });
        let derived = Signal::derive(parent.clone(), |p| p.b);
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });
        derived.add_subscriber(&cb);
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
}
