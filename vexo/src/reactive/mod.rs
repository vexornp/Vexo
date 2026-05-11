//! Reactive state primitives for the Vexo framework.
//!
//! Provides `StatefulMutable<T>` which bridges futures-signals `Mutable<T>`
//! with the Vexo BuildOwner for automatic dirty marking when state changes.

pub use futures_signals::signal::{Mutable, ReadOnlyMutable, Signal, SignalExt};

use std::sync::Arc;

/// A `Mutable<T>` that automatically marks its owning element dirty when changed.
///
/// This is the primary reactive primitive for `StatefulWidget` state.
/// When `set()` is called and the value actually changes, the owning
/// `StatefulElement` is automatically marked dirty in the `BuildOwner`,
/// triggering a rebuild on the next frame.
///
/// # Usage in State
///
/// ```ignore
/// use vexo::reactive::StatefulMutable;
///
/// struct CounterState {
///     count: StatefulMutable<u32>,
/// }
///
/// impl Default for CounterState {
///     fn default() -> Self {
///         Self {
///             count: StatefulMutable::new(0),
///         }
///     }
/// }
///
/// impl retain::State for CounterState {
///     fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
///         self.count.set_dirty_callback(callback);
///     }
/// }
///
/// // In a callback:
/// state.count.set(5);  // Automatically marks element dirty
/// ```
///
/// # Clone Semantics
///
/// `StatefulMutable<T>` uses Arc semantics — cloning creates a new handle
/// to the same underlying value and dirty callback. This allows callbacks
/// to capture clones and still trigger rebuilds.
pub struct StatefulMutable<T> {
    /// The underlying Mutable value.
    inner: Mutable<T>,

    /// Callback invoked when the value changes.
    /// Set during StatefulElement mount via `set_dirty_callback()`.
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl<T> StatefulMutable<T> {
    /// Create a new `StatefulMutable` with the given initial value.
    ///
    /// The dirty callback is not set until the element is mounted.
    /// Until then, `set()` will update the value but not trigger a rebuild.
    pub fn new(value: T) -> Self {
        Self {
            inner: Mutable::new(value),
            on_change: None,
        }
    }

    /// Set the dirty callback. Called once during `StatefulElement` mount.
    ///
    /// The callback marks the owning element dirty in the BuildOwner.
    pub fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.on_change = Some(callback);
    }

    /// Get a read-only view of the underlying `Mutable`.
    pub fn read_only(&self) -> ReadOnlyMutable<T> {
        self.inner.read_only()
    }
}

impl<T: Copy> StatefulMutable<T> {
    /// Get the current value (requires `T: Copy`).
    pub fn get(&self) -> T {
        self.inner.get()
    }

    /// Set the value and mark the owning element dirty.
    ///
    /// The element will be rebuilt on the next frame.
    pub fn set(&self, value: T) {
        self.inner.set(value);
        if let Some(ref callback) = self.on_change {
            callback();
        }
    }
}

impl<T: Clone> StatefulMutable<T> {
    /// Get a cloned copy of the current value (requires `T: Clone`).
    pub fn get_cloned(&self) -> T {
        self.inner.get_cloned()
    }

    /// Set the value from a reference and mark dirty.
    pub fn set_from(&self, value: &T) {
        self.inner.set(value.clone());
        if let Some(ref callback) = self.on_change {
            callback();
        }
    }
}

impl<T: PartialEq + Copy> StatefulMutable<T> {
    /// Set the value only if it's different, and mark dirty if changed.
    ///
    /// This avoids spurious rebuilds when the value hasn't actually changed.
    pub fn set_neq(&self, value: T) {
        let old = self.inner.get();
        self.inner.set_neq(value);
        if old != value {
            if let Some(ref callback) = self.on_change {
                callback();
            }
        }
    }
}

impl<T> Clone for StatefulMutable<T> {
    /// Clone creates a new handle to the same underlying value.
    ///
    /// Both the original and the clone share the same `Mutable<T>` and
    /// dirty callback. Setting the value through either handle will
    /// trigger a rebuild.
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            on_change: self.on_change.clone(),
        }
    }
}

impl<T: Default> Default for StatefulMutable<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
