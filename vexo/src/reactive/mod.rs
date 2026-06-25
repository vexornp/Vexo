//! Reactive state primitives for the Vexo framework.
//!
//! Provides `Signal<T>` which bridges futures-signals `Mutable<T>`
//! with the Vexo BuildOwner for automatic dirty marking when state changes.

pub use futures_signals::signal::{Mutable, ReadOnlyMutable, SignalExt};

use std::sync::Arc;

/// Reactive state primitive — the Vexo equivalent of React's `useState` or Vue's `ref()`.
///
/// When `set()` is called and the value changes, the owning element is
/// automatically marked dirty, triggering a rebuild on the next frame.
///
/// # Usage in ComponentState
///
/// ```ignore
/// use vexo::Signal;
///
/// #[derive(ComponentState)]
/// struct CounterState {
///     count: Signal<u32>,
/// }
///
/// impl Default for CounterState {
///     fn default() -> Self {
///         Self { count: Signal::new(0) }
///     }
/// }
///
/// // In a callback:
/// state.count.set(5);  // Automatically marks element dirty
/// ```
///
/// # Clone Semantics
///
/// `Signal<T>` uses Arc semantics — cloning creates a new handle
/// to the same underlying value and dirty callback. This allows callbacks
/// to capture clones and still trigger rebuilds.
pub struct Signal<T> {
    /// The underlying Mutable value.
    inner: Mutable<T>,

    /// Callback invoked when the value changes.
    /// Set during StatefulElement mount via `set_dirty_callback()`.
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl<T> Signal<T> {
    /// Create a new `Signal` with the given initial value.
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

impl<T: PartialEq + Copy> Signal<T> {
    /// Get the current value (requires `T: Copy`).
    pub fn get(&self) -> T {
        self.inner.get()
    }

    /// Set the value and mark the owning element dirty only if the value changed.
    ///
    /// If the new value equals the old value, no dirty callback is fired,
    /// avoiding spurious rebuilds.
    pub fn set(&self, value: T) {
        let old = self.inner.get();
        self.inner.set(value);
        if old != value {
            if let Some(ref callback) = self.on_change {
                callback();
            }
        }
    }
}

impl<T: PartialEq + Clone> Signal<T> {
    /// Get a cloned copy of the current value (requires `T: Clone`).
    pub fn get_cloned(&self) -> T {
        self.inner.get_cloned()
    }

    /// Set the value from a reference and mark dirty only if the value changed.
    pub fn set_from(&self, value: &T) {
        let old = self.inner.get_cloned();
        self.inner.set(value.clone());
        if old != *value {
            if let Some(ref callback) = self.on_change {
                callback();
            }
        }
    }
}

impl<T> Clone for Signal<T> {
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

impl<T: Default> Default for Signal<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
