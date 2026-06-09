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

/// Generate layout property builder methods for a widget struct.
///
/// Each method delegates to the corresponding `Layout` builder,
/// setting only that field on the widget's existing `self.layout`.
/// This preserves all other layout properties, unlike `.layout()`
/// which replaces the entire Layout.
///
/// Usage: `impl MyWidget { layout_builder_methods!(); }`
///
/// Requires: the struct must have a `layout: Layout` field, and the
/// following types must be in scope where the macro is invoked:
/// `Layout`, `FlexDirection`, `FlexWrap`, `AlignItems`, `AlignContent`,
/// `JustifyContent`, `Overflow`, `AlignSelf`, `EdgeInsets`, `Dimension`,
/// `Size<Logical>`, `Inset`.
#[macro_export]
macro_rules! layout_builder_methods {
    () => {
        // Box model
        pub fn padding(mut self, value: f32) -> Self {
            self.layout = self.layout.padding(value);
            self
        }
        pub fn padding_each(mut self, left: f32, right: f32, top: f32, bottom: f32) -> Self {
            self.layout = self.layout.padding_each(left, right, top, bottom);
            self
        }
        pub fn margin(mut self, value: f32) -> Self {
            self.layout = self.layout.margin(value);
            self
        }
        pub fn margin_each(mut self, left: f32, right: f32, top: f32, bottom: f32) -> Self {
            self.layout = self.layout.margin_each(left, right, top, bottom);
            self
        }
        pub fn width(mut self, value: f32) -> Self {
            self.layout = self.layout.width(value);
            self
        }
        pub fn height(mut self, value: f32) -> Self {
            self.layout = self.layout.height(value);
            self
        }
        pub fn width_percent(mut self, value: f32) -> Self {
            self.layout = self.layout.width_percent(value);
            self
        }
        pub fn height_percent(mut self, value: f32) -> Self {
            self.layout = self.layout.height_percent(value);
            self
        }
        pub fn min_width(mut self, value: f32) -> Self {
            self.layout = self.layout.min_width(value);
            self
        }
        pub fn min_height(mut self, value: f32) -> Self {
            self.layout = self.layout.min_height(value);
            self
        }
        pub fn max_width(mut self, value: f32) -> Self {
            self.layout = self.layout.max_width(value);
            self
        }
        pub fn max_height(mut self, value: f32) -> Self {
            self.layout = self.layout.max_height(value);
            self
        }
        // Flexbox
        pub fn flex_direction(mut self, value: FlexDirection) -> Self {
            self.layout = self.layout.flex_direction(value);
            self
        }
        pub fn flex_wrap(mut self) -> Self {
            self.layout = self.layout.flex_wrap();
            self
        }
        pub fn flex_wrap_mode(mut self, value: FlexWrap) -> Self {
            self.layout = self.layout.flex_wrap_mode(value);
            self
        }
        pub fn flex_grow(mut self, value: f32) -> Self {
            self.layout = self.layout.flex_grow(value);
            self
        }
        pub fn flex_shrink(mut self, value: f32) -> Self {
            self.layout = self.layout.flex_shrink(value);
            self
        }
        pub fn flex_basis(mut self, value: f32) -> Self {
            self.layout = self.layout.flex_basis(value);
            self
        }
        pub fn justify(mut self, value: JustifyContent) -> Self {
            self.layout = self.layout.justify(value);
            self
        }
        pub fn align(mut self, value: AlignItems) -> Self {
            self.layout = self.layout.align(value);
            self
        }
        pub fn align_content(mut self, value: AlignContent) -> Self {
            self.layout = self.layout.align_content(value);
            self
        }
        pub fn gap(mut self, value: f32) -> Self {
            self.layout = self.layout.gap(value);
            self
        }
        pub fn gap_size(mut self, size: Size<Logical>) -> Self {
            self.layout = self.layout.gap_size(size);
            self
        }
        pub fn gap_each(mut self, width: f32, height: f32) -> Self {
            self.layout = self.layout.gap_each(width, height);
            self
        }
        // Positioning
        pub fn absolute(mut self) -> Self {
            self.layout = self.layout.absolute();
            self
        }
        pub fn relative(mut self) -> Self {
            self.layout = self.layout.relative();
            self
        }
        pub fn inset(mut self, value: f32) -> Self {
            self.layout = self.layout.inset(value);
            self
        }
        pub fn top(mut self, value: f32) -> Self {
            self.layout = self.layout.top(value);
            self
        }
        pub fn right(mut self, value: f32) -> Self {
            self.layout = self.layout.right(value);
            self
        }
        pub fn bottom(mut self, value: f32) -> Self {
            self.layout = self.layout.bottom(value);
            self
        }
        pub fn left(mut self, value: f32) -> Self {
            self.layout = self.layout.left(value);
            self
        }
        // Per-item alignment
        pub fn align_self(mut self, value: AlignSelf) -> Self {
            self.layout = self.layout.align_self(value);
            self
        }
        // Sizing
        pub fn aspect_ratio(mut self, value: f32) -> Self {
            self.layout = self.layout.aspect_ratio(value);
            self
        }
        // Overflow
        pub fn overflow(mut self, value: Overflow) -> Self {
            self.layout = self.layout.overflow(value);
            self
        }
        pub fn overflow_x(mut self, value: Overflow) -> Self {
            self.layout = self.layout.overflow_x(value);
            self
        }
        pub fn overflow_y(mut self, value: Overflow) -> Self {
            self.layout = self.layout.overflow_y(value);
            self
        }
    };
}
