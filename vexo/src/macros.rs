//! Widget construction macros that auto-box widgets for ergonomic syntax.
//!
//! These macros eliminate the need for `Box::new()` wrappers when building
//! widget trees, while preserving the builder pattern for styling.

/// Create a Text widget wrapped in Box.
///
/// # Examples
/// ```
/// use vexo::widgets::Text;
/// let t = vexo::text!("Hello");  // Basic text
/// ```
#[macro_export]
macro_rules! text {
    ($content:expr) => {
        Box::new($crate::widgets::Text::new($content))
    };
    ($content:expr, font_size: $size:expr) => {
        Box::new($crate::widgets::Text::new($content).font_size($size))
    };
}

/// Create a ColorWidget wrapped in Box.
///
/// # Example
/// ```
/// use vexo::Color;
/// let w = vexo::color_widget!(Color::RED);  // Just color, use .frame() for size
/// ```
#[macro_export]
macro_rules! color_widget {
    ($color:expr) => {
        Box::new($crate::widgets::ColorWidget::new($color))
    };
}

/// Create a TextEdit widget wrapped in Box.
///
/// # Examples
/// ```
/// use vexo::widgets::TextEdit;
/// let editor = vexo::text_edit!("id", "placeholder");
/// ```
#[macro_export]
macro_rules! text_edit {
    ($id:expr, $placeholder:expr) => {
        Box::new($crate::widgets::TextEdit::new($id, $placeholder))
    };
}

/// Create a Button widget wrapped in Box.
///
/// # Examples
/// ```
/// use vexo::widgets::{Button, Text};
/// #[derive(Clone, Debug)]
/// enum Message { Clicked }
/// let btn = vexo::button!(Box::new(Text::new("Click")), Message::Clicked);
/// ```
#[macro_export]
macro_rules! button {
    ($content:expr, $msg:expr) => {
        Box::new($crate::widgets::Button::new($content, $msg))
    };
}

/// Create a Column container widget wrapped in Box.
///
/// # Examples
/// ```
/// use vexo::widgets::Column;
/// let col: Box<Column<()>> = vexo::column![
///     vexo::text!("Title"),
/// ];
/// ```
#[macro_export]
macro_rules! column {
    // With alignment
    (align: $align:expr, $($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new().align_items($align);
            $(
                col = col.push($child);
            )*
            Box::new(col)
        }
    };
    // With padding
    (padding: $padding:expr, $($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new().padding($padding);
            $(
                col = col.push($child);
            )*
            Box::new(col)
        }
    };
    // With gap
    (gap: $gap:expr, $($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new().gap($gap);
            $(
                col = col.push($child);
            )*
            Box::new(col)
        }
    };
    // Without options
    ($($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new();
            $(
                col = col.push($child);
            )*
            Box::new(col)
        }
    };
}

/// Create a Row container widget wrapped in Box.
///
/// # Example
/// ```
/// use vexo::widgets::Row;
/// let row: Box<Row<()>> = vexo::row![
///     vexo::text!("Left"),
///     vexo::text!("Right"),
/// ];
/// ```
#[macro_export]
macro_rules! row {
    // With alignment
    (align: $align:expr, $($child:expr),* $(,)?) => {
        {
            let mut row = $crate::widgets::Row::new().align_items($align);
            $(
                row = row.push($child);
            )*
            Box::new(row)
        }
    };
    // With padding
    (padding: $padding:expr, $($child:expr),* $(,)?) => {
        {
            let mut row = $crate::widgets::Row::new().padding($padding);
            $(
                row = row.push($child);
            )*
            Box::new(row)
        }
    };
    // With gap
    (gap: $gap:expr, $($child:expr),* $(,)?) => {
        {
            let mut row = $crate::widgets::Row::new().gap($gap);
            $(
                row = row.push($child);
            )*
            Box::new(row)
        }
    };
    // Without options
    ($($child:expr),* $(,)?) => {
        {
            let mut row = $crate::widgets::Row::new();
            $(
                row = row.push($child);
            )*
            Box::new(row)
        }
    };
}

/// Create a Grid container widget wrapped in Box.
///
/// # Examples
/// ```
/// use vexo::widgets::Grid;
/// use vexo::layout::TrackSizing;
/// let grid: Box<Grid<()>> = vexo::grid![
///     vexo::text!("Cell 1"),
///     vexo::text!("Cell 2"),
/// ];
/// ```
#[macro_export]
macro_rules! grid {
    // With columns and rows
    (columns: $cols:expr, rows: $rows:expr, $($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new()
                .columns($cols)
                .rows($rows);
            $(
                grid = grid.push($child);
            )*
            Box::new(grid)
        }
    };
    // With columns only
    (columns: $cols:expr, $($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new().columns($cols);
            $(
                grid = grid.push($child);
            )*
            Box::new(grid)
        }
    };
    // Children only (auto columns)
    ($($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new();
            $(
                grid = grid.push($child);
            )*
            Box::new(grid)
        }
    };
}
