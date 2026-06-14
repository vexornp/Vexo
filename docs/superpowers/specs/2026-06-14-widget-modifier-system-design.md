# Widget Modifier System Design

## Problem

Vexo's wrapper widget pattern creates deeply nested, inside-out code. Adding decoration, layout, and behavior to a widget requires wrapping it in multiple container widgets:

```rust
Transform::translate(
    DecoratedContainer::new(Text::new("Shifted"))
        .background(Color::rgb(0.85, 0.9, 1.0))
        .padding(8.0),
    100.0, 100.0,
)
```

This reads outside-in (Transform wraps DecoratedContainer wraps Text) when the developer's mental model is inside-out ("a text that has a background, padding, and is translated").

SwiftUI and Compose solve this with modifier chains:

```swift
Text("Shifted")
    .background(Color.blue)
    .padding(8)
    .offset(x: 100, y: 100)
```

## Design

### Modifier Categories

Three categories of modifiers, two implementation strategies:

| Category | Examples | Implementation | Runtime cost |
|----------|----------|----------------|--------------|
| Decoration | `.background()`, `.border()`, `.corner_radius()`, `.clip()` | Property on same widget (Style field) | 0 extra nodes |
| Layout | `.padding()`, `.margin()`, `.width()`, `.height()`, `.flex_grow()`, `.align_self()`, etc. | Property on same widget (Layout field) | 0 extra nodes |
| Behavioral/Transform | `.on_press()`, `.on_release()`, `.cursor()`, `.on_enter()`, `.on_exit()`, `.translate()`, `.rotate()`, `.scale()` | Wrapper widget | 1 node each |

The property pattern is inspired by Xilem's `Prop` system: decoration and layout modifiers set fields on the widget itself, not on a wrapper. The widget's render object handles painting and layout for these properties directly.

### Widget Trait Extension

Add modifier methods to the `Widget` trait as default implementations. These serve as fallbacks when the type is already erased (`Box<dyn Widget>`). Each concrete widget provides its own inherent methods that shadow these defaults and return `Self` (see Chaining Semantics).

The trait defaults for decoration/layout modifiers wrap in `DecoratedContainer`/`WithLayout`. The trait defaults for behavioral/transform modifiers wrap in the corresponding wrapper widget:

```rust
pub trait Widget: Any {
    // ...existing methods...

    // Decoration modifier defaults (fallback for Box<dyn Widget>)
    fn background(self, color: Color) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(DecoratedContainer::new(self).background(color))
    }

    fn border(self, color: Color, width: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(DecoratedContainer::new(self).border(color, width))
    }

    fn corner_radius(self, radius: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(DecoratedContainer::new(self).corner_radius(radius))
    }

    fn clip(self) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(DecoratedContainer::new(self).clip())
    }

    // Layout modifier defaults (fallback for Box<dyn Widget>)
    fn padding(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().padding(value)))
    }

    fn margin(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().margin(value)))
    }

    fn width(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().width(value)))
    }

    fn height(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().height(value)))
    }

    fn flex_grow(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().flex_grow(value)))
    }

    fn flex_shrink(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().flex_shrink(value)))
    }

    fn align_self(self, value: AlignSelf) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().align_self(value)))
    }

    fn position(self, value: Position) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().position(value)))
    }

    fn inset(self, value: Inset) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().inset(value)))
    }

    fn aspect_ratio(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().aspect_ratio(value)))
    }

    fn overflow_x(self, value: Overflow) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().overflow_x(value)))
    }

    fn overflow_y(self, value: Overflow) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().overflow_y(value)))
    }

    // Behavioral modifiers (always wrap — these are the primary impls, not fallbacks)
    fn on_press(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(GestureDetector::new(self).on_press(callback))
    }

    fn on_release(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(GestureDetector::new(self).on_release(callback))
    }

    fn cursor(self, cursor: MouseCursor) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(MouseRegion::new(self).cursor(cursor))
    }

    fn on_enter(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(MouseRegion::new(self).on_enter(callback))
    }

    fn on_exit(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(MouseRegion::new(self).on_exit(callback))
    }

    // Transform modifiers (always wrap — these are the primary impls, not fallbacks)
    fn translate(self, dx: f32, dy: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(Transform::translate(self, dx, dy))
    }

    fn rotate(self, radians: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(Transform::rotate(self, radians))
    }

    fn scale(self, sx: f32, sy: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(Transform::scale(self, sx, sy))
    }
}
```

### Decoration + Layout Properties on All Widgets

Every widget struct gains `style: Style` and `layout: Layout` fields:

```rust
pub struct Text {
    content: String,
    style: Style,      // background, border, corner_radius, clip
    layout: Layout,    // padding, margin, width, height, flex, etc.
}
```

The Widget trait default implementations for decoration/layout modifiers clone the widget, set the field, and box it:

