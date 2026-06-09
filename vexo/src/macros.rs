//! Declarative macros for widget composition.

/// Create a `Column` widget with children.
///
/// ```ignore
/// column![Text::new("Hello"), Text::new("World")]
/// ```
/// expands to:
/// ```ignore
/// Column::new().push(Text::new("Hello")).push(Text::new("World"))
/// ```
#[macro_export]
macro_rules! column {
    ($($child:expr),* $(,)?) => {{
        let mut col = $crate::Column::new();
        $(col = col.push($child);)*
        col
    }};
}

/// Create a `Row` widget with children.
///
/// ```ignore
/// row![Text::new("Left"), Text::new("Right")]
/// ```
/// expands to:
/// ```ignore
/// Row::new().push(Text::new("Left")).push(Text::new("Right"))
/// ```
#[macro_export]
macro_rules! row {
    ($($child:expr),* $(,)?) => {{
        let mut row = $crate::Row::new();
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