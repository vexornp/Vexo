# Counter Component Demo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update desktop demo to showcase custom component usage with a Counter Component that demonstrates local state, message isolation, and output message mapping.

**Architecture:** Add `CounterComponent` implementing the `Component` trait with local count state, internal messages (Increment/Decrement/Reset), and output message (CountReached) emitted when count hits 10. Create a `MapWidget` wrapper to convert `CounterOutput` to `Message`. Integrate into existing demo app.

**Tech Stack:** Rust, Vexo component system (`Component` trait, `ComponentWidget`, `ComponentContext`)

---

## Files

| File | Purpose |
|------|---------|
| `shared_app/src/lib.rs` | Add CounterComponent, MapWidget, update State/Message, modify view() |

---

### Task 1: Add CounterComponent Types and Component Implementation

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add CounterComponent types and implementation after the existing Message enum**

Add after line 15 (after `Message` enum definition):

```rust
// --- Counter Component ---

#[derive(Clone, Debug)]
pub enum CounterMessage {
    Increment,
    Decrement,
    Reset,
}

#[derive(Clone, Debug)]
pub enum CounterOutput {
    CountReached(u32),
}

#[derive(Default)]
pub struct CounterState {
    count: u32,
}

pub struct CounterComponent;

impl vexo::component::Component for CounterComponent {
    type Message = CounterMessage;
    type Output = CounterOutput;
    type State = CounterState;

    fn update(state: &mut Self::State, message: Self::Message) {
        match message {
            CounterMessage::Increment => state.count += 1,
            CounterMessage::Decrement => {
                if state.count > 0 {
                    state.count -= 1;
                }
            }
            CounterMessage::Reset => state.count = 0,
        }
    }

    fn view(
        state: &Self::State,
        ctx: &mut vexo::component::ComponentContext<'_, Self::Message>,
    ) -> Box<dyn vexo::widgets::Widget<Self::Message>> {
        let count_text = format!("Count: {}", state.count);

        vexo::column![
            vexo::text!(count_text).font_size(24.0),
            vexo::row![
                vexo::button!(vexo::text!("-"), CounterMessage::Decrement)
                    .width(40.0)
                    .height(40.0),
                vexo::button!(vexo::text!("+"), CounterMessage::Increment)
                    .width(40.0)
                    .height(40.0),
                vexo::button!(vexo::text!("Reset"), CounterMessage::Reset)
                    .height(40.0),
            ]
            .gap(8.0),
        ]
        .align(vexo::layout::AlignItems::Center)
        .padding(16.0)
        .background(vexo::Color::rgb(0.95, 0.95, 0.95))
        .border(vexo::Color::rgb(0.8, 0.8, 0.8), 1.0)
        .corner_radius(8.0)
        .boxed()
    }

    fn map_message(message: Self::Message, state: &Self::State) -> Option<Self::Output> {
        match message {
            CounterMessage::Increment if state.count == 10 => {
                Some(CounterOutput::CountReached(10))
            }
            _ => None,
        }
    }
}
```

- [ ] **Step 2: Build to verify no compilation errors**

Run: `cargo build -p shared_app`
Expected: Build succeeds with no errors

---

### Task 2: Add MapWidget for Message Mapping

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add MapWidget struct and Widget implementation after CounterComponent**

Add after the `CounterComponent` implementation:

```rust
// --- Message Mapping Widget ---

/// A widget wrapper that maps messages from one type to another.
pub struct MapWidget<M1, M2, F>
where
    M1: Clone + std::fmt::Debug + Send,
    M2: Clone + std::fmt::Debug + Send,
    F: Fn(M1) -> M2 + Send,
{
    inner: Box<dyn vexo::widgets::Widget<M1>>,
    mapper: F,
    computed_layout: Option<vexo::testable::ComputedLayout>,
}

impl<M1, M2, F> MapWidget<M1, M2, F>
where
    M1: Clone + std::fmt::Debug + Send,
    M2: Clone + std::fmt::Debug + Send,
    F: Fn(M1) -> M2 + Send,
{
    pub fn new(inner: Box<dyn vexo::widgets::Widget<M1>>, mapper: F) -> Self {
        Self {
            inner,
            mapper,
            computed_layout: None,
        }
    }
}

impl<M1, M2, F> vexo::widgets::Widget<M2> for MapWidget<M1, M2, F>
where
    M1: Clone + std::fmt::Debug + Send,
    M2: Clone + std::fmt::Debug + Send,
    F: Fn(M1) -> M2 + Send,
{
    fn key(&self) -> Option<&str> {
        self.inner.key()
    }

    fn layout_props(&self) -> vexo::layout::Layout {
        self.inner.layout_props()
    }

    fn cursor(&self) -> vexo::input::CursorIcon {
        self.inner.cursor()
    }

    fn layout(
        &mut self,
        layout_ctx: &mut vexo::layout::LayoutContext,
        widget_ctx: &mut vexo::widgets::WidgetContext,
    ) -> vexo::layout::LayoutNodeId {
        self.inner.layout(layout_ctx, widget_ctx)
    }

    fn apply_layout(&mut self, layout: vexo::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
        self.inner.apply_layout(layout);
    }

    fn paint(&self, ctx: &mut vexo::testable::PaintContext) -> Vec<vexo::render::RenderCommand> {
        self.inner.paint(ctx)
    }

    fn draw(
        &self,
        layout_view: &vexo::layout::LayoutView,
        node: vexo::layout::LayoutNodeId,
        renderer: &mut vexo::renderer::UiBatcher,
        offset: vexo::core::Point<vexo::core::Logical>,
        focused_id: Option<vexo::core::WidgetId>,
        cursor_blink: &vexo::CursorBlinkState,
        widget_ctx: &mut vexo::widgets::WidgetContext,
    ) {
        self.inner.draw(
            layout_view,
            node,
            renderer,
            offset,
            focused_id,
            cursor_blink,
            widget_ctx,
        );
    }

    fn on_event(
        &mut self,
        layout_view: &vexo::layout::LayoutView,
        node: vexo::layout::LayoutNodeId,
        offset: vexo::core::Point<vexo::core::Logical>,
        event: &vexo::input::InputEvent,
        focused_id: Option<vexo::core::WidgetId>,
        widget_ctx: &mut vexo::widgets::WidgetContext,
    ) -> vexo::widgets::WidgetResponse<M2> {
        let response = self.inner.on_event(
            layout_view,
            node,
            offset,
            event,
            focused_id,
            widget_ctx,
        );

        let mapped_message = response.message.map(&self.mapper);

        vexo::widgets::WidgetResponse {
            message: mapped_message,
            focus_request: response.focus_request,
            handled: response.handled,
            clear_focus: response.clear_focus,
            cursor: response.cursor,
        }
    }
}
```

