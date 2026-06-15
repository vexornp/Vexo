# ScrollView Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a vertical ScrollView widget that clips its child to a viewport and allows scrolling via mouse wheel and keyboard.

**Architecture:** Dedicated `ScrollViewRenderObject` with `Cell<f32>` for scroll offset (interior mutability). New `scroll_offset()` on the `RenderObject` trait. Painter emits `PushOffset`/`PopOffset`. EventHandler dispatches `Scroll` events. `ScrollViewElement` manages keyboard scrolling and updates the render object via `Cell`. `EventContext` gains an optional `&RenderObjectRegistry` reference for element-to-render-object communication. Drag scrolling is not implemented — it requires pointer capture infrastructure that doesn't exist yet.

**Tech Stack:** Rust, Taffy layout (overflow_y: Scroll), Cell<f32> interior mutability, existing PushOffset/PopOffset render commands

---

### Task 1: Add `scroll_offset()` to RenderObject trait

**Files:**
- Modify: `vexo/src/render_object.rs`

- [ ] **Step 1: Add the `scroll_offset()` default method**

In `vexo/src/render_object.rs`, add after `clip_bounds()` (after line 327):

```rust
    fn scroll_offset(&self) -> Option<crate::core::Point<crate::core::Logical>> {
        None
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p vexo`
Expected: compiles (default impl, no downstream changes needed)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/render_object.rs
git commit -m "feat: add scroll_offset() default method to RenderObject trait"
```

---

### Task 2: Emit PushOffset/PopOffset in Painter for scroll_offset

**Files:**
- Modify: `vexo/src/painter.rs`

- [ ] **Step 1: Add scroll offset emission in `paint_recursive`**

In `vexo/src/painter.rs`, replace lines 118-143 (from `// If this object clips its children` through the final `}` of `if transform.is_some()`) with:

```rust
        // If this object clips its children, push clip before painting children.
        let clip = obj.clip_bounds();
        if let Some(local_clip) = &clip {
            let absolute_clip = crate::core::Bounds::new(
                absolute_position.x + local_clip.left,
                absolute_position.y + local_clip.top,
                absolute_position.x + local_clip.right,
                absolute_position.y + local_clip.bottom,
            );
            ctx.push_command(RenderCommand::PushClip { bounds: absolute_clip });
        }

        // If this object has a scroll offset, push it before painting children.
        let scroll = obj.scroll_offset();
        if let Some(offset) = &scroll {
            ctx.push_command(RenderCommand::PushOffset { offset: *offset });
        }

        // Paint children
        for child_id in obj.children() {
            Self::paint_recursive(render_objects, *child_id, ctx, absolute_position);
        }

        // Pop scroll offset after children
        if scroll.is_some() {
            ctx.push_command(RenderCommand::PopOffset);
        }

        // Pop clip after children
        if clip.is_some() {
            ctx.push_command(RenderCommand::PopClip);
        }

        // Pop transform after children
        if transform.is_some() {
            ctx.push_command(RenderCommand::PopTransform);
        }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p vexo`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add vexo/src/painter.rs
git commit -m "feat: emit PushOffset/PopOffset in Painter when scroll_offset() returns Some"
```

---

### Task 3: Add `render_objects` to EventContext

**Files:**
- Modify: `vexo/src/event_context.rs`

The `ScrollViewElement::on_event` needs to read and update the `ScrollViewRenderObject`'s scroll offset. Using `Cell<f32>` on the render object means only `&RenderObjectRegistry` is needed. Add it to `EventContext`.

- [ ] **Step 1: Add `render_objects` field and accessor to `EventContext`**

In `vexo/src/event_context.rs`, add these imports at the top:

```rust
use crate::render_object::RenderObjectRegistry;
```

Add this field to the `EventContext` struct (after `dirty_sender`):

```rust
    /// Render object registry for direct render object access from event handlers.
    /// Used by ScrollViewElement to read/update the ScrollViewRenderObject's scroll offset.
    /// `None` in test contexts.
    render_objects: Option<&'a RenderObjectRegistry>,
```

Update the `new()` constructor — add `render_objects: Option<&'a RenderObjectRegistry>` parameter:

