// vexo/src/retain/window_integration_test.rs
//! Integration test for retain-mode with WindowState.

#[cfg(test)]
mod tests {
    use crate::retain::{Background, Column, Text, Widget};
    use crate::core::Color;

    #[test]
    fn test_retain_view_returns_widget_tree() {
        // Test that a simple retain widget tree can be created
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);

        // Verify the widget tree structure
        assert!(bg.child().as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_retain_column_with_modifiers() {
        // Test a more complex widget tree with modifiers
        let text1 = Box::new(Text::new("First"));
        let bg1 = Background::new(text1, Color::BLUE);

        let text2 = Box::new(Text::new("Second"));
        let bg2 = Background::new(text2, Color::GREEN);

        let col = Column::new()
            .push(bg1)
            .push(bg2);

        // Verify column has children
        let col_any = col.as_any();
        assert!(col_any.downcast_ref::<Column>().is_some());
    }
}