```rust
fn background(mut self, color: Color) -> Box<dyn Widget>
where Self: Sized + 'static {
    self.style = self.style.background(color);
    Box::new(self)
}
```

For widgets that already have `style`/`layout` fields (DecoratedContainer, Flex, Grid, WithLayout), the same default implementation works — it sets the field on the concrete type before boxing.

**Alternative: macros for boilerplate reduction.** Since every widget needs `style` and `layout` fields plus the same modifier implementations, a macro can generate both:

```rust
// In each widget file:
modifier_fields!();  // generates style: Style, layout: Layout fields

// In Widget trait, a single macro generates all default impls:
modifier_defaults!();
```

This is optional — the pattern works without macros, they just reduce repetition.

### Render Object Unification

Merge `ContainerRenderObject`, `DecoratedContainerRenderObject`, and `WithLayoutRenderObject` into a single `ContainerRenderObject` that handles both layout and decoration:

```rust
pub struct ContainerRenderObject {
    layout: Layout,
    style: Style,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

`WithLayoutRenderObject` is removed entirely (it's now just a `ContainerRenderObject` with default style). `DecoratedContainerRenderObject` is removed (same thing).

`TextRenderObject` and `TextEditRenderObject` gain `Style` and `Layout` fields so they can handle decoration and layout properties set on Text/TextEdit widgets directly:

```rust
pub struct TextRenderObject {
    // ...existing fields...
    style: Style,      // NEW
    layout: Layout,    // NEW
}
```

Render object catalog after unification:

| Render Object | Used by | Handles |
|---------------|---------|---------|
| `TextRenderObject` | Text | text + layout + decoration |
| `TextEditRenderObject` | TextEdit | text edit + layout + decoration |
| `ContainerRenderObject` | Flex, Grid | flex/grid layout + decoration |
| `TransformRenderObject` | Transform | affine transform (wraps child) |
| `GestureDetectorRenderObject` | GestureDetector | pass-through (event handling) |
| `MouseRegionRenderObject` | MouseRegion | pass-through (cursor/hover) |

### Wrapper Widget Visibility

`DecoratedContainer`, `WithLayout`, `Transform`, `GestureDetector`, and `MouseRegion` become `pub(crate)`. They remain as implementation details for behavioral/transform modifiers, but users never write them directly.

The public API surface:

- **Leaf widgets**: `Text`, `TextEdit`
- **Container widgets**: `Flex`, `Grid`
- **Semantic widgets**: `Focus` (stays public)
- **Modifier methods**: all on `Widget` trait
- **Direct construction**: Flex/Grid still have their own builder methods (`.gap()`, `.columns()`, etc.) that return `Self`

### Chaining Semantics

Decoration and layout modifiers set properties on the same widget. Behavioral and transform modifiers wrap in a new widget. This means:

```rust
Text::new("Hello")
    .padding(8.0)            // Sets layout on Text → Box<dyn Widget>
    .background(Color::RED)  // Sets style on Text → Box<dyn Widget>
    .on_press(|| {})         // Wraps in GestureDetector → Box<dyn Widget>
```

The element tree for this chain: `GestureDetectorElement` → `TextElement` (with style+layout). Two elements, not four.

After the first `Box<dyn Widget>` return, subsequent calls go through the `Widget` trait impl on `Box<dyn Widget>`. The default implementations for decoration/layout modifiers on `Box<dyn Widget>` need a different strategy since we can't mutate the inner concrete type. Two options:

1. **Clone-and-set**: The default impl calls `clone_boxed()`, downcasts, sets the field, re-boxes. Requires each widget to implement a `set_style`/`set_layout` method.
2. **Wrap in DecoratedContainer/WithLayout**: When called on `Box<dyn Widget>` (type-erased), fall back to wrapping. This is the safe default — it works for any widget type without downcasting.

**Recommendation**: Option 2 (wrap on type-erased). When the concrete type is known (first call in a chain from `Text`, `Flex`, etc.), the widget-specific impl sets the field directly. Once type-erased to `Box<dyn Widget>`, the fallback wraps in `DecoratedContainer`/`WithLayout`. This means:

```rust
Text::new("Hello")
    .padding(8.0)            // Text has layout field → sets directly
    .background(Color::RED)  // Box<dyn Widget> → wraps in DecoratedContainer
```

This creates one extra node for the `DecoratedContainer` in this specific pattern. To avoid it, users can reorder:

```rust
Text::new("Hello")
    .background(Color::RED)  // Text has style field → sets directly
    .padding(8.0)            // Text has layout field → sets directly
```

Or the macro can generate impls for each concrete widget that return the concrete type (not `Box<dyn Widget>`) until a behavioral/transform modifier boxes it. This preserves the monomorphic chain as long as possible.

**Refined recommendation**: Each concrete widget (Text, Flex, Grid, etc.) implements its own modifier methods that return `Self`, setting fields directly. The `Widget` trait has default implementations returning `Box<dyn Widget>` as fallback. When chaining from a concrete type, Rust resolves to the concrete impl first (it shadows the trait default). After boxing (from a behavioral/transform modifier), the trait default kicks in and wraps.

**Method resolution rule**: Rust prefers inherent methods over trait methods. So `Text::background()` (inherent, returns `Self`) shadows `Widget::background()` (trait, returns `Box<dyn Widget>`). The trait default only fires when the type is already `Box<dyn Widget>` — i.e., after a behavioral/transform modifier has boxed it. This means:

```rust
// On Text specifically:
impl Text {
    fn background(mut self, color: Color) -> Self {
        self.style = self.style.background(color);
        self
    }
    fn padding(mut self, value: f32) -> Self {
        self.layout = self.layout.padding(value);
        self
    }
}

// Chain:
Text::new("Hello")
    .background(Color::RED)  // Text::background → Text (inherent method, returns Self)
    .padding(8.0)            // Text::padding → Text (inherent method, returns Self)
    .on_press(|| {})         // Widget::on_press → Box<dyn Widget> (trait method, wraps in GestureDetector)

// After boxing, trait defaults take over:
Text::new("Hello")
    .on_press(|| {})         // Widget::on_press → Box<dyn Widget>
    .background(Color::RED)  // Widget::background → Box<dyn Widget> (wraps in DecoratedContainer)
    .padding(8.0)            // Widget::padding → Box<dyn Widget> (wraps in WithLayout)
```

The second chain creates 4 nodes (GestureDetector → DecoratedContainer → WithLayout → Text). The first chain creates 2 nodes (GestureDetector → Text). Users naturally write the first pattern (decorate first, then add behavior), which is the zero-cost path.

This is zero-cost for decoration+layout modifiers on concrete types. Only behavioral/transform modifiers create wrapper nodes.

### Before → After Examples

**Button helper**:
```rust
// Before
fn tap_button(label: &str, on_press: impl FnMut() + 'static) -> GestureDetector {
    GestureDetector::new(
        DecoratedContainer::new(Text::new(label))
            .background(Color::rgb(0.9, 0.9, 0.9))
            .border(Color::rgb(0.6, 0.6, 0.6), 1.0)
            .corner_radius(8.0)
            .padding(24.0),
    )
    .on_press(on_press)
}

// After
fn tap_button(label: &str, on_press: impl FnMut() + 'static) -> Box<dyn Widget> {
    Text::new(label)
        .background(Color::rgb(0.9, 0.9, 0.9))
        .border(Color::rgb(0.6, 0.6, 0.6), 1.0)
        .corner_radius(8.0)
        .padding(24.0)
        .on_press(on_press)
}
```

**Hoverable card**:
```rust
// Before
Box::new(
    MouseRegion::new(
        DecoratedContainer::new(column)
            .background(Color::rgb(0.95, 0.95, 1.0))
            .border(border_color, border_width)
            .corner_radius(8.0)
            .padding(8.0),
    )
    .cursor(MouseCursor::System(SystemCursorKind::Pointer))
    .on_enter(...)
    .on_exit(...),
)

// After
column
    .background(Color::rgb(0.95, 0.95, 1.0))
    .border(border_color, border_width)
    .corner_radius(8.0)
    .padding(8.0)
    .cursor(MouseCursor::System(SystemCursorKind::Pointer))
    .on_enter(...)
    .on_exit(...)
```

**Transform**:
```rust
// Before
Transform::translate(
    DecoratedContainer::new(Text::new("Shifted"))
        .background(Color::rgb(0.85, 0.9, 1.0))
        .padding(8.0),
    100.0, 100.0,
)

// After
Text::new("Shifted")
    .background(Color::rgb(0.85, 0.9, 1.0))
    .padding(8.0)
    .translate(100.0, 100.0)
```

### Testing Strategy

- Unit tests for each modifier method on concrete widget types (Text, Flex, Grid)
- Unit tests for modifier chaining (decoration+layout chain, behavioral+transform chain, mixed chain)
- Integration tests verifying modifier chains produce the same visual output as equivalent wrapper widget construction
- Migration test: rewrite `shared_app` using modifiers, verify it compiles and renders identically

### What This Does NOT Change

- `Flex` and `Grid` retain their own builder methods (`.gap()`, `.columns()`, `.rows()`, etc.) for direct construction
- `StatefulWidget` and `StatefulMutable` are unaffected
- `Focus` widget stays public — it's a semantic widget, not a modifier
- The three-tree architecture is unchanged — modifiers are a widget-layer ergonomics improvement
- Element and render object internals are unchanged — we're adding fields to existing render objects and removing redundant ones, not changing the architecture
