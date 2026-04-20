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
    // Without alignment
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
