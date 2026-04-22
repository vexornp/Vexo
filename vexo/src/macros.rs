//! Widget construction macros for ergonomic syntax.
//!
//! All widget macros return the unboxed type, allowing method chaining
//! before calling `.boxed()`.
//!
//! Container macros (`column!`, `row!`, `grid!`) accept children without
//! requiring `.boxed()` - the conversion is implicit.

/// Create a Text widget.
///
/// Returns `Text` (unboxed) so you can chain layout methods
/// before calling `.boxed()`.
///
/// # Example
/// ```
/// use vexo::{text, WidgetExt};
///
/// #[derive(Debug, Clone)]
/// enum Message {}
///
/// let txt: Box<dyn vexo::widgets::Widget<Message>> = text!("Hello")
///     .font_size(24.0)
///     .width(100.0)
///     .height(50.0)
///     .boxed();
/// ```
#[macro_export]
macro_rules! text {
    ($content:expr) => {
        $crate::widgets::Text::new($content)
    };
}

/// Create a ColorWidget.
///
/// Returns `ColorWidget` (unboxed) so you can chain layout methods
/// before calling `.boxed()`.
///
/// # Example
/// ```
/// use vexo::{color_widget, Color, WidgetExt};
///
/// #[derive(Debug, Clone)]
/// enum Message {}
///
/// let widget: Box<dyn vexo::widgets::Widget<Message>> = color_widget!(Color::RED)
///     .width(100.0)
///     .height(50.0)
///     .boxed();
/// ```
#[macro_export]
macro_rules! color_widget {
    ($color:expr) => {
        $crate::widgets::ColorWidget::new($color)
    };
}

/// Create a TextEdit widget.
///
/// Returns `TextEdit` (unboxed) so you can chain layout methods
/// before calling `.boxed()`.
///
/// # Example
/// ```
/// use vexo::{text_edit, WidgetExt};
///
/// #[derive(Debug, Clone)]
/// enum Message {}
///
/// let editor: Box<dyn vexo::widgets::Widget<Message>> = text_edit!("editor_id")
///     .content("Type here...")
///     .width(200.0)
///     .height(50.0)
///     .boxed();
/// ```
#[macro_export]
macro_rules! text_edit {
    ($id:expr) => {
        $crate::widgets::TextEdit::new($id)
    };
}

/// Create a Button widget.
///
/// Returns `Button<M>` (unboxed) so you can chain layout methods
/// before calling `.boxed()`.
///
/// The content widget doesn't need `.boxed()` - the macro adds it automatically.
///
/// # Example
/// ```
/// use vexo::{button, text, WidgetExt};
///
/// #[derive(Debug, Clone)]
/// enum Message { Clicked }
///
/// let btn = button!(text!("Click me"), Message::Clicked)
///     .width(100.0)
///     .height(50.0)
///     .boxed();
/// ```
#[macro_export]
macro_rules! button {
    ($content:expr, $msg:expr) => {
        $crate::widgets::Button::new($content.boxed(), $msg)
    };
}

/// Create a Column container widget.
///
/// Returns `Column<M>` (unboxed) so you can chain layout methods
/// before calling `.boxed()`.
///
/// Children don't need `.boxed()` - the macro adds it automatically.
///
/// # Example
/// ```
/// use vexo::{column, text, WidgetExt};
///
/// #[derive(Debug, Clone)]
/// enum Message {}
///
/// let col: Box<dyn vexo::widgets::Widget<Message>> = column![
///     text!("Title"),
///     text!("Body"),
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
                col = col.push($child.boxed());
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
/// Children don't need `.boxed()` - the macro adds it automatically.
///
/// # Example
/// ```
/// use vexo::{row, text, WidgetExt};
///
/// #[derive(Debug, Clone)]
/// enum Message {}
///
/// let row: Box<dyn vexo::widgets::Widget<Message>> = row![
///     text!("Left"),
///     text!("Right"),
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
                row = row.push($child.boxed());
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
/// Children don't need `.boxed()` - the macro adds it automatically.
///
/// # Example
/// ```
/// use vexo::{grid, text, layout::TrackSizing, WidgetExt};
///
/// #[derive(Debug, Clone)]
/// enum Message {}
///
/// let grid: Box<dyn vexo::widgets::Widget<Message>> = grid![
///     text!("Cell 1"),
///     text!("Cell 2"),
/// ]
/// .columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(1.0)])
/// .rows(vec![TrackSizing::Px(40.0), TrackSizing::Px(40.0)])
/// .gap(5.0)
/// .boxed();
/// ```
#[macro_export]
macro_rules! grid {
    ($($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new();
            $(
                grid = grid.push($child.boxed());
            )*
            grid
        }
    };
}