```rust
    pub fn new(
        element_id: ElementKey,
        pointer_position: Point<Logical>,
        local_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        scale: Scale,
        font_system: &'a mut glyphon::FontSystem,
        render_objects: Option<&'a RenderObjectRegistry>,
    ) -> Self {
        Self {
            element_id,
            pointer_position,
            local_position,
            focused_element,
            bounds,
            modifiers,
            scale,
            font_system,
            build_owner: None,
            dirty_sender: None,
            focus_request: None,
            clear_focus_request: false,
            render_objects,
        }
    }
```

Update the `with_build_owner()` constructor — add `render_objects: Option<&'a RenderObjectRegistry>` parameter:

```rust
    pub fn with_build_owner(
        element_id: ElementKey,
        pointer_position: Point<Logical>,
        local_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        scale: Scale,
        font_system: &'a mut glyphon::FontSystem,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a std::sync::mpsc::Sender<ElementKey>,
        render_objects: Option<&'a RenderObjectRegistry>,
    ) -> Self {
        Self {
            element_id,
            pointer_position,
            local_position,
            focused_element,
            bounds,
            modifiers,
            scale,
            font_system,
            build_owner: Some(build_owner),
            dirty_sender: Some(dirty_sender),
            focus_request: None,
            clear_focus_request: false,
            render_objects,
        }
    }
```

Add accessor method:

```rust
    /// Get the render object registry (if available).
    pub fn render_objects(&self) -> Option<&RenderObjectRegistry> {
        self.render_objects
    }
```

Update all existing tests in the file that call `EventContext::new()` to pass `None` as the last parameter.

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p vexo`
Expected: compilation errors in `event_handler.rs` (EventContext constructors changed) — will fix in Task 4

- [ ] **Step 3: Commit**

```bash
git add vexo/src/event_context.rs
git commit -m "feat: add render_objects field to EventContext for element-to-render-object communication"
```

---

### Task 4: Dispatch Scroll events in EventHandler

**Files:**
- Modify: `vexo/src/event_handler.rs`

- [ ] **Step 1: Add `InputEvent::Scroll` arm to `handle_event`**

In `vexo/src/event_handler.rs`, add a new arm in the `handle_event` match, after the `PointerButton` arm and before `Keyboard`:

```rust
            InputEvent::Scroll { .. } => Self::handle_scroll_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focus_manager,
                _position,
                event,
                modifiers,
                scale,
            ),
```

- [ ] **Step 2: Update `handle_pointer_event` to pass `render_objects` to `EventContext`**

In `handle_pointer_event`, update the `EventContext::with_build_owner` call to include `Some(render_objects)`:

```rust
                let mut ctx = EventContext::with_build_owner(
                    element_id,
                    position,
                    local_position,
                    focus_manager.primary_focus_element(),
                    bounds,
                    modifiers,
                    scale,
                    font_system,
                    build_owner,
                    dirty_sender,
                    Some(render_objects),
                );
```

- [ ] **Step 3: Update `handle_keyboard_event` to pass `None` for render_objects**

In `handle_keyboard_event`, update the `EventContext::with_build_owner` call:

```rust
        let mut ctx = EventContext::with_build_owner(
            focused,
            Point::zero(),
            Point::zero(),
            focus_manager.primary_focus_element(),
            bounds,
            modifiers,
            scale,
            font_system,
            build_owner,
            dirty_sender,
            None,
        );
```

- [ ] **Step 4: Add `handle_scroll_event` method**

Add this new method to `EventHandler`:

```rust
    /// Handle a scroll event.
    ///
    /// Dispatches the scroll event to the nearest ScrollView element
    /// in the hit test path. Scroll events are routed to the nearest
    /// scrollable ancestor of the pointer position.
    pub(crate) fn handle_scroll_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        scale: Scale,
    ) -> Option<Box<dyn Any>> {
        let absolute_position = Position::<Logical, Absolute>::new(position.x, position.y);
        let hit_result = render_objects.hit_test(absolute_position);

        if !hit_result.is_hit() {
            return None;
        }

        // Walk the hit path from deepest to shallowest, looking for a
        // ScrollView (identified by render object having non-None scroll_offset).
        let element_path = hit_result.element_path();
        let ro_path = hit_result.path();

        for (&ro_key, &element_id) in ro_path.iter().zip(element_path.iter()).rev() {
            if let Some(ro) = render_objects.get(ro_key) {
                if ro.scroll_offset().is_some() {
                    let bounds = hit_result.absolute_bounds().unwrap_or_default();
                    let local_position = hit_result
                        .inner_bounds()
                        .map(|b| Point::new(position.x - b.position().x, position.y - b.position().y))
                        .unwrap_or(position);

                    if let Some(element) = element_registry.get_mut(element_id) {
                        let mut ctx = EventContext::with_build_owner(
                            element_id,
                            position,
                            local_position,
                            focus_manager.primary_focus_element(),
                            bounds,
                            modifiers,
                            scale,
                            font_system,
                            build_owner,
                            dirty_sender,
                            Some(render_objects),
                        );

                        return element.on_event(event, &mut ctx, state);
                    }
                }
            }
        }

        None
    }
