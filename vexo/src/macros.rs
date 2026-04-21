//! Widget construction macros for ergonomic syntax.
//!
//! Leaf widget macros (text!, button!, etc.) return Box<dyn Widget<M>>.
//! Container macros (column!, row!, grid!) return the unboxed type,
//! allowing method chaining before .boxed().

/// Create a Text widget wrapped in Box.
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
#[macro_export]
macro_rules! color_widget {
    ($color:expr) => {
        Box::new($crate::widgets::ColorWidget::new($color))
    };
}

/// Create a TextEdit widget wrapped in Box.
#[macro_export]
macro_rules! text_edit {
    ($id:expr, $placeholder:expr) => {
        Box::new($crate::widgets::TextEdit::new($id, $placeholder))
    };
}

/// Create a Button widget wrapped in Box.
#[macro_export]
macro_rules! button {
    ($content:expr, $msg:expr) => {
        Box::new($crate::widgets::Button::new($content, $msg))
    };
}

/// Create a Column container widget.
///
/// Returns `Column<M>` (unboxed) so you can chain layout methods
/// before calling `.boxed()`.
///
/// # Example
/// ```
/// use vexo::column;
/// let col = column![
///     vexo::text!("Title"),
///     vexo::text!("Body"),
/// ]
/// .padding(10.0)
/// .gap(5.0)
/// .boxed();
/// ```
#[macro_export]
macro_rules! column {
    ($($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new();
            $(
                col = col.push($child);
            )*
            col
        }
    };
}

/// Create a Row container widget.
///
/// Returns `Row<M>` (unboxed) so you can chain layout methods
/// before calling `.boxed()`.
///
/// # Example
/// ```
/// use vexo::row;
/// let row = row![
///     vexo::text!("Left"),
///     vexo::text!("Right"),
/// ]
/// .gap(10.0)
/// .boxed();
/// ```
#[macro_export]
macro_rules! row {
    ($($child:expr),* $(,)?) => {
        {
            let mut row = $crate::widgets::Row::new();
            $(
                row = row.push($child);
            )*
            row
        }
    };
}

/// Create a Grid container widget.
///
/// Returns `Grid<M>` (unboxed) so you can chain layout methods
/// before calling `.boxed()`.
///
/// # Example
/// ```
/// use vexo::{grid, layout::TrackSizing};
/// let grid = grid![
///     columns: vec![TrackSizing::Fr(1.0), TrackSizing::Fr(1.0)],
///     rows: vec![TrackSizing::Px(40.0), TrackSizing::Px(40.0)],
///     vexo::text!("Cell 1"),
///     vexo::text!("Cell 2"),
/// ]
/// .gap(5.0)
/// .boxed();
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
            grid
        }
    };
    // With columns only
    (columns: $cols:expr, $($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new().columns($cols);
            $(
                grid = grid.push($child);
            )*
            grid
        }
    };
    // Children only (auto columns)
    ($($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new();
            $(
                grid = grid.push($child);
            )*
            grid
        }
    };
}
