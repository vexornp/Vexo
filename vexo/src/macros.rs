//! Widget construction macros that auto-box widgets for ergonomic syntax.
//!
//! These macros eliminate the need for `Box::new()` wrappers when building
//! widget trees, while preserving the builder pattern for styling.

/// Create a Text widget wrapped in Box.
///
/// # Examples
/// ```
/// text!("Hello")                    // Basic text
/// text!("Hello", size: 24.0)        // With font size
/// ```
#[macro_export]
macro_rules! text {
    ($content:expr) => {
        Box::new($crate::widgets::Text::new($content))
    };
    ($content:expr, size: $size:expr) => {
        Box::new($crate::widgets::Text::new($content).size($size))
    };
}

/// Create a Rectangle widget wrapped in Box.
///
/// # Example
/// ```
/// use vexo::Color;
/// rect!(60.0, 70.0, Color::RED)           // width, height, Color
/// rect!(60.0, 70.0, [1.0, 0.0, 0.0])      // width, height, RGB array (also works)
/// ```
#[macro_export]
macro_rules! rect {
    ($width:expr, $height:expr, $color:expr) => {
        Box::new($crate::widgets::Rectangle::new($width, $height, $color))
    };
}

/// Create a TextEdit widget wrapped in Box.
///
/// # Examples
/// ```
/// text_edit!("id", "placeholder")
/// text_edit!("id", "placeholder", size: (100.0, 50.0))
/// ```
#[macro_export]
macro_rules! text_edit {
    ($id:expr, $placeholder:expr) => {
        Box::new($crate::widgets::TextEdit::new($id, $placeholder))
    };
    ($id:expr, $placeholder:expr, size: $size:expr) => {
        Box::new($crate::widgets::TextEdit::new($id, $placeholder).size($size))
    };
}

/// Create a Button widget wrapped in Box.
///
/// # Examples
/// ```
/// use vexo::Color;
/// button!(text!("Click"), Message::Clicked)
/// button!(text!("Click"), Message::Clicked, color: Color::rgb(0.1, 0.4, 0.1))
/// button!(text!("Click"), Message::Clicked, color: [0.1, 0.4, 0.1])  // RGB array also works
/// ```
#[macro_export]
macro_rules! button {
    ($content:expr, $msg:expr) => {
        Box::new($crate::widgets::Button::new($content, $msg))
    };
    ($content:expr, $msg:expr, color: $color:expr) => {
        Box::new($crate::widgets::Button::new($content, $msg).color($color))
    };
}

/// Create a Column container widget wrapped in Box.
///
/// # Examples
/// ```
/// column![
///     text!("Title"),
///     rect!(60.0, 70.0, [1.0, 0.0, 0.0]),
/// ]
///
/// column![
///     align: Center,
///     text!("Title"),
///     button!(text!("Click"), Message::Clicked),
/// ]
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
/// row![
///     text!("Left"),
///     text!("Right"),
/// ]
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
