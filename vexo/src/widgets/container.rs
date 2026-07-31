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

/// Splice a pre-built `Vec<Box<dyn Widget>>` into a container. Used by
/// `view_builder::build_optional` / `build_array` to flatten control-flow
/// results (0 or N children) into the parent without injecting extra tree
/// nodes.
impl ChildPush for Vec<Box<dyn Widget>> {
    fn push_into(self, children: &mut Vec<Box<dyn Widget>>) {
        children.extend(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn vec_push_into_extends_destination() {
        let mut dst: Vec<Box<dyn Widget>> = vec![Text::new("a").boxed()];
        let src: Vec<Box<dyn Widget>> = vec![Text::new("b").boxed(), Text::new("c").boxed()];
        src.push_into(&mut dst);
        assert_eq!(dst.len(), 3);
    }

    #[test]
    fn vec_push_into_empty_is_noop() {
        let mut dst: Vec<Box<dyn Widget>> = vec![Text::new("a").boxed()];
        let src: Vec<Box<dyn Widget>> = vec![];
        src.push_into(&mut dst);
        assert_eq!(dst.len(), 1);
    }
}
