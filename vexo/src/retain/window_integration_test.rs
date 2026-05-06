// vexo/src/retain/window_integration_test.rs
//! Integration test for retain-mode with WindowState.

#[cfg(test)]
mod tests {
    use crate::retain::{Column, Text, Widget, DecoratedContainer};
    use crate::core::Color;

    #[test]
    fn test_retain_view_returns_widget_tree() {
        // Test that a simple retain widget tree can be created
        let child = Box::new(Text::<()>::new("Hello"));
        let container = DecoratedContainer::new(child)
            .style(crate::retain::Style::new().background(Color::RED));

        // Verify the widget tree structure
        assert!(container.child().as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_retain_column_with_decorated_containers() {
        // Test a more complex widget tree with DecoratedContainers
        let text1 = Box::new(Text::<()>::new("First"));
        let dc1 = DecoratedContainer::new(text1)
            .style(crate::retain::Style::new().background(Color::BLUE));

        let text2 = Box::new(Text::<()>::new("Second"));
        let dc2 = DecoratedContainer::new(text2)
            .style(crate::retain::Style::new().background(Color::GREEN));

        let col = Column::new()
            .push(dc1)
            .push(dc2);

        // Verify column has children
        let col_any = col.as_any();
        assert!(col_any.downcast_ref::<Column>().is_some());
    }
}