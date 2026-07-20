//! Declarative macros for widget composition.

/// Build a `Vec<Box<dyn Widget>>` from child expressions.
///
/// Each child must implement `ChildPush` (any `impl Widget` or
/// `Option<Box<dyn Widget>>` for conditional children). The resulting
/// `Vec` is typically passed to `MultiChild::new(children, layout)`.
///
/// # Example
///
/// ```ignore
/// MultiChild::new(children![Text::new("A"), Text::new("B")], Layout::column())
/// ```
#[macro_export]
macro_rules! children {
    ($($child:expr),* $(,)?) => {{
        let mut __vexo_children: Vec<::std::boxed::Box<dyn $crate::Widget>> = Vec::new();
        $(
            $crate::widgets::ChildPush::push_into($child, &mut __vexo_children);
        )*
        __vexo_children
    }};
}

#[cfg(test)]
mod tests {
    #[test]
    fn children_macro_builds_vec() {
        let kids: Vec<Box<dyn crate::Widget>> = children![
            crate::Text::new("A"),
            crate::Text::new("B"),
            crate::Text::new("C"),
        ];
        assert_eq!(kids.len(), 3);
    }

    #[test]
    fn children_macro_single_child() {
        let kids: Vec<Box<dyn crate::Widget>> = children![crate::Text::new("Only"),];
        assert_eq!(kids.len(), 1);
    }

    #[test]
    fn children_macro_no_children() {
        let kids: Vec<Box<dyn crate::Widget>> = children![];
        assert_eq!(kids.len(), 0);
    }

    #[test]
    fn children_macro_with_multi_child() {
        use crate::layout::Layout;
        let mc = crate::MultiChild::new(
            children![crate::Text::new("A"), crate::Text::new("B")],
            Layout::column().gap(16.0),
        );
        assert_eq!(mc.children().len(), 2);
    }
}
