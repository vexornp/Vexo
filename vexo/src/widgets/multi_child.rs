//! MultiChild widget — a multi-child container with a user-supplied `Layout`.
//!
//! `MultiChild` is the Vexo replacement for `Flex`/`Column`/`Row`. It holds
//! N children and applies a `Layout` (flexbox, grid, or block) to them.
//! Unlike the old `Flex`, it has no `Style` field — decoration goes on
//! `DecoratedBox`.
//!
//! # Example
//!
//! ```ignore
//! use vexo::{MultiChild, Layout, Text};
//!
//! MultiChild::new(
//!     vec![Text::new("A").boxed(), Text::new("B").boxed()],
//!     Layout::column().gap(16.0),
//! )
//! ```

use super::container::ChildPush;
use super::{Element, Widget};
use crate::core::{Logical, Size};
use crate::key::WidgetKey;
use crate::layout::{
    AlignContent, AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, GridAutoFlow,
    GridPlacement, JustifyContent, Layout, Overflow, Position, TrackSizing,
};
use crate::render_objects::ContainerRenderObject;
use crate::{RenderObject, UpdateResult};

/// A multi-child container with a user-supplied `Layout`.
///
/// The replacement for `Flex`/`Column`/`Row`. Pass a `Layout::column()`,
/// `Layout::row()`, `Layout::grid()`, or `Layout::default()` (block) to
/// control how children are arranged. For decoration (background, border,
/// etc.), wrap in `DecoratedBox`.
pub struct MultiChild {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}

/// Generate fluent `Layout` passthrough methods on `MultiChild`.
///
/// Each entry `$method($args)` becomes `pub fn $method(mut self, $args) -> Self`
/// that delegates to `self.layout.$method($args)`, mutating `self.layout` in
/// place. Names and signatures mirror `Layout`'s instance builders exactly
/// (`vexo/src/layout/style.rs`), so the API reads identically to `Layout`'s.
macro_rules! impl_layout_passthrough {
    ($($method:ident($($arg:ident: $ty:ty),*)),* $(,)?) => {
        $(
            #[doc = concat!("Set [`Layout::", stringify!($method), "`] on this container's layout.")]
            #[doc = ""]
            #[doc = concat!("Mirrors `Layout::", stringify!($method), "`; modifies the existing layout in place,")]
            #[doc = "preserving other fields (e.g. the `column`/`row` direction set by `column!`/`row!`)."]
            pub fn $method(mut self, $($arg: $ty),*) -> Self {
                self.layout = self.layout.$method($($arg),*);
                self
            }
        )*
    };
}

impl MultiChild {
    /// Create a new `MultiChild` with the given children and layout.
    pub fn new(children: Vec<Box<dyn Widget>>, layout: Layout) -> Self {
        Self {
            key: None,
            children,
            layout,
        }
    }

