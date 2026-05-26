use crate::id::ElementKey;
use crate::widgets::Widget;

/// A command emitted by an element to request a child tree operation.
/// The pipeline executes these after the element method returns.
pub enum ChildOp {
    /// Mount a new child element at the given slot
    Inflate {
        slot: Option<usize>,
        widget: Box<dyn Widget>,
        parent: ElementKey,
    },
    /// Update an existing child element with a new widget
    Update {
        child: ElementKey,
        widget: Box<dyn Widget>,
    },
    /// Unmount a child element
    Unmount {
        child: ElementKey,
    },
}

/// Accumulator for child operations emitted during element lifecycle methods.
/// Elements push ops here instead of directly accessing the ElementRegistry.
pub struct ChildOps {
    ops: Vec<ChildOp>,
}

impl ChildOps {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Request inflation of a new child element.
    pub fn inflate(&mut self, slot: Option<usize>, widget: Box<dyn Widget>, parent: ElementKey) {
        self.ops.push(ChildOp::Inflate { slot, widget, parent });
    }

    /// Request update of an existing child element.
    pub fn update(&mut self, child: ElementKey, widget: Box<dyn Widget>) {
        self.ops.push(ChildOp::Update { child, widget });
    }

    /// Request unmount of a child element.
    pub fn unmount(&mut self, child: ElementKey) {
        self.ops.push(ChildOp::Unmount { child });
    }

    /// Drain all pending operations, leaving the accumulator empty.
    pub fn drain(&mut self) -> Vec<ChildOp> {
        std::mem::take(&mut self.ops)
    }

    /// Check if there are any pending operations.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl Default for ChildOps {
    fn default() -> Self {
        Self::new()
    }
}
