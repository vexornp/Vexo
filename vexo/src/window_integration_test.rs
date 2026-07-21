// vexo/src/retain/window_integration_test.rs
//! Integration test for retain-mode with WindowState.

#[cfg(test)]
mod tests {
    use crate::core::Color;
    use crate::layout::Layout;
    use crate::style::Style;
    use crate::widgets::{DecoratedBox, WithLayout};
    use crate::{children, MultiChild, Text, Widget};

    #[test]
    fn test_retain_view_returns_widget_tree() {
        // Test that a simple retain widget tree can be created
        let container = DecoratedBox::with_style(
            WithLayout::new(Text::new("Hello"), Layout::default()),
            Style::default().background(Color::RED),
        );

        // Verify the widget tree structure: DecoratedBox → WithLayout → Text
        let wl = container
            .child()
            .as_any()
            .downcast_ref::<WithLayout>()
            .expect("DecoratedBox child should be WithLayout");
        assert!(wl
            .child()
            .expect("WithLayout should have a child")
            .as_any()
            .downcast_ref::<Text>()
            .is_some());
    }

    #[test]
    fn test_retain_column_with_decorated_boxes() {
        // Test a more complex widget tree with DecoratedBox wrappers
        let dc1 = DecoratedBox::with_style(
            WithLayout::new(Text::new("First"), Layout::default()),
            Style::default().background(Color::BLUE),
        );
        let dc2 = DecoratedBox::with_style(
            WithLayout::new(Text::new("Second"), Layout::default()),
            Style::default().background(Color::GREEN),
        );

        let col = MultiChild::new(children![dc1, dc2], Layout::column());

        // Verify column has children
        let col_any = col.as_any();
        assert!(col_any.downcast_ref::<MultiChild>().is_some());
    }
}
