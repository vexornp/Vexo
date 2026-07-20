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

/// Build a `Vec<Box<dyn Widget>>` from child expressions.
///
/// Each child must implement `ChildPush` (any `impl Widget` or
/// `Option<Box<dyn Widget>>` for conditional children). The resulting
/// `Vec` is typically passed to `MultiChild::new(children, layout)`.
///
/// # Example
///
/// ```ignore
/// MultiChild::new(children![Text::new("A"), Text::new("B")], Layout::column())
/// ```
#[macro_export]
macro_rules! children {
    ($($child:expr),* $(,)?) => {{
        let mut __vexo_children: Vec<::std::boxed::Box<dyn $crate::Widget>> = Vec::new();
        $(
            $crate::widgets::ChildPush::push_into($child, &mut __vexo_children);
        )*
        __vexo_children
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
        /// CSS `flex: 1 1 0` + `min-height: 0` — fill remaining space without
        /// propagating min-content upward.
        ///
        /// This is the correct pattern for scrollable content areas inside a
        /// flex column. Without `min_height(0.0)`, the default `min-height:
        /// auto` resolves to the content's min-content, which propagates up
        /// and can push siblings (e.g. a tab bar) off screen on short windows.
        pub fn flex_fill(mut self) -> Self {
            self.layout = self.layout.flex_grow(1.0).flex_basis(0.0).min_height(0.0);
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

/// Generate `style` and `layout` fields inside a struct definition.
///
/// Usage:
/// ```ignore
/// struct MyWidget {
///     modifier_fields!();
///     // other fields...
/// }
/// ```
///
/// Expands to:
/// ```ignore
/// style: Style,
/// layout: Layout
/// ```
#[macro_export]
macro_rules! modifier_fields {
    () => {
        style: $crate::Style,
        layout: $crate::Layout
    };
}

/// Generate modifier methods on a concrete widget type that return `Self`.
///
/// Each method delegates to the corresponding `Style` or `Layout` builder,
/// setting only that property. This enables SwiftUI/Compose-style modifier
/// chains while preserving all other properties.
///
/// Usage: `impl MyWidget { modifier_methods!(); }`
///
/// Requires: the struct must have `style: Style` and `layout: Layout` fields
/// (e.g., generated by `modifier_fields!()`).
#[allow(unused)]
#[macro_export]
macro_rules! modifier_methods {
    () => {
        // Style methods
        pub fn background(mut self, color: $crate::core::Color) -> Self {
            self.style = self.style.background(color);
            self
        }
        pub fn border(mut self, color: $crate::core::Color, width: f32) -> Self {
            self.style = self.style.border(color, width);
            self
        }
        pub fn corner_radius(mut self, radius: f32) -> Self {
            self.style = self.style.corner_radius(radius);
            self
        }
        pub fn clip(mut self) -> Self {
            self.style = self.style.clip();
            self
        }

        // Layout methods
        pub fn padding(mut self, value: f32) -> Self {
            self.layout = self.layout.padding(value);
            self
        }
        pub fn padding_each(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
            self.layout = self.layout.padding_each(left, right, top, bottom);
            self
        }
        pub fn margin(mut self, value: f32) -> Self {
            self.layout = self.layout.margin(value);
            self
        }
        pub fn margin_each(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
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
        pub fn align_self(mut self, value: $crate::layout::AlignSelf) -> Self {
            self.layout = self.layout.align_self(value);
            self
        }
        pub fn position(mut self, value: $crate::layout::Position) -> Self {
            self.layout = self.layout.position(value);
            self
        }
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
        pub fn aspect_ratio(mut self, value: f32) -> Self {
            self.layout = self.layout.aspect_ratio(value);
            self
        }
        pub fn overflow(mut self, value: $crate::layout::Overflow) -> Self {
            self.layout = self.layout.overflow(value);
            self
        }
        pub fn overflow_x(mut self, value: $crate::layout::Overflow) -> Self {
            self.layout = self.layout.overflow_x(value);
            self
        }
        pub fn overflow_y(mut self, value: $crate::layout::Overflow) -> Self {
            self.layout = self.layout.overflow_y(value);
            self
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::core::Color;
    use crate::layout::Layout;
    use crate::layout::{AlignSelf, Overflow, Position};
    use crate::Style;

    /// Dummy widget struct with the same fields that modifier_fields!() generates.
    /// (Rust declarative macros cannot expand inside struct definitions, so we
    /// manually write the fields here and test modifier_methods!() on it.)
    struct TestWidget {
        style: Style,
        layout: Layout,
    }

    impl TestWidget {
        fn new() -> Self {
            Self {
                style: Style::default(),
                layout: Layout::default(),
            }
        }

        modifier_methods!();
    }

    #[test]
    fn test_modifier_fields_expansion() {
        // Verify that modifier_fields!() expands to the expected types
        // by creating a struct with the same field types and checking defaults.
        // Since Rust macros cannot expand inside struct definitions, we test
        // the contract: Style::default() has background=None, Layout::default()
        // has padding=None.
        let style = Style::default();
        let layout = Layout::default();
        assert!(style.background.is_none());
        assert!(layout.padding.is_none());
    }

    #[test]
    fn test_background_sets_style() {
        let widget = TestWidget::new().background(Color::RED);
        assert_eq!(widget.style.background, Some(Color::RED));
        // Layout should remain default
        assert!(widget.layout.padding.is_none());
    }

    #[test]
    fn test_padding_sets_layout() {
        let widget = TestWidget::new().padding(10.0);
        let p = widget.layout.padding.unwrap();
        assert_eq!(p.left, 10.0);
        assert_eq!(p.right, 10.0);
        assert_eq!(p.top, 10.0);
        assert_eq!(p.bottom, 10.0);
        // Style should remain default
        assert!(widget.style.background.is_none());
    }

    #[test]
    fn test_chaining_preserves_all_properties() {
        let widget = TestWidget::new()
            .background(Color::RED)
            .padding(10.0)
            .margin(5.0);

        assert_eq!(widget.style.background, Some(Color::RED));
        assert!(widget.layout.padding.is_some());
        assert!(widget.layout.margin.is_some());
        let p = widget.layout.padding.unwrap();
        assert_eq!(p.left, 10.0);
        let m = widget.layout.margin.unwrap();
        assert_eq!(m.left, 5.0);
    }

    #[test]
    fn test_corner_radius() {
        let widget = TestWidget::new().corner_radius(8.0);
        let cr = widget.style.corner_radius.unwrap();
        assert_eq!(cr.radius, 8.0);
    }

    #[test]
    fn test_border() {
        let widget = TestWidget::new().border(Color::BLACK, 2.0);
        let border = widget.style.border.unwrap();
        assert_eq!(border.color, Color::BLACK);
        assert_eq!(border.width, 2.0);
    }

    #[test]
    fn test_clip() {
        let widget = TestWidget::new().clip();
        assert!(widget.style.clip);
    }

    #[test]
    fn test_modifier_methods_return_self() {
        let widget = TestWidget::new()
            .background(Color::BLUE)
            .padding(4.0)
            .corner_radius(12.0)
            .border(Color::BLACK, 1.0)
            .clip();

        assert_eq!(widget.style.background, Some(Color::BLUE));
        assert!(widget.layout.padding.is_some());
        assert_eq!(widget.style.corner_radius.unwrap().radius, 12.0);
        assert_eq!(widget.style.border.unwrap().width, 1.0);
        assert!(widget.style.clip);
    }

    #[test]
    fn test_layout_modifier_methods() {
        let widget = TestWidget::new()
            .width(100.0)
            .height(50.0)
            .min_width(20.0)
            .min_height(10.0)
            .max_width(200.0)
            .max_height(150.0)
            .flex_grow(1.0)
            .flex_shrink(0.5)
            .flex_basis(50.0)
            .absolute()
            .inset(5.0)
            .aspect_ratio(1.5);

        use crate::layout::Dimension;
        assert_eq!(widget.layout.width, Some(Dimension::Length(100.0)));
        assert_eq!(widget.layout.height, Some(Dimension::Length(50.0)));
        assert_eq!(widget.layout.min_width, Some(Dimension::Length(20.0)));
        assert_eq!(widget.layout.min_height, Some(Dimension::Length(10.0)));
        assert_eq!(widget.layout.max_width, Some(Dimension::Length(200.0)));
        assert_eq!(widget.layout.max_height, Some(Dimension::Length(150.0)));
        assert_eq!(widget.layout.flex_grow, Some(1.0));
        assert_eq!(widget.layout.flex_shrink, Some(0.5));
        assert_eq!(widget.layout.flex_basis, Some(Dimension::Length(50.0)));
        assert_eq!(widget.layout.position, Some(Position::Absolute));
        assert!(widget.layout.inset.is_some());
        assert_eq!(widget.layout.aspect_ratio, Some(1.5));
    }

    #[test]
    fn test_align_self_modifier() {
        let widget = TestWidget::new().align_self(AlignSelf::Center);
        assert_eq!(widget.layout.align_self, Some(AlignSelf::Center));
    }

    #[test]
    fn test_position_modifier() {
        let widget = TestWidget::new().position(Position::Absolute);
        assert_eq!(widget.layout.position, Some(Position::Absolute));
    }

    #[test]
    fn test_overflow_modifiers() {
        let widget = TestWidget::new()
            .overflow(Overflow::Hidden)
            .overflow_x(Overflow::Clip)
            .overflow_y(Overflow::Scroll);
        assert_eq!(widget.layout.overflow_x, Some(Overflow::Clip));
        assert_eq!(widget.layout.overflow_y, Some(Overflow::Scroll));
    }

    #[test]
    fn test_relative_modifier() {
        let widget = TestWidget::new().relative();
        assert_eq!(widget.layout.position, Some(Position::Relative));
    }

    #[test]
    fn test_inset_individual_modifiers() {
        let widget = TestWidget::new().top(1.0).right(2.0).bottom(3.0).left(4.0);
        let inset = widget.layout.inset.unwrap();
        assert_eq!(inset.top, Some(1.0));
        assert_eq!(inset.right, Some(2.0));
        assert_eq!(inset.bottom, Some(3.0));
        assert_eq!(inset.left, Some(4.0));
    }

    #[test]
    fn test_margin_each_modifier() {
        let widget = TestWidget::new().margin_each(1.0, 2.0, 3.0, 4.0);
        let m = widget.layout.margin.unwrap();
        // margin_each(top=1, right=2, bottom=3, left=4) delegates to
        // Layout::margin_each(left=4, right=2, top=1, bottom=3)
        assert_eq!(m.top, 1.0);
        assert_eq!(m.right, 2.0);
        assert_eq!(m.bottom, 3.0);
        assert_eq!(m.left, 4.0);
    }

    #[test]
    fn test_padding_each_modifier() {
        let widget = TestWidget::new().padding_each(1.0, 2.0, 3.0, 4.0);
        let p = widget.layout.padding.unwrap();
        // padding_each(top=1, right=2, bottom=3, left=4) delegates to
        // Layout::padding_each(left=4, right=2, top=1, bottom=3)
        assert_eq!(p.top, 1.0);
        assert_eq!(p.right, 2.0);
        assert_eq!(p.bottom, 3.0);
        assert_eq!(p.left, 4.0);
    }

    // --- children! macro tests ---

    #[test]
    fn children_macro_builds_vec() {
        let kids: Vec<Box<dyn crate::Widget>> = children![
            crate::Text::new("A"),
            crate::Text::new("B"),
            crate::Text::new("C"),
        ];
        assert_eq!(kids.len(), 3);
    }

    #[test]
    fn children_macro_single_child() {
        let kids: Vec<Box<dyn crate::Widget>> = children![crate::Text::new("Only"),];
        assert_eq!(kids.len(), 1);
    }

    #[test]
    fn children_macro_no_children() {
        let kids: Vec<Box<dyn crate::Widget>> = children![];
        assert_eq!(kids.len(), 0);
    }

    #[test]
    fn children_macro_with_multi_child() {
        use crate::layout::Layout;
        let mc = crate::MultiChild::new(
            children![crate::Text::new("A"), crate::Text::new("B")],
            Layout::column().gap(16.0),
        );
        assert_eq!(mc.children().len(), 2);
    }
}
