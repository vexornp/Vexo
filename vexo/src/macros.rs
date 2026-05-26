//! Widget construction macros for ergonomic syntax.
//!
//! These macros are deprecated and will be removed in a later task.
//! They previously created immediate-mode widgets which have been removed.
//! Retain-mode widgets should be constructed directly instead.

/// Deprecated: Use `retain::Text::new()` instead.
#[macro_export]
macro_rules! text {
    ($content:expr) => {
        compile_error!("text! macro is deprecated. Use retain::Text::new() instead.")
    };
}

/// Deprecated: Use `retain::Column::new()` instead.
#[macro_export]
macro_rules! column {
    ($($child:expr),* $(,)?) => {
        compile_error!("column! macro is deprecated. Use retain::Column::new() instead.")
    };
}

/// Deprecated: Use `retain::Row::new()` instead.
#[macro_export]
macro_rules! row {
    ($($child:expr),* $(,)?) => {
        compile_error!("row! macro is deprecated. Use retain::Row::new() instead.")
    };
}

/// Deprecated: Use `retain::GestureDetector` instead.
#[macro_export]
macro_rules! button {
    ($content:expr, $msg:expr) => {
        compile_error!("button! macro is deprecated. Use retain::GestureDetector instead.")
    };
}

/// Deprecated: Use `retain::TextEdit::new()` instead.
#[macro_export]
macro_rules! text_edit {
    ($id:expr) => {
        compile_error!("text_edit! macro is deprecated. Use retain::TextEdit::new() instead.")
    };
}

/// Deprecated: ColorWidget has been removed.
#[macro_export]
macro_rules! color_widget {
    ($color:expr) => {
        compile_error!("color_widget! macro is deprecated. ColorWidget has been removed.")
    };
}

/// Deprecated: Grid has been removed.
#[macro_export]
macro_rules! grid {
    ($($child:expr),* $(,)?) => {
        compile_error!("grid! macro is deprecated. Grid has been removed.")
    };
}

/// Deprecated: Component system will be removed in a later task.
#[macro_export]
macro_rules! component {
    ($component:ty, $key:expr, $mapper:expr) => {
        compile_error!("component! macro is deprecated. Component system will be removed in a later task.")
    };
}