```

Note: `position` comes from the `_position` parameter of `handle_event`, which the pipeline passes as the current pointer position. This works for scroll events because the pipeline already tracks pointer position and passes it when calling `handle_event`.

- [ ] **Step 5: Verify compilation**

Run: `cargo build -p vexo`
Expected: compiles

- [ ] **Step 6: Commit**

```bash
git add vexo/src/event_handler.rs
git commit -m "feat: dispatch Scroll events to nearest ScrollView in hit path"
```

---

### Task 5: Create ScrollViewRenderObject

**Files:**
- Create: `vexo/src/render_objects/scroll_view.rs`
- Modify: `vexo/src/render_objects/mod.rs`

- [ ] **Step 1: Create `ScrollViewRenderObject`**

Create `vexo/src/render_objects/scroll_view.rs`:

```rust
//! ScrollViewRenderObject - manages scroll offset and viewport clipping.

use std::any::Any;
use std::cell::Cell;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{FlexDirection, AlignItems, Layout, LayoutNodeKey, Overflow};
use crate::render::RenderCommand;
use crate::render_object::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject};
use crate::id::RenderObjectKey;

pub struct ScrollViewRenderObject {
    child: Option<RenderObjectKey>,
    /// Scroll offset stored as Cell for interior mutability.
    /// The element updates this via `&self` through `Cell::set()`,
    /// avoiding the need for `&mut RenderObjectRegistry` in EventContext.
    scroll_offset: Cell<f32>,
    content_size: Size<Logical>,
    viewport_size: Size<Logical>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
    child_layout_node: Option<LayoutNodeKey>,
}

impl ScrollViewRenderObject {
    pub fn new() -> Self {
        Self {
            child: None,
            scroll_offset: Cell::new(0.0),
            content_size: Size::zero(),
            viewport_size: Size::zero(),
            computed_bounds: None,
            layout_node: None,
            child_layout_node: None,
        }
    }

    pub fn set_scroll_offset(&self, offset: f32) {
        self.scroll_offset.set(offset);
    }

    pub fn scroll_offset_value(&self) -> f32 {
        self.scroll_offset.get()
    }

    pub fn content_size(&self) -> Size<Logical> {
        self.content_size
    }

    pub fn viewport_size(&self) -> Size<Logical> {
        self.viewport_size
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_size.height - self.viewport_size.height).max(0.0)
    }
}

impl Default for ScrollViewRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for ScrollViewRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        self.child_layout_node = child_nodes.first().copied();

        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .overflow_y(Overflow::Scroll);

        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult { node: existing, size: Size::zero() }
            }
            None => {
                let node = ctx.engine().create_container(&layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult { node, size: Size::zero() }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
                self.viewport_size = Size::new(computed.bounds.width(), computed.bounds.height());
            }
        }

        if let Some(child_node) = self.child_layout_node {
            if let Some(child_computed) = ctx.engine_ref().get_layout(child_node) {
                self.content_size = Size::new(
                    child_computed.bounds.width(),
                    child_computed.bounds.height(),
                );
            }
        }

        // Clamp scroll offset if content shrank
        let max = self.max_scroll();
        if self.scroll_offset.get() > max {
            self.scroll_offset.set(max);
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        self.computed_bounds.map_or(false, |b| b.contains(&position))
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(c) => std::slice::from_ref(c),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> { self.layout_node }
    fn computed_bounds(&self) -> Option<Bounds<Logical>> { self.computed_bounds }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn scroll_offset(&self) -> Option<Point<Logical>> {
        Some(Point::new(0.0, -self.scroll_offset.get()))
    }
}
```

- [ ] **Step 2: Register the module and re-export**

In `vexo/src/render_objects/mod.rs`, add:

```rust
pub mod scroll_view;
pub use scroll_view::ScrollViewRenderObject;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add vexo/src/render_objects/scroll_view.rs vexo/src/render_objects/mod.rs
git commit -m "feat: add ScrollViewRenderObject with Cell-based scroll offset, clipping, and hit test transform"
```

---

### Task 6: Create ScrollViewElement

**Files:**
- Create: `vexo/src/elements/scroll_view.rs`
- Modify: `vexo/src/elements/mod.rs`

- [ ] **Step 1: Create `ScrollViewElement`**

Create `vexo/src/elements/scroll_view.rs`:

```rust
//! ScrollViewElement - manages scroll state and handles input events.

