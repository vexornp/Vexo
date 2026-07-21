//! `ChildPush` trait — used by `MultiChild::push` and the `children!` macro.

use super::Widget;

/// Trait for types that can be pushed as children into a container.
///
/// Implemented by `impl Widget` (always pushed) and `Option<Box<dyn Widget>>`
/// (pushed only if `Some`, skipped if `None`). This enables the `children![]`
/// macro to handle conditional children transparently.
pub trait ChildPush {
    fn push_into(self, children: &mut Vec<Box<dyn Widget>>);
}

impl<W: Widget + 'static> ChildPush for W {
    fn push_into(self, children: &mut Vec<Box<dyn Widget>>) {
        children.push(self.boxed());
    }
}

impl ChildPush for Option<Box<dyn Widget>> {
    fn push_into(self, children: &mut Vec<Box<dyn Widget>>) {
        if let Some(child) = self {
            children.push(child);
        }
    }
}
