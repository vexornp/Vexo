//! Element and RenderObject identifiers.

use std::sync::atomic::{AtomicUsize, Ordering};

use slotmap::new_key_type;

new_key_type! {
    /// Stable, generational key for elements in the retain-mode element tree.
    /// Unlike ElementId (which is a simple counter), ElementKey provides ABA
    /// protection: if an element is removed and a new element later occupies
    /// the same slot, the old key's generation won't match, so access safely
    /// returns None.
    pub struct ElementKey;
}

new_key_type! {
    /// Stable, generational key for render objects in the retain-mode render tree.
    pub struct RenderObjectKey;
}

/// Legacy element identifier — a simple atomic counter.
/// Will be replaced by ElementKey in a future migration step.

static NEXT_ELEMENT_ID: AtomicUsize = AtomicUsize::new(1);
static NEXT_RENDER_OBJECT_ID: AtomicUsize = AtomicUsize::new(1);

/// Unique identifier for an Element in the element tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ElementId(usize);

impl ElementId {
    /// Generate a new unique ElementId.
    pub fn new() -> Self {
        ElementId(NEXT_ELEMENT_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Create an ElementId from a raw value (for testing).
    #[cfg(test)]
    pub fn from_raw(n: usize) -> Self {
        ElementId(n)
    }
}

impl Default for ElementId {
    fn default() -> Self {
        Self::new()
    }
}

/// Legacy render object identifier — a simple atomic counter.
/// Will be replaced by RenderObjectKey in a future migration step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderObjectId(usize);

impl RenderObjectId {
    /// Generate a new unique RenderObjectId.
    pub fn new() -> Self {
        RenderObjectId(NEXT_RENDER_OBJECT_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Create a RenderObjectId from a raw value (for testing).
    #[cfg(test)]
    pub fn from_raw(n: usize) -> Self {
        RenderObjectId(n)
    }
}

impl Default for RenderObjectId {
    fn default() -> Self {
        Self::new()
    }
}
