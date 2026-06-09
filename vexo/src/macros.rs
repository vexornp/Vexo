//! Declarative macros for widget composition.

/// Create a column (vertical Flex) widget with children.
///
/// ```ignore
/// column![Text::new("Hello"), Text::new("World")]
/// ```
/// expands to:
/// ```ignore
/// Flex::column().push(Text::new("Hello")).push(Text::new("World"))
/// ```
#[macro_export]
macro_rules! column {
    ($($child:expr),* $(,)?) => {{
        let mut col = $crate::Flex::column();
        $(col = col.push($child);)*
        col
    }};
}

/// Create a row (horizontal Flex) widget with children.
///
/// ```ignore
/// row![Text::new("Left"), Text::new("Right")]
/// ```
/// expands to:
/// ```ignore
/// Flex::row().push(Text::new("Left")).push(Text::new("Right"))
/// ```
#[macro_export]
macro_rules! row {
    ($($child:expr),* $(,)?) => {{
        let mut row = $crate::Flex::row();
        $(row = row.push($child);)*
        row
    }};
}

/// Create a `Grid` widget with children.
///
/// ```ignore
/// grid![child1, child2]
/// ```
/// expands to:
/// ```ignore
/// Grid::new().push(child1).push(child2)
/// ```
#[macro_export]
macro_rules! grid {
    ($($child:expr),* $(,)?) => {{
        let mut grid = $crate::Grid::new();
        $(grid = grid.push($child);)*
        grid
    }};
}