- [ ] **Step 2: Build to verify no compilation errors**

Run: `cargo build -p shared_app`
Expected: Build succeeds with no errors

---

### Task 3: Update Application State and Message

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Update Message enum to include CounterOutput**

Replace the existing `Message` enum (lines 11-15):

```rust
#[derive(Debug, Clone)]
pub enum Message {
    None,
    Clicked,
    CounterOutput(CounterOutput),
}
```

- [ ] **Step 2: Add milestones field to State struct**

Replace the existing `State` struct (lines 17-19):

```rust
pub struct State {
    click_count: u32,
    milestones: u32,
}
```

- [ ] **Step 3: Update State::new() to initialize milestones**

Replace the existing `new()` implementation (lines 25-27):

```rust
    fn new() -> Self::State {
        Self {
            click_count: 0,
            milestones: 0,
        }
    }
```

- [ ] **Step 4: Update update() to handle CounterOutput**

Replace the existing `update()` implementation (lines 29-36):

```rust
    fn update(state: &mut Self::State, message: Self::Message) {
        match message {
            Message::Clicked => {
                state.click_count += 1;
            }
            Message::CounterOutput(CounterOutput::CountReached(_n)) => {
                state.milestones += 1;
            }
            Message::None => {}
        }
    }
```

- [ ] **Step 5: Build to verify no compilation errors**

Run: `cargo build -p shared_app`
Expected: Build succeeds with no errors

---

### Task 4: Update view() to Include CounterComponent

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Update view() to add CounterComponent with MapWidget**

Replace the existing `view()` function (lines 38-97):

```rust
    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>> {
        let text_content = format!("You clicked {} times!", state.click_count);
        let milestone_text = format!("Milestones reached: {}", state.milestones);

        // Create the counter component wrapped in MapWidget
        let counter_widget = MapWidget::new(
            Box::new(vexo::component::ComponentWidget::<CounterComponent>::new("counter")),
            |output| Message::CounterOutput(output),
        );

        vexo::column![
            // Title
            vexo::text!("Counter Component Demo")
                .font_size(28.0),
            // Counter Component with message mapping
            counter_widget,
            // Milestone display
            vexo::text!(milestone_text)
                .font_size(18.0)
                .padding(10.0),
            // Existing demo widgets
            vexo::text_edit!("editor_id_input")
                .content("Type here...")
                .width(100.0)
                .height(50.0),
            vexo::column![vexo::text!("Modified Text")
                .font_size(24.0)
                .background(vexo::Color::RED)
                .border(vexo::Color::GREEN, 2.0)
                .corner_radius(8.0)]
            .padding(10.0),
            vexo::column![
                vexo::button!(vexo::text!(text_content).font_size(24.0), Message::Clicked)
                    .background(vexo::Color::rgb(0.1, 0.4, 0.1))
                    .border(vexo::Color::BLACK, 1.0)
                    .corner_radius(8.0)
            ]
            .padding(10.0)
            .background(vexo::Color::BLUE),
            vexo::color_widget!(vexo::Color::CYAN).width(110.0).height(30.0),
            vexo::row![
                vexo::color_widget!(vexo::Color::RED).width(60.0).height(70.0),
                vexo::color_widget!(vexo::Color::YELLOW).width(90.0).height(40.0),
            ],
        ]
        .align(vexo::layout::AlignItems::Center)
        .fill()
        .background(vexo::Color::WHITE)
        .boxed()
    }
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p shared_app`
Expected: Build succeeds with no errors

---

### Task 5: Run and Verify

- [ ] **Step 1: Run the desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Window opens with counter component visible

- [ ] **Step 2: Manually test counter functionality**

1. Click [+] button 10 times
2. Verify "Milestones reached: 1" appears
3. Click [Reset] button
4. Verify count goes to 0
5. Click [+] 10 more times
6. Verify "Milestones reached: 2"
7. Click [-] button
8. Verify count decreases and never goes below 0

- [ ] **Step 3: Commit the changes**

```bash
git add shared_app/src/lib.rs
git commit -m "feat(demo): add CounterComponent to demonstrate custom component usage"
```