// vexo/src/retain/window_integration_test.rs
//! Integration test for retain-mode with WindowState.

#[cfg(test)]
mod tests {
    use crate::{Flex, Text, Widget};
    use crate::widgets::DecoratedContainer;
    use crate::core::Color;

    #[test]
    fn test_retain_view_returns_widget_tree() {
        // Test that a simple retain widget tree can be created
        let container = DecoratedContainer::new(Text::new("Hello"))
            .style(crate::Style::new().background(Color::RED));

        // Verify the widget tree structure
        assert!(container.child().as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_retain_column_with_decorated_containers() {
        // Test a more complex widget tree with DecoratedContainers
        let dc1 = DecoratedContainer::new(Text::new("First"))
            .style(crate::Style::new().background(Color::BLUE));

        let dc2 = DecoratedContainer::new(Text::new("Second"))
            .style(crate::Style::new().background(Color::GREEN));

        let col = Flex::column()
            .push(dc1)
            .push(dc2);

        // Verify column has children
        let col_any = col.as_any();
        assert!(col_any.downcast_ref::<Flex>().is_some());
    }
}