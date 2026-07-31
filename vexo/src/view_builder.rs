//! Type-erased result-builder helpers. Mirrors Swift's `ViewBuilder` vocabulary.
//!
//! Each function is a thin adapter over `Vec<Box<dyn Widget>>` + `ChildPush`.
//! The `column!`/`row!` macros (in `vexo_macros`) emit calls to these helpers
//! for control-flow statements (`if`, `for`); plain widgets go straight through
//! `ChildPush::push_into`.

use crate::widgets::Widget;

/// Identity for the block's collected children. The macro builds the `Vec`
/// inline via `ChildPush::push_into`, so this fn is called only when a user
/// invokes the builder API by hand. Mirrors Swift's `buildBlock`.
pub fn build_block(children: Vec<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    children
}

/// `if cond { body }` (no else). `None` renders nothing. Returns a `Vec`
/// (0 or 1 elements) so it flattens into the parent via `ChildPush for Vec`.
/// Mirrors Swift's `buildOptional`.
pub fn build_optional(child: Option<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    match child {
        Some(c) => vec![c],
        None => vec![],
    }
}

/// `if cond { a } else { b }`. Both arms already erased to `Box<dyn Widget>`,
/// so this is identity. Kept for vocabulary symmetry with Swift's
/// `buildEither`; documents intent. The macro wraps `if/else` in this call.
pub fn build_either(child: Box<dyn Widget>) -> Box<dyn Widget> {
    child
}

/// `for x in xs { body }`. Collects all iterations into a `Vec`, which then
/// flattens into the parent via `ChildPush for Vec`. Mirrors Swift's
/// `buildArray`.
pub fn build_array(children: Vec<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn build_block_is_identity() {
        let v = build_block(vec![Text::new("a").boxed(), Text::new("b").boxed()]);
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn build_optional_some_yields_one() {
        let v = build_optional(Some(Text::new("x").boxed()));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn build_optional_none_yields_zero() {
        let v = build_optional(None::<Box<dyn Widget>>);
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn build_either_is_identity() {
        let w: Box<dyn Widget> = Text::new("x").boxed();
        let out = build_either(w);
        assert!(out.as_any().is::<Text>());
    }

    #[test]
    fn build_array_passes_through() {
        let v = build_array(vec![Text::new("a").boxed(), Text::new("b").boxed()]);
        assert_eq!(v.len(), 2);
    }
}