use std::any::Any;

use crate::core::{Bounds, Logical, Point};
use crate::input::{ButtonState, InputEvent, Key, NamedKey};
use crate::element::Element;
use crate::element_context::ElementContext;
use crate::element_state::StateStorage;
use crate::event_context::EventContext;
use crate::id::{ElementKey, RenderObjectKey};
use crate::key::WidgetKey;
use crate::render_objects::ScrollViewRenderObject;
use crate::widgets::Widget;
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;

const LINE_HEIGHT: f32 = 40.0;

pub struct ScrollViewElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
    scroll_offset: f32,
    content_height: f32,
    viewport_height: f32,
}

impl ScrollViewElement {
    pub fn new() -> Self {
        Self {
            id: None, key: None, render_object: None, widget: None,
            focus_attachment: None,
            scroll_offset: 0.0, content_height: 0.0, viewport_height: 0.0,
        }
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    fn clamp_offset(&self, offset: f32) -> f32 {
        offset.clamp(0.0, self.max_scroll())
    }

    /// Update viewport/content dimensions from the render object,
    /// then set scroll offset on both the element and the render object.
    /// Returns true if the offset changed.
    fn apply_scroll_offset(&mut self, new_offset: f32, ctx: &EventContext) -> bool {
        // Read current dimensions from the render object
        if let Some(ro) = ctx.render_objects().and_then(|rr| rr.get(self.render_object?)) {
            if let Some(svro) = ro.as_any().downcast_ref::<ScrollViewRenderObject>() {
                self.viewport_height = svro.viewport_size().height;
                self.content_height = svro.content_size().height;
            }
        }

        let clamped = self.clamp_offset(new_offset);
        if (clamped - self.scroll_offset).abs() < f32::EPSILON {
            return false;
        }
        self.scroll_offset = clamped;

        // Write to render object via Cell (interior mutability through &self)
        if let Some(svro) = ctx.render_objects().and_then(|rr| rr.get(self.render_object?)) {
            if let Some(svro) = ro.as_any().downcast_ref::<ScrollViewRenderObject>() {
                svro.set_scroll_offset(clamped);
            }
        }

        // Mark render object as needing paint
        if let Some(ro_key) = self.render_object {
            if let Some(bo) = ctx.build_owner {
                // mark_needs_build triggers a frame request which repaints
                bo.mark_needs_build(ctx.element_id());
            }
        }
        true
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for ScrollViewElement {
    fn default() -> Self { Self::new() }
}

impl RenderObjectElement for ScrollViewElement {
    fn widget(&self) -> Option<&dyn Widget> { self.widget.as_deref() }
    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(sv) = widget.as_any().downcast_ref::<crate::widgets::scroll_view::ScrollView>() {
            self.key = sv.key.clone();
        }
        self.widget = Some(widget);
    }
    fn render_object_id(&self) -> Option<RenderObjectKey> { self.render_object }
    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) { self.render_object = id; }
    fn stored_key(&self) -> Option<WidgetKey> { self.key.clone() }
    fn set_stored_key(&mut self, key: Option<WidgetKey>) { self.key = key; }
    fn element_id(&self) -> Option<ElementKey> { self.id }
    fn set_element_id(&mut self, id: Option<ElementKey>) { self.id = id; }
}

impl Element for ScrollViewElement {
    fn mount(&mut self, context: &mut ElementContext) {
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context.focus_manager().create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }
        self.mount_render_object(context);
        if let Some(child_widget) = self.get_child_widget() {
            context.inflate_child(None, child_widget.clone_boxed());
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        self.unmount_render_object(context);
        if let Some(mut attachment) = self.focus_attachment.take() {
            attachment.detach(context.focus_manager());
        }
    }

