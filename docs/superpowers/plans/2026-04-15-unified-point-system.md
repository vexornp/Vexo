# Unified Point System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a unified type-safe point system for logical and physical coordinates, fixing the ColorWidget double-scaling bug.

**Architecture:** Generic types `Point<T>`, `Size<T>`, `Rect<T>` with marker types `Logical` and `Physical` for compile-time safety. Conversion methods on types. Update all widgets and renderer to use typed coordinates.

**Tech Stack:** Rust, wgpu, Taffy, glyphon

---

## Task 1: Add Core Point Types

**Files:**
- Modify: `vexo/src/utils.rs`

- [ ] **Step 1: Add marker types and generic Point/Size/Rect structs**

Add after line 4 (after `use winit::dpi::LogicalPosition;`):

```rust
// ============================================================================
// UNIFIED POINT SYSTEM
// ============================================================================

/// Marker type for logical (DPI-independent) coordinates
pub struct Logical;

/// Marker type for physical (screen pixel) coordinates
pub struct Physical;

/// A 2D point in either logical or physical coordinates
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T> {
    pub x: f32,
    pub y: f32,
    _marker: std::marker::PhantomData<T>,
}

/// A 2D size in either logical or physical coordinates
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T> {
    pub width: f32,
    pub height: f32,
    _marker: std::marker::PhantomData<T>,
}

/// A rectangle with origin and size in either logical or physical coordinates
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<T> {
    pub origin: Point<T>,
    pub size: Size<T>,
    _marker: std::marker::PhantomData<T>,
}
```

- [ ] **Step 2: Add constructors for Point, Size, Rect**

Add after the struct definitions:

```rust
// ============================================================================
// CONSTRUCTORS
// ============================================================================

impl<T> Point<T> {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Size<T> {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Rect<T> {
    pub fn new(origin: Point<T>, size: Size<T>) -> Self {
        Self {
            origin,
            size,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new(Point::new(x, y), Size::new(width, height))
    }
}
```

- [ ] **Step 3: Add conversion methods for Point**

Add after constructors:

```rust
// ============================================================================
// CONVERSIONS
// ============================================================================

impl Point<Logical> {
    /// Convert logical point to physical pixels
    pub fn to_physical(self, scale: f32) -> Point<Physical> {
        Point::new(self.x * scale, self.y * scale)
    }

    /// Convert from Taffy's Point type
    pub fn from_taffy(p: taffy::Point<f32>) -> Self {
        Point::new(p.x, p.y)
    }

    /// Convert to Taffy's Point type
    pub fn to_taffy(self) -> taffy::Point<f32> {
        taffy::Point { x: self.x, y: self.y }
    }

    /// Convert to array for GPU buffers
    pub fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

impl Point<Physical> {
    /// Convert physical point to logical coordinates
    pub fn to_logical(self, scale: f32) -> Point<Logical> {
        Point::new(self.x / scale, self.y / scale)
    }
}
```

- [ ] **Step 4: Add conversion methods for Size**

Add after Point conversions:

```rust
impl Size<Logical> {
    /// Convert logical size to physical pixels
    pub fn to_physical(self, scale: f32) -> Size<Physical> {
        Size::new(self.width * scale, self.height * scale)
    }

    /// Convert from Taffy's Size type
    pub fn from_taffy(s: taffy::Size<f32>) -> Self {
        Size::new(s.width, s.height)
    }

    /// Convert to Taffy's Size type
    pub fn to_taffy(self) -> taffy::Size<f32> {
        taffy::Size { width: self.width, height: self.height }
    }

    /// Convert to array for GPU buffers
    pub fn to_array(self) -> [f32; 2] {
        [self.width, self.height]
    }
}

impl Size<Physical> {
    /// Convert physical size to logical coordinates
    pub fn to_logical(self, scale: f32) -> Size<Logical> {
        Size::new(self.width / scale, self.height / scale)
    }
}
```

- [ ] **Step 5: Add conversion methods for Rect**

Add after Size conversions:

```rust
impl Rect<Logical> {
    /// Convert logical rect to physical pixels
    pub fn to_physical(self, scale: f32) -> Rect<Physical> {
        Rect::new(
            self.origin.to_physical(scale),
            self.size.to_physical(scale),
        )
    }

    /// Create from layout result
    pub fn from_layout(location: taffy::Point<f32>, size: taffy::Size<f32>) -> Self {
        Rect::new(Point::from_taffy(location), Size::from_taffy(size))
    }
}

impl Rect<Physical> {
    /// Convert physical rect to logical coordinates
    pub fn to_logical(self, scale: f32) -> Rect<Logical> {
        Rect::new(
            self.origin.to_logical(scale),
            self.size.to_logical(scale),
        )
    }
}
```

- [ ] **Step 6: Add arithmetic operators for Point**

Add after conversions:

```rust
// ============================================================================
// ARITHMETIC
// ============================================================================

impl<T> std::ops::Add for Point<T> {
    type Output = Point<T>;

    fn add(self, other: Point<T>) -> Point<T> {
        Point::new(self.x + other.x, self.y + other.y)
    }
}

impl<T> std::ops::AddAssign for Point<T> {
    fn add_assign(&mut self, other: Point<T>) {
        self.x += other.x;
        self.y += other.y;
    }
}
```

- [ ] **Step 7: Update PhysicalLocation to use Point<Physical>**

Replace the existing `PhysicalLocation` struct and impl (lines 42-67) with:

```rust
pub struct PhysicalLocation(Point<Physical>);

impl PhysicalLocation {
    pub fn new(pos: winit::dpi::PhysicalPosition<f64>) -> Self {
        Self(Point::new(pos.x as f32, pos.y as f32))
    }

    pub fn default() -> Self {
        Self(Point::new(0.0, 0.0))
    }

    pub fn x(&self) -> f64 {
        self.0.x as f64
    }

    pub fn y(&self) -> f64 {
        self.0.y as f64
    }

    pub fn to_logical(self, scale: &Scale) -> Point<Logical> {
        self.0.to_logical(scale.factor())
    }

    fn to_taffy_point(&self, scale: &Scale) -> taffy::Point<f32> {
        self.to_logical(scale).to_taffy()
    }
}
```

- [ ] **Step 8: Update is_location_inside_quad to use Point**

Replace the existing `is_location_inside_quad` function (lines 70-83) with:

```rust
pub fn is_location_inside_quad(
    location: &PhysicalLocation,
    scale: &Scale,
    quad: &TaffyQuad,
) -> bool {
    let logical_pos = location.to_logical(scale);
    let x = logical_pos.x;
    let y = logical_pos.y;

    x >= quad.location.x
        && x <= quad.location.x + quad.size.width
        && y >= quad.location.y
        && y <= quad.location.y + quad.size.height
}
```

- [ ] **Step 9: Verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles without errors

- [ ] **Step 10: Commit**

```bash
git add vexo/src/utils.rs
git commit -m "feat: add unified Point, Size, Rect types with logical/physical markers"
```

---

## Task 2: Update Renderer Types

**Files:**
- Modify: `vexo/src/renderer.rs`
- Modify: `vexo/src/quad_instance.rs`

- [ ] **Step 1: Update TextRequest to use Point**

In `vexo/src/renderer.rs`, replace `TextRequest` struct (lines 23-28) with:

```rust
pub struct TextRequest {
    pub content: String,
    pub position: crate::utils::Point<crate::utils::Logical>,
    pub size: f32,
    pub color: [f32; 4],
}
```

- [ ] **Step 2: Replace Bounds with Rect<Logical>**

In `vexo/src/renderer.rs`, replace `Bounds` struct (lines 30-35) with:

```rust
pub type Bounds = crate::utils::Rect<crate::utils::Logical>;
```

- [ ] **Step 3: Update UiBatcher::add_text signature**

In `vexo/src/renderer.rs`, replace `add_text` method (lines 102-117) with:

```rust
pub fn add_text(
    &mut self,
    content: String,
    position: crate::utils::Point<crate::utils::Logical>,
    size: f32,
    color: impl Into<Color>,
) {
    let color: Color = color.into();
    self.text_requests.push(TextRequest {
        content,
        position,
        size,
        color: color.to_array(),
    });
}
```

- [ ] **Step 4: Add helper to QuadInstance**

In `vexo/src/quad_instance.rs`, add after line 10:

```rust
impl QuadInstance {
    /// Create a QuadInstance from logical coordinates
    pub fn from_logical(
        pos: crate::utils::Point<crate::utils::Logical>,
        size: crate::utils::Size<crate::utils::Logical>,
        color: crate::Color,
        border_color: crate::Color,
        border_width: f32,
    ) -> Self {
        Self {
            position: pos.to_array(),
            size: size.to_array(),
            color: color.to_array(),
            border_color: border_color.to_array(),
            border_width,
            _padding: [0.0; 3],
        }
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles without errors

- [ ] **Step 6: Commit**

```bash
git add vexo/src/renderer.rs vexo/src/quad_instance.rs
git commit -m "refactor: update renderer to use Point and Size types"
```

---

## Task 3: Update Widget Trait Signature

**Files:**
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Update Widget trait draw method signature**

Find the `Widget` trait definition and update the `draw` method signature to use `Point<Logical>` for offset. The trait is around line 720. Change:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: (f32, f32),
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
);
```

To:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: crate::utils::Point<crate::utils::Logical>,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
);
```

- [ ] **Step 2: Update Widget trait on_event method signature**

Similarly update `on_event` to use `Point<Logical>` for offset:

```rust
fn on_event(
    &mut self,
    taffy: &taffy::TaffyTree,
    node: NodeId,
    offset: crate::utils::Point<crate::utils::Logical>,
    event: &winit::event::WindowEvent,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) -> WidgetResponse<M>;
```

- [ ] **Step 3: Update root widget draw call**

In `FrameworkState::render()` method (around line 410), update the draw call:

```rust
self.root_widget.draw(
    &mut self.taffy,
    self.root_node_id,
    &mut self.batcher,
    crate::utils::Point::new(0.0, 0.0),
    self.focused_widget_id,
    &mut self.widget_context,
);
```

- [ ] **Step 4: Verify compilation**

Run: `cargo build -p vexo`
Expected: Errors about trait impl mismatches (expected - widgets need updating)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/lib.rs
git commit -m "refactor: update Widget trait to use Point for offset"
```

---

## Task 4: Update ColorWidget (Fix Double-Scaling Bug)

**Files:**
- Modify: `vexo/src/widgets/color_widget.rs`

- [ ] **Step 1: Update draw method to use Point and fix bug**

Replace the entire `draw` method (lines 53-88) with:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: crate::utils::Point<crate::utils::Logical>,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) {
    use crate::utils::{Point, Size};

    let layout = taffy.layout(node).unwrap();

    // Calculate absolute position by adding offset to layout location
    let x = offset.x + layout.location.x;
    let y = offset.y + layout.location.y;

    // Pass LOGICAL coordinates - shader handles conversion to physical
    let pos = Point::new(x, y);
    let size = Size::new(layout.size.width, layout.size.height);

    let border_color = crate::Color::WHITE;
    let border_width = 1.0;

    renderer.add_rect(pos.to_array(), size.to_array(), self.color, border_color, border_width);
}
```

- [ ] **Step 2: Update on_event method signature**

Update the `on_event` method signature (lines 90-100):

```rust
fn on_event(
    &mut self,
    taffy: &taffy::TaffyTree,
    node: NodeId,
    offset: crate::utils::Point<crate::utils::Logical>,
    event: &winit::event::WindowEvent,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) -> WidgetResponse<M> {
    WidgetResponse::default()
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: ColorWidget compiles, other widgets still have errors

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/color_widget.rs
git commit -m "fix: ColorWidget double-scaling bug, use Point type"
```

---

## Task 5: Update Container Widgets (Column, Row)

**Files:**
- Modify: `vexo/src/widgets/containers.rs`

- [ ] **Step 1: Update Column::draw method**

Replace the `draw` method (lines 77-109) with:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: crate::utils::Point<crate::utils::Logical>,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) {
    use crate::utils::{Point, Size};

    let layout = taffy.layout(node).unwrap();
    let my_offset = Point::new(
        offset.x + layout.location.x,
        offset.y + layout.location.y,
    );

    let pos = my_offset;
    let size = Size::new(layout.size.width, layout.size.height);
    let color = [0.8, 0.8, 0.8, 1.0];
    let border_color = [0.0, 0.0, 0.0, 1.0];
    let border_width = 2.0;

    renderer.add_rect(pos.to_array(), size.to_array(), color, border_color, border_width);

    let child_ids = taffy.children(node).unwrap();
    for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
        child_widget.draw(
            taffy,
            child_node_id,
            renderer,
            my_offset,
            focused_id,
            ctx,
        );
    }
}
```

- [ ] **Step 2: Update Column::on_event method**

Replace the `on_event` method (lines 111-136) with:

```rust
fn on_event(
    &mut self,
    taffy: &taffy::TaffyTree,
    node: NodeId,
    offset: crate::utils::Point<crate::utils::Logical>,
    event: &winit::event::WindowEvent,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) -> WidgetResponse<M> {
    use crate::utils::Point;

    let child_ids = taffy.children(node).unwrap();
    let layout = taffy.layout(node).unwrap();
    let my_offset = Point::new(
        offset.x + layout.location.x,
        offset.y + layout.location.y,
    );

    for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
        let child_response =
            child.on_event(taffy, child_node_id, my_offset, event, focused_id, ctx);

        if child_response.handled || child_response.focus_request.is_some() {
            return child_response;
        }
    }
    WidgetResponse::default()
}
```

- [ ] **Step 3: Update Row::draw method**

Replace the `draw` method (lines 200-223) with:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: crate::utils::Point<crate::utils::Logical>,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) {
    use crate::utils::Point;

    let layout = taffy.layout(node).unwrap();
    let my_offset = Point::new(
        offset.x + layout.location.x,
        offset.y + layout.location.y,
    );

    let child_ids = taffy.children(node).unwrap();
    for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
        child_widget.draw(
            taffy,
            child_node_id,
            renderer,
            my_offset,
            focused_id,
            ctx,
        );
    }
}
```

- [ ] **Step 4: Update Row::on_event method**

Replace the `on_event` method (lines 225-250) with:

```rust
fn on_event(
    &mut self,
    taffy: &taffy::TaffyTree,
    node: NodeId,
    offset: crate::utils::Point<crate::utils::Logical>,
    event: &winit::event::WindowEvent,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) -> WidgetResponse<M> {
    use crate::utils::Point;

    let child_ids = taffy.children(node).unwrap();
    let layout = taffy.layout(node).unwrap();
    let my_offset = Point::new(
        offset.x + layout.location.x,
        offset.y + layout.location.y,
    );

    for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
        let child_response =
            child.on_event(taffy, child_node_id, my_offset, event, focused_id, ctx);

        if child_response.handled || child_response.focus_request.is_some() {
            return child_response;
        }
    }
    WidgetResponse::default()
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo build -p vexo`
Expected: containers.rs compiles

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/containers.rs
git commit -m "refactor: update Column and Row to use Point type"
```

---

## Task 6: Update Button Widget

**Files:**
- Modify: `vexo/src/widgets/button.rs`

- [ ] **Step 1: Update Button::draw method**

Replace the `draw` method (lines 77-114) with:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: crate::utils::Point<crate::utils::Logical>,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) {
    use crate::utils::{Point, Size};

    let layout = taffy.layout(node).unwrap();

    let pos = Point::new(
        offset.x + layout.location.x,
        offset.y + layout.location.y,
    );
    let size = Size::new(layout.size.width, layout.size.height);

    let color = self.background_color;
    let border_color = crate::Color::BLACK;
    let border_width = 1.0;

    renderer.add_rect(pos.to_array(), size.to_array(), color, border_color, border_width);

    let child_ids = taffy.children(node).unwrap();
    if let Some(content_node) = child_ids.get(0) {
        self.content.draw(
            taffy,
            *content_node,
            renderer,
            pos,
            focused_id,
            ctx,
        );
    }
}
```

- [ ] **Step 2: Update Button::on_event method**

Replace the `on_event` method (lines 116-183) with:

```rust
fn on_event(
    &mut self,
    taffy: &taffy::TaffyTree,
    node: NodeId,
    offset: crate::utils::Point<crate::utils::Logical>,
    event: &winit::event::WindowEvent,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) -> WidgetResponse<M> {
    use crate::utils::{Point, TaffyQuad};

    let layout = taffy.layout(node).unwrap();
    let x = offset.x + layout.location.x;
    let y = offset.y + layout.location.y;

    let taffy_quad = TaffyQuad::from(x, y, layout.size);

    // Handle pointer events
    if let WindowEvent::PointerButton {
        state: winit::event::ElementState::Pressed,
        position,
        ..
    } = event
    {
        let location = PhysicalLocation::new(*position);
        let is_mouse_over = is_location_inside_quad(&location, &ctx.scale, &taffy_quad);
        if is_mouse_over {
            return WidgetResponse {
                message: Some(self.on_press.clone()),
                focus_request: None,
                handled: true,
            };
        }
    }

    // Child event propagation
    let child_ids = taffy.children(node).unwrap();
    if let Some(content_node) = child_ids.get(0) {
        let content_offset = Point::new(x, y);
        return self.content.on_event(
            taffy,
            *content_node,
            content_offset,
            event,
            focused_id,
            ctx,
        );
    }

    WidgetResponse::default()
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: button.rs compiles

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/button.rs
git commit -m "refactor: update Button to use Point type"
```

---

## Task 7: Update Text and TextEdit Widgets

**Files:**
- Modify: `vexo/src/widgets/text.rs`

- [ ] **Step 1: Update Text::draw method**

Replace the `draw` method (lines 89-102) with:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: crate::utils::Point<crate::utils::Logical>,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) {
    use crate::utils::Point;

    let layout = taffy.layout(node).unwrap();
    let pos = Point::new(
        offset.x + layout.location.x,
        offset.y + layout.location.y,
    );

    renderer.add_text(self.content.clone(), pos, self.size, self.color);
}
```

- [ ] **Step 2: Update Text::on_event method signature**

Replace the `on_event` method (lines 104-114) with:

```rust
fn on_event(
    &mut self,
    taffy: &taffy::TaffyTree,
    node: NodeId,
    offset: crate::utils::Point<crate::utils::Logical>,
    event: &winit::event::WindowEvent,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) -> WidgetResponse<M> {
    WidgetResponse::default()
}
```

- [ ] **Step 3: Update TextEdit::draw method**

Replace the `draw` method (lines 176-221) with:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: crate::utils::Point<crate::utils::Logical>,
    _focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) {
    use crate::utils::{Point, Rect, Size};

    let layout = taffy.layout(node).unwrap();
    let pos = Point::new(
        offset.x + layout.location.x,
        offset.y + layout.location.y,
    );
    let size = Size::new(layout.size.width, layout.size.height);

    // Debug border
    let debug_color = crate::Color::RED;
    renderer.add_rect(pos.to_array(), size.to_array(), crate::Color::BLACK, debug_color, 1.0);

    let editor_arc = ctx.get_or_create_editor(&self.editor_id, &self.initial_text);
    let mut editor_ref = editor_arc.borrow_mut();

    editor_ref.set_size(&mut ctx.font_system, size.width, size.height);
    editor_ref.shape_as_needed(&mut ctx.font_system, true);

    renderer.add_editor_request(
        &self.editor_id,
        Rect::new(pos, size),
    );

    let _text_color = crate::Color::WHITE;
    let _cursor_color = crate::Color::WHITE;
    let _selection_color = crate::Color::new(1.0, 1.0, 1.0, 0.2);
    let _selected_text_color = crate::Color::rgb(0.627, 0.627, 1.0);

    let mut _cache = SwashCache::new();
}
```

- [ ] **Step 4: Update TextEdit::on_event method**

Update the signature and offset usage in `on_event` (lines 223-391). Replace the beginning of the method:

```rust
fn on_event(
    &mut self,
    _taffy: &taffy::TaffyTree,
    _node: NodeId,
    _offset: crate::utils::Point<crate::utils::Logical>,
    _event: &winit::event::WindowEvent,
    focused_id: Option<WidgetId>,
    ctx: &mut WidgetContext,
) -> WidgetResponse<M> {
    use crate::utils::{Point, TaffyQuad};

    // Determine our widget id from the node->widget mapping
    let my_id = ctx.get_widget_id(_node);
    let is_focused = focused_id == my_id;

    if !is_focused {
        // Check for click to grab focus
        if let WindowEvent::PointerButton {
            state: winit::event::ElementState::Pressed,
            ..
        } = _event
        {
            let layout = _taffy.layout(_node).unwrap();
            let taffy_quad = TaffyQuad::new(layout.location, layout.size);

            let is_mouse_over =
                is_location_inside_quad(&ctx.cursor_pos, &ctx.scale, &taffy_quad);
            if is_mouse_over {
                return WidgetResponse {
                    message: None,
                    focus_request: my_id,
                    handled: true,
                };
            }
        }
        return WidgetResponse::default();
    }
    // ... rest of the method stays the same
```

- [ ] **Step 5: Verify compilation**

Run: `cargo build -p vexo`
Expected: text.rs compiles

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/text.rs
git commit -m "refactor: update Text and TextEdit to use Point type"
```

---

## Task 8: Update Render Loop in lib.rs

**Files:**
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Update text area creation to use Point**

In the render loop (around lines 456-492), update the text_areas creation:

```rust
let text_areas: Vec<glyphon::TextArea> = processed_texts
    .iter_mut()
    .map(|(buffer, req)| {
        // Convert logical position to physical for glyphon
        let physical_pos = req.position.to_physical(scale_factor);

        let bounds_left: i32 = physical_pos.x.floor() as i32;
        let bounds_top = physical_pos.y.floor() as i32;
        let bounds_right = self.config.width as i32;
        let bounds_bottom: i32 = self.config.height as i32;

        let color_rgba_u8 = cosmic_text::Color::rgba(
            (req.color[0] * 255.0) as u8,
            (req.color[1] * 255.0) as u8,
            (req.color[2] * 255.0) as u8,
            (req.color[3] * 255.0) as u8,
        );

        glyphon::TextArea {
            buffer: buffer,
            left: physical_pos.x,
            top: physical_pos.y,
            scale: scale_factor,
            bounds: TextBounds {
                left: bounds_left,
                top: bounds_top,
                right: bounds_right,
                bottom: bounds_bottom,
            },
            default_color: color_rgba_u8,
            custom_glyphs: &[],
        }
    })
    .collect();
```

- [ ] **Step 2: Update editor requests processing**

In the editor requests loop (around lines 506-528), update to use Rect:

```rust
for req in self.batcher.editor_requests.iter_mut() {
    // Convert logical bounds to physical
    let physical_rect = req.bounds.to_physical(scale_factor);

    let bounds_left: i32 = physical_rect.origin.x.floor() as i32;
    let bounds_top: i32 = physical_rect.origin.y.floor() as i32;
    let bounds_right: i32 = (physical_rect.origin.x + physical_rect.size.width).ceil() as i32;
    let bounds_bottom: i32 = (physical_rect.origin.y + physical_rect.size.height).ceil() as i32;

    let color_rgba_u8 = cosmic_text::Color::rgba(
        (req.color[0] * 255.0) as u8,
        (req.color[1] * 255.0) as u8,
        (req.color[2] * 255.0) as u8,
        (req.color[3] * 255.0) as u8,
    );

    // ... rest of the loop
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add vexo/src/lib.rs
git commit -m "refactor: update render loop to use Point conversions"
```

---

## Task 9: Final Verification

- [ ] **Step 1: Build entire vexo crate**

Run: `cargo build -p vexo --release`
Expected: Compiles without warnings

- [ ] **Step 2: Build desktop demo**

Run: `cargo build -p desktop_demo --release`
Expected: Compiles successfully

- [ ] **Step 3: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Window opens, widgets render correctly without double-scaling artifacts

- [ ] **Step 4: Test on different scale factor**

If you have a Retina display, verify widgets render at correct sizes.

- [ ] **Step 5: Build iOS**

Run: `./build_for_ios.sh`
Expected: iOS framework builds successfully

- [ ] **Step 6: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix: final adjustments for unified point system"
```

---

## Summary

This plan introduces a unified point system with compile-time type safety:

1. **Task 1**: Add `Point<T>`, `Size<T>`, `Rect<T>` with `Logical`/`Physical` markers
2. **Task 2**: Update renderer types to use new types
3. **Task 3**: Update Widget trait signatures
4. **Task 4**: Fix ColorWidget double-scaling bug
5. **Tasks 5-7**: Update all widgets to use typed coordinates
6. **Task 8**: Update render loop conversions
7. **Task 9**: Final verification

The ColorWidget bug was caused by converting to physical coordinates before passing to `add_rect()`, while the shader also multiplies by scale_factor. The fix is to pass logical coordinates consistently.