    /// Create an empty `MultiChild` with the given layout; add children via `.push()`.
    pub fn empty(layout: Layout) -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Replace the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Add a child widget.
    ///
    /// Accepts any `impl Widget` or `Option<Box<dyn Widget>>` (for conditional children).
    pub fn push(mut self, child: impl ChildPush + 'static) -> Self {
        child.push_into(&mut self.children);
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Get the layout.
    pub fn layout_ref(&self) -> &Layout {
        &self.layout
    }

    impl_layout_passthrough! {
        // Box model
        padding(value: f32),
        padding_each(left: f32, right: f32, top: f32, bottom: f32),
        margin(value: f32),
        margin_each(left: f32, right: f32, top: f32, bottom: f32),
        width(value: f32),
        height(value: f32),
        width_percent(value: f32),
        height_percent(value: f32),
        min_width(value: f32),
        min_height(value: f32),
        max_width(value: f32),
        max_height(value: f32),

        // Flexbox
        flex_direction(value: FlexDirection),
        flex_wrap(),
        flex_wrap_mode(value: FlexWrap),
        flex_grow(value: f32),
        flex_shrink(value: f32),
        flex_basis(value: f32),
        justify(value: JustifyContent),
        align(value: AlignItems),
        align_content(value: AlignContent),
        gap(value: f32),
        gap_size(size: Size<Logical>),
        gap_each(width: f32, height: f32),

        // Grid
        columns(sizes: Vec<TrackSizing>),
        rows(sizes: Vec<TrackSizing>),
        grid_column(placement: GridPlacement),
        grid_row(placement: GridPlacement),
        grid_auto_flow(value: GridAutoFlow),
        auto_rows(sizes: Vec<TrackSizing>),
        auto_columns(sizes: Vec<TrackSizing>),

        // Positioning
        absolute(),
        relative(),
        position(value: Position),
        inset(value: f32),
        top(value: f32),
        right(value: f32),
        bottom(value: f32),
        left(value: f32),

        // Per-item alignment
        align_self(value: AlignSelf),

        // Display
        display(value: Display),

        // Sizing
        aspect_ratio(value: f32),

        // Overflow
        overflow(value: Overflow),
        overflow_x(value: Overflow),
        overflow_y(value: Overflow),
    }
}

impl Default for MultiChild {
    fn default() -> Self {
        Self::empty(Layout::default())
    }
}

impl Clone for MultiChild {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for MultiChild {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::elements::ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new(self.layout.clone()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(container_ro) = render_object
            .as_any_mut()
            .downcast_mut::<ContainerRenderObject>()
        {
            if container_ro.set_layout(self.layout.clone()) {
                UpdateResult::LAYOUT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Size;
    use crate::layout::{FlexDirection, JustifyContent, Layout, Overflow, Position, TrackSizing};
    use crate::Text;

    #[test]
    fn test_multi_child_new_with_children() {
        let mc = MultiChild::new(
            vec![Text::new("A").boxed(), Text::new("B").boxed()],
            Layout::column(),
        );
        assert_eq!(mc.children().len(), 2);
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    }

    #[test]
    fn test_multi_child_empty_then_push() {
        let mc = MultiChild::empty(Layout::column())
            .push(Text::new("A"))
            .push(Text::new("B"));
        assert_eq!(mc.children().len(), 2);
    }

    #[test]
    fn test_multi_child_with_key() {
        let mc = MultiChild::empty(Layout::column()).with_key("my-mc");
        assert_eq!(mc.key(), Some(WidgetKey::Local(crate::Key::new("my-mc"))));
    }

    #[test]
    fn test_multi_child_with_layout_replaces() {
        let mc = MultiChild::empty(Layout::column()).with_layout(Layout::row().gap(8.0));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
        assert!(mc.layout_ref().gap.is_some());
    }

    #[test]
    fn test_multi_child_clone() {
        let mc = MultiChild::new(vec![Text::new("A").boxed()], Layout::column().gap(16.0));
        let cloned = mc.clone();
        assert_eq!(cloned.children().len(), 1);
        assert!(cloned.layout_ref().gap.is_some());
    }

    #[test]
    fn test_multi_child_creates_container_render_object() {
        let mc = MultiChild::empty(Layout::column());
        let ro = mc.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<ContainerRenderObject>()
            .is_some());
    }

    #[test]
    fn test_multi_child_update_render_object_layout_change() {
        let mc1 = MultiChild::empty(Layout::default().padding(10.0));
        let mc2 = MultiChild::empty(Layout::default().padding(20.0));
        let mut ro = ContainerRenderObject::new(Layout::default().padding(10.0));
        assert_eq!(mc1.update_render_object(&mut ro), UpdateResult::NONE);
        assert!(mc2
            .update_render_object(&mut ro)
            .contains(UpdateResult::LAYOUT));
    }

    #[test]
    fn fluent_gap_preserves_column_direction() {
        let mc = MultiChild::empty(Layout::column()).gap(8.0);
        assert_eq!(mc.layout_ref().gap, Some(Size::new(8.0, 8.0)));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    }

    #[test]
    fn fluent_padding_preserves_row_direction() {
        let mc = MultiChild::empty(Layout::row()).padding(12.0);
        let p = mc.layout_ref().padding.unwrap();
        assert_eq!(p.top, 12.0);
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
    }

    #[test]
    fn fluent_flex_shrink_preserves_direction() {
        let mc = MultiChild::empty(Layout::row()).flex_shrink(0.0);
        assert_eq!(mc.layout_ref().flex_shrink, Some(0.0));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
    }

    #[test]
    fn fluent_justify_overrides_default() {
        let mc = MultiChild::empty(Layout::column()).justify(JustifyContent::SpaceBetween);
        assert_eq!(
            mc.layout_ref().justify_content,
            Some(JustifyContent::SpaceBetween)
        );
    }

    #[test]
    fn fluent_columns_sets_grid_template() {
        let mc = MultiChild::empty(Layout::grid())
            .columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)]);
        let cols = mc.layout_ref().grid_template_columns.as_ref().unwrap();
        assert_eq!(cols.len(), 2);
    }

    #[test]
    fn fluent_absolute_top_sets_position_and_inset() {
        let mc = MultiChild::empty(Layout::default()).absolute().top(10.0);
        assert_eq!(mc.layout_ref().position, Some(Position::Absolute));
        assert_eq!(mc.layout_ref().inset.unwrap().top, Some(10.0));
    }

    #[test]
    fn fluent_overflow_sets_both_axes() {
        let mc = MultiChild::empty(Layout::default()).overflow(Overflow::Hidden);
        assert_eq!(mc.layout_ref().overflow_x, Some(Overflow::Hidden));
        assert_eq!(mc.layout_ref().overflow_y, Some(Overflow::Hidden));
    }

    #[test]
    fn fluent_chaining_sets_all_three() {
        let mc = MultiChild::empty(Layout::column())
            .gap(8.0)
            .padding(12.0)
            .flex_shrink(0.0);
        assert_eq!(mc.layout_ref().gap, Some(Size::new(8.0, 8.0)));
        assert!(mc.layout_ref().padding.is_some());
        assert_eq!(mc.layout_ref().flex_shrink, Some(0.0));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    }

    #[test]
    fn fluent_flex_direction_overrides_column_default() {
        // Start with Layout::column() (what column! would set); calling
        // .flex_direction(Row) overrides it. No error — methods are low-level
        // setters, user intent honored. Macro integration is covered by the
        // integration test in vexo/tests/builder_macros.rs.
        let mc = MultiChild::new(vec![Text::new("A").boxed()], Layout::column())
            .flex_direction(FlexDirection::Row);
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
    }
}