    fn render_object(&self) -> Option<RenderObjectKey> { self.render_object }
    fn widget_key(&self) -> Option<WidgetKey> { self.key.clone() }
    fn can_update(&self, _widget: &dyn Any) -> bool { true }

    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
        _state: &mut StateStorage,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::Scroll { delta } => {
                let new_offset = self.scroll_offset - delta.y;
                self.apply_scroll_offset(new_offset, context);
                return Some(Box::new(()));
            }

            InputEvent::Keyboard { key, .. } => {
                if context.is_focused_self() {
                    let delta = match key {
                        Key::Named(NamedKey::ArrowUp) => Some(-LINE_HEIGHT),
                        Key::Named(NamedKey::ArrowDown) => Some(LINE_HEIGHT),
                        Key::Named(NamedKey::PageUp) => Some(-self.viewport_height),
                        Key::Named(NamedKey::PageDown) => Some(self.viewport_height),
                        Key::Named(NamedKey::Home) => Some(-self.scroll_offset),
                        Key::Named(NamedKey::End) => Some(self.max_scroll() - self.scroll_offset),
                        _ => None,
                    };
                    if let Some(d) = delta {
                        self.apply_scroll_offset(self.scroll_offset + d, context);
                        return Some(Box::new(()));
                    }
                }
            }

            _ => {}
        }
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            if let Some(sv) = widget.as_any().downcast_ref::<crate::widgets::scroll_view::ScrollView>() {
                self.key = sv.key.clone();
            }
            self.widget = Some(*widget);

            if let Some(child_widget) = self.get_child_widget() {
                let old_child = context.children().first().copied();
                match old_child {
                    Some(old_child_key) => context.update_child(old_child_key, child_widget.clone_boxed()),
                    None => context.inflate_child(None, child_widget.clone_boxed()),
                }
            } else if let Some(old_child_key) = context.children().first().copied() {
                context.unmount_child(old_child_key);
            }
        }

        if let Some(attachment) = self.focus_attachment.as_ref() {
            let new_parent_id = context.parent_focus_node_id();
            attachment.reparent_to(new_parent_id, context.focus_manager());
        }
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }

    fn focus_attachment(&self) -> &Option<FocusAttachment> { &self.focus_attachment }
    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> { &mut self.focus_attachment }
}
```

**Note:** The `apply_scroll_offset` method has a bug in the second `if let` block — `ro` is not in scope. The correct code should be:

```rust
    fn apply_scroll_offset(&mut self, new_offset: f32, ctx: &EventContext) -> bool {
        if let Some(rr) = ctx.render_objects() {
            if let Some(ro) = rr.get(self.render_object.unwrap()) {
                if let Some(svro) = ro.as_any().downcast_ref::<ScrollViewRenderObject>() {
                    self.viewport_height = svro.viewport_size().height;
                    self.content_height = svro.content_size().height;
                }
            }
        }

        let clamped = self.clamp_offset(new_offset);
        if (clamped - self.scroll_offset).abs() < f32::EPSILON {
            return false;
        }
        self.scroll_offset = clamped;

        if let Some(rr) = ctx.render_objects() {
            if let Some(ro) = rr.get(self.render_object.unwrap()) {
                if let Some(svro) = ro.as_any().downcast_ref::<ScrollViewRenderObject>() {
                    svro.set_scroll_offset(clamped);
                }
            }
        }

        if let Some(bo) = ctx.build_owner {
            bo.mark_needs_build(ctx.element_id());
        }
        true
    }
```

- [ ] **Step 2: Register the module and re-export**

In `vexo/src/elements/mod.rs`, add:

```rust
pub mod scroll_view;
pub use scroll_view::ScrollViewElement;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: compiles (may need adjustments to `apply_scroll_offset` — fix any borrow/type errors)

- [ ] **Step 4: Commit**

```bash
git add vexo/src/elements/scroll_view.rs vexo/src/elements/mod.rs
git commit -m "feat: add ScrollViewElement with scroll and keyboard event handling"
```

---

### Task 7: Create ScrollView widget

**Files:**
- Create: `vexo/src/widgets/scroll_view.rs`
- Modify: `vexo/src/widgets/mod.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Create `ScrollView` widget**

Create `vexo/src/widgets/scroll_view.rs`:

```rust
//! ScrollView widget - a scrollable container for content that overflows.

use std::any::Any;

use crate::elements::ScrollViewElement;
use crate::element::Element;
use crate::key::WidgetKey;
use crate::render_object::RenderObject;
use crate::render_objects::ScrollViewRenderObject;
use crate::widgets::Widget;
use crate::UpdateResult;

pub struct ScrollView {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
}

impl ScrollView {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { key: None, child: Box::new(child) }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Clone for ScrollView {
    fn clone(&self) -> Self {
        Self { key: self.key.clone(), child: self.child.clone_boxed() }
    }
}

impl Widget for ScrollView {
    fn key(&self) -> Option<WidgetKey> { self.key.clone() }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ScrollViewElement::new();
        if let Some(sv) = self.as_any().downcast_ref::<ScrollView>() {
            elem.set_widget(self.clone_boxed());
        }
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ScrollViewRenderObject::new())
    }

    fn as_any(&self) -> &dyn Any { self }

    fn child(&self) -> Option<&dyn Widget> { Some(self.child.as_ref()) }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<ScrollView>().is_some()
    }

    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        UpdateResult::default()
    }

    fn clone_boxed(&self) -> Box<dyn Widget> { Box::new(self.clone()) }
}
```

**Note:** The `create_element` implementation follows the `GestureDetector` pattern. The element's `set_widget` method (from `RenderObjectElement`) handles downcasting. However, the `GestureDetectorElement` uses a custom `set_widget_from_widget` method. Since `ScrollViewElement` implements `RenderObjectElement::set_widget` to handle downcasting, we can just call `set_widget` directly. The simplest correct implementation:

```rust
    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ScrollViewElement::new();
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }
```

But `set_widget` takes `Box<dyn Widget>`, and `ScrollViewElement::set_widget` (from `RenderObjectElement`) expects `Box<dyn Widget>`. This should work because `RenderObjectElement::set_widget` is the trait method that `ScrollViewElement` overrides. The override downcasts to `ScrollView` and extracts the key.

Wait — looking at the `RenderObjectElement` trait, `set_widget` takes `Box<dyn Widget>`, not `Box<dyn Any>`. But the `ScrollViewElement` override of `set_widget` does `widget.as_any().downcast_ref::<ScrollView>()`. This works because `Box<dyn Widget>` has an `as_any()` method via the `Widget` trait's `as_any()`.

However, `RenderObjectElement::set_widget` takes `Box<dyn Widget>`, while `Element::update` takes `Box<dyn Any>`. These are different. For `create_element`, we call `set_widget(Box<dyn Widget>)`, which is fine.

- [ ] **Step 2: Register the widget module and re-export**

In `vexo/src/widgets/mod.rs`, add:

```rust
pub(crate) mod scroll_view;
```

And add `ScrollView` to the public re-exports (alongside `Flex`, `Grid`, `Text`, etc.):

```rust
pub use scroll_view::ScrollView;
```

In `vexo/src/lib.rs`, add to the re-exports:

```rust
pub use widgets::ScrollView;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/scroll_view.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat: add ScrollView widget"
```

---

### Task 8: Write unit tests for ScrollViewElement and ScrollViewRenderObject

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs` (add tests)
- Modify: `vexo/src/render_objects/scroll_view.rs` (add tests)

- [ ] **Step 1: Add tests to ScrollViewElement**

Add a `#[cfg(test)] mod tests` section at the bottom of `vexo/src/elements/scroll_view.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bounds;

    #[test]
    fn test_clamp_offset_at_zero() {
        let elem = ScrollViewElement::new();
        assert_eq!(elem.clamp_offset(-10.0), 0.0);
    }

    #[test]
    fn test_clamp_offset_at_max() {
        let mut elem = ScrollViewElement::new();
        elem.content_height = 500.0;
        elem.viewport_height = 100.0;
        assert_eq!(elem.clamp_offset(450.0), 400.0);
    }

    #[test]
    fn test_no_scroll_when_content_fits() {
        let mut elem = ScrollViewElement::new();
        elem.content_height = 300.0;
        elem.viewport_height = 500.0;
        assert_eq!(elem.max_scroll(), 0.0);
        assert_eq!(elem.clamp_offset(100.0), 0.0);
    }
}
```

- [ ] **Step 2: Add tests to ScrollViewRenderObject**

Add a `#[cfg(test)] mod tests` section at the bottom of `vexo/src/render_objects/scroll_view.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_defaults() {
        let ro = ScrollViewRenderObject::new();
        assert_eq!(ro.scroll_offset_value(), 0.0);
        assert_eq!(ro.max_scroll(), 0.0);
    }

    #[test]
    fn test_set_scroll_offset_via_cell() {
        let ro = ScrollViewRenderObject::new();
        ro.set_scroll_offset(42.0);
        assert_eq!(ro.scroll_offset_value(), 42.0);
    }

    #[test]
    fn test_scroll_offset_trait_method() {
        let ro = ScrollViewRenderObject::new();
        ro.set_scroll_offset(100.0);
        let offset = ro.scroll_offset().unwrap();
        assert_eq!(offset.x, 0.0);
        assert_eq!(offset.y, -100.0);
    }

    #[test]
    fn test_hit_test_transform_is_none() {
        let ro = ScrollViewRenderObject::new();
        ro.set_scroll_offset(50.0);
        // ScrollView uses scroll_offset for child pointer adjustment, not hit_test_transform.
        assert!(ro.hit_test_transform().is_none());
    }

    #[test]
    fn test_clip_bounds_returns_computed_bounds() {
        let mut ro = ScrollViewRenderObject::new();
        assert!(ro.clip_bounds().is_none());
        ro.computed_bounds = Some(Bounds::from_xywh(10.0, 20.0, 200.0, 100.0));
        assert!(ro.clip_bounds().is_some());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/elements/scroll_view.rs vexo/src/render_objects/scroll_view.rs
git commit -m "test: add ScrollViewElement and ScrollViewRenderObject unit tests"
```

---

### Task 9: Add ScrollView demo to shared_app

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add ScrollView import and demo function**

In `shared_app/src/lib.rs`, add `ScrollView` and `Flex` to the imports:

```rust
use vexo::{
    column, row, run_desktop_demo, Application, BuildContext, Color, Focus,
    Flex, ScrollView,
    State as RetainState, StatefulWidget, SystemCursorKind, Text, TextEdit,
    TextEditingController, Widget, input::MouseCursor, reactive::StatefulMutable,
};
```

Add a helper function:

```rust
fn scroll_demo() -> Box<dyn Widget> {
    let mut column = Flex::column().gap(0.0);
    for i in 0..20 {
        let label = format!("Item {}", i + 1);
        column = column.push(
            Text::new(&label)
                .padding(16.0)
                .background(if i % 2 == 0 {
                    Color::rgb(0.95, 0.95, 0.95)
                } else {
                    Color::WHITE
                })
        );
    }
    ScrollView::new(column)
        .width(200.0)
        .height(300.0)
        .border(Color::rgb(0.6, 0.6, 0.6), 1.0)
        .boxed()
}
```

Note: `.width()`, `.height()`, and `.border()` on `ScrollView` are trait methods from `Widget` (they return `Box<dyn Widget>`). Since `ScrollView` doesn't have inherent methods for these, the trait defaults wrap it in `WithLayout` and `DecoratedContainer`. This works but creates extra elements. If that's undesirable, wrap the `ScrollView` in a `Flex` with size constraints instead.

Include the `scroll_demo()` output in the main layout.

- [ ] **Step 2: Build and run the desktop demo**

Run: `cargo run -p desktop_demo`
Expected: app runs with a scrollable section visible

- [ ] **Step 3: Verify scroll interactions**

Verify:
- Mouse wheel scrolling works
- Keyboard scrolling (arrow keys, PageUp/Down, Home/End) works after clicking inside
- Content is clipped to the viewport
- Scroll offset is clamped

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat: add ScrollView demo to shared_app"
```

---

### Task 10: Final integration and cleanup

- [ ] **Step 1: Run full build and test suite**

Run: `cargo build && cargo test`
Expected: everything compiles and passes

- [ ] **Step 2: Fix any issues found during testing**

If the desktop demo reveals issues (e.g., scroll events not reaching the ScrollView, content not rendering inside the viewport, hit testing misbehaving), investigate and fix them.

Common issues to check:
- Scroll events need the correct pointer position for hit testing. If `Point::zero()` is used, the hit test may miss the ScrollView. Verify the pipeline passes the correct position.
- The `WithLayout`/`DecoratedContainer` wrappers from trait modifier methods may interfere with `ScrollViewElement`'s child mounting. If `.width(200.0)` wraps ScrollView in `WithLayout`, the `ScrollViewElement` may not find its child. Test with and without trait modifier methods.
- `apply_layout` reads `content_size` from the child's Taffy node. Verify the Taffy `Overflow::Scroll` setting produces the expected content size (it should be the child's natural size, not clamped to the viewport).

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "fix: address ScrollView integration issues"
```
