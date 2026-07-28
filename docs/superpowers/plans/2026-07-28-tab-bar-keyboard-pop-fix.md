# Tab Bar Keyboard-Dismiss "Pop" Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the end-of-keyboard-dismiss tab bar height "pop" by making the bar's effective height constant (`49 + home_indicator_inset`) throughout keyboard show/dismiss animations.

**Architecture:** Two coordinated changes in `vexo_uikit/src/tab_bar.rs`. Both switch from reading the keyboard-clamped `padding.bottom` to reading the raw, un-clamped `viewPadding.bottom` — Change 1 wraps the bar's `SafeArea` in a `MediaQueryMutator` that forces `padding.bottom = viewPadding.bottom`; Change 2 updates the page's `tab_bar_height` math to use the same source. TDD: one test per change, each fails before its change and passes after.

**Tech Stack:** Rust, vexo framework (Component/MediaQuery/SafeArea), Taffy layout, vexo_uikit (TabBarView).

## Global Constraints

- All changes are in `vexo_uikit/src/tab_bar.rs` — no changes to `SafeArea`, `RootMediaQuery`, or any other file.
- `SafeArea`'s contract (reads `MediaQuery::of(ctx).padding`) is unchanged — Change 1 feeds it un-clamped data via a `MediaQueryMutator` wrapper.
- `RootMediaQuery`'s `padding = max(viewPadding - viewInsets, 0)` invariant is unchanged.
- The two changes must both reference `viewPadding.bottom` — this coordination is verified by Task 2's test.

---

### Task 1: Fix tab bar height pop (Change 1 — bar inset source)

**Files:**
- Modify: `vexo_uikit/src/tab_bar.rs:197-200` (bar inset wrapper)
- Test: `vexo_uikit/src/tab_bar.rs` (test module, new test)

**Interfaces:**
- Consumes: `MediaQueryMutator`, `MediaQueryData`, `SafeArea` — all already imported at `tab_bar.rs:14-19`.
- Produces: bar height = `TAB_BAR_HEIGHT + viewPadding.bottom` (constant during keyboard animation).

- [ ] **Step 1: Write the failing test**

Add this test to the `#[cfg(test)] mod tests` block in `vexo_uikit/src/tab_bar.rs`, after the existing `test_tab_bar_tap_between_icons_selects_slot` test:

```rust
#[test]
fn test_tab_bar_height_constant_during_keyboard_animation() {
    use vexo::animation::AnimationTicker;
    use vexo::layout::TaffyLayoutEngine;
    use vexo::render::RenderCommand;
    use vexo::ThreeTreePipeline;

    // Simulate what RootMediaQuery would produce for two keyboard states.
    // safe_area.bottom = 34 (home indicator), keyboard interpolates 150 -> 0.
    // padding.bottom = max(34 - kh, 0)  — the clamped formula that causes the pop.
    fn mq_for_keyboard(kh: f32) -> MediaQueryData {
        let mut mq = MediaQueryData::all_zero();
        mq.size = vexo::core::Size::new(390.0, 600.0);
        mq.padding.bottom = (34.0 - kh).max(0.0);
        mq.viewInsets.bottom = kh;
        mq.viewPadding.bottom = 34.0;
        mq
    }

    fn build_view(kh: f32) -> Box<dyn Widget> {
        let ctrl = TabController::new(TestTab::A);
        let inner = TabBarView::new(
            ctrl,
            vec![TestTab::A, TestTab::B],
            |tab| match tab {
                TestTab::A => Text::new("Page A").boxed(),
                TestTab::B => Text::new("Page B").boxed(),
                TestTab::C => Text::new("Page C").boxed(),
            },
            |_, is_selected| Text::new(if is_selected { "[A]" } else { "A" }).boxed(),
        );
        MediaQuery::new(mq_for_keyboard(kh), inner).boxed()
    }

    fn bar_bg_height(kh: f32) -> f32 {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(build_view(kh));
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(390.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        let commands = pipeline.paint();
        let nav_bg = crate::theme::tokens::navigation::colors(&vexo::ThemeData::light())
            .mobile_header_bg;
        commands
            .iter()
            .find_map(|cmd| {
                if let RenderCommand::Rect { bounds, fill, .. } = cmd {
                    if *fill == nav_bg && bounds.width() >= 380.0 {
                        return Some(bounds.height());
                    }
                }
                None
            })
            .expect("expected a tab bar background rect")
    }

    // The bar height must be the SAME in both states: 49 + 34 = 83.
    // Before the fix, mid-animation (kh=150) gives 49 (clamped padding=0),
    // and end (kh=0) gives 83 (padding=34) — the "pop".
    let mid = bar_bg_height(150.0);
    let end = bar_bg_height(0.0);
    assert!(
        (mid - 83.0).abs() < 1.0,
        "mid-animation bar height {} should be ~83 (49 + 34 home-indicator inset)",
        mid
    );
    assert!(
        (end - 83.0).abs() < 1.0,
        "end bar height {} should be ~83",
        end
    );
    assert!(
        (mid - end).abs() < 0.5,
        "bar height must be constant: mid={}, end={}",
        mid,
        end
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit test_tab_bar_height_constant_during_keyboard_animation`
Expected: FAIL — mid-animation bar height ≈ 49 (not 83), because `SafeArea` reads `padding.bottom = max(34 - 150, 0) = 0`.

- [ ] **Step 3: Apply Change 1 — wrap `SafeArea` in `MediaQueryMutator`**

In `vexo_uikit/src/tab_bar.rs`, replace lines 197-200. The current code is:

```rust
        let bar = DecoratedBox::with_style(
            SafeArea::new(bar.boxed()).top(false).boxed(),
            Style::default().background(nav.mobile_header_bg),
        );
```

Replace with:

```rust
        let bar = DecoratedBox::with_style(
            MediaQueryMutator::new(
                SafeArea::new(bar.boxed()).top(false).boxed(),
                |parent: &MediaQueryData| {
                    let mut p = parent.padding;
                    p.bottom = parent.viewPadding.bottom;
                    parent.copy_with_padding(p)
                },
            )
            .boxed(),
            Style::default().background(nav.mobile_header_bg),
        );
```

This forces `SafeArea` to see `padding.bottom = viewPadding.bottom` (the raw home-indicator inset, ~34pt) regardless of keyboard state, instead of the clamped `max(viewPadding.bottom - keyboard_height, 0)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo_uikit test_tab_bar_height_constant_during_keyboard_animation`
Expected: PASS — bar height is now ~83 in both mid-animation and end states.

- [ ] **Step 5: Run all existing tab_bar tests to verify no regressions**

Run: `cargo test -p vexo_uikit`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/tab_bar.rs
git commit -m "fix: tab bar height pop during keyboard dismiss

Wrap SafeArea in MediaQueryMutator that forces padding.bottom =
viewPadding.bottom, so the bar's safe-area inset is the constant
home-indicator height instead of the keyboard-clamped value. Eliminates
the end-of-animation 'pop' where the bar jumps from 49pt to 83pt."
```

---

### Task 2: Fix page keyboard-avoidance coordination (Change 2 — page `tab_bar_height`)

**Files:**
- Modify: `vexo_uikit/src/tab_bar.rs:219` (page mutator `tab_bar_height` source)
- Test: `vexo_uikit/src/tab_bar.rs` (test module, new test + helper widget)

**Interfaces:**
- Consumes: `viewPadding.bottom` from parent `MediaQueryData` (same source as Change 1).
- Produces: page child's `viewInsets.bottom = max(keyboard_height - tab_bar_height, 0)` where `tab_bar_height = 49 + viewPadding.bottom` — matches the actual bar height from Change 1.

- [ ] **Step 1: Write the failing test**

Add a capture widget and test to the `#[cfg(test)] mod tests` block in `vexo_uikit/src/tab_bar.rs`. First, add these imports to the test module's header (after line 240: `use vexo::Text;`):

```rust
    use std::cell::RefCell;
    use vexo::SimpleState;
```

Then add these items after the last test:

```rust
/// A tiny Component that captures the `MediaQueryData` its subtree receives.
/// Used to verify what `viewInsets.bottom` the page child sees after
/// `TabBarView`'s `MediaQueryMutator` chain transforms the parent MQ.
#[derive(Clone)]
struct MqCapture {
    captured: Rc<RefCell<Option<MediaQueryData>>>,
}

impl Component for MqCapture {
    type State = SimpleState<()>;
    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        *self.captured.borrow_mut() = Some(MediaQuery::of(ctx));
        Text::new("capture").boxed()
    }
}

#[test]
fn test_tab_bar_page_view_insets_match_bar_height() {
    // Simulate mid-keyboard-dismiss: keyboard_height=150, safe_bottom=34.
    // RootMediaQuery would produce: padding.bottom = max(34-150,0) = 0,
    // viewInsets.bottom = 150, viewPadding.bottom = 34.
    let mut mq = MediaQueryData::all_zero();
    mq.size = vexo::core::Size::new(390.0, 600.0);
    mq.padding.bottom = 0.0; // clamped (34 - 150 < 0)
    mq.viewInsets.bottom = 150.0;
    mq.viewPadding.bottom = 34.0;

    let captured = Rc::new(RefCell::new(None));
    let captured_for_page = Rc::clone(&captured);
    let ctrl = TabController::new(TestTab::A);
    let inner = TabBarView::new(
        ctrl,
        vec![TestTab::A],
        move |_| MqCapture {
            captured: Rc::clone(&captured_for_page),
        }
        .boxed(),
        |_, _| Text::new("A").boxed(),
    );
    let view = MediaQuery::new(mq, inner).boxed();

    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);

    let captured_mq = captured
        .borrow()
        .clone()
        .expect("MqCapture::render should have been called");

    // After Change 2: tab_bar_height = 49 + viewPadding.bottom = 83.
    // Page child sees viewInsets.bottom = max(150 - 83, 0) = 67.
    //
    // Before Change 2: tab_bar_height = 49 + padding.bottom = 49 + 0 = 49
    // (clamped!), so page child sees viewInsets.bottom = max(150 - 49, 0) = 101.
    // The 34pt gap means chat content would overlap the bar by 34pt.
    assert!(
        (captured_mq.viewInsets.bottom - 67.0).abs() < 1.0,
        "page child viewInsets.bottom should be ~67 (150 - 83), got {} — \
         tab_bar_height is using clamped padding.bottom instead of viewPadding.bottom",
        captured_mq.viewInsets.bottom
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit test_tab_bar_page_view_insets_match_bar_height`
Expected: FAIL — captured `viewInsets.bottom` ≈ 101 (not 67), because `tab_bar_height` uses `parent.padding.bottom` (= 0, clamped) instead of `parent.viewPadding.bottom` (= 34).

- [ ] **Step 3: Apply Change 2 — use `viewPadding.bottom` for `tab_bar_height`**

In `vexo_uikit/src/tab_bar.rs`, find the page `MediaQueryMutator` closure (around line 218-223). The current code is:

```rust
        let page = MediaQueryMutator::new(stack.boxed(), |parent: &MediaQueryData| {
            let tab_bar_height = TAB_BAR_HEIGHT + parent.padding.bottom;
            let mut v = parent.viewInsets;
            v.bottom = (v.bottom - tab_bar_height).max(0.0);
            parent.copy_with_view_insets(v)
        });
```

Change `parent.padding.bottom` to `parent.viewPadding.bottom`:

```rust
        let page = MediaQueryMutator::new(stack.boxed(), |parent: &MediaQueryData| {
            let tab_bar_height = TAB_BAR_HEIGHT + parent.viewPadding.bottom;
            let mut v = parent.viewInsets;
            v.bottom = (v.bottom - tab_bar_height).max(0.0);
            parent.copy_with_view_insets(v)
        });
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo_uikit test_tab_bar_page_view_insets_match_bar_height`
Expected: PASS — captured `viewInsets.bottom` ≈ 67 (150 - 83).

- [ ] **Step 5: Run the full test suite to verify no regressions**

Run: `cargo test -p vexo_uikit && cargo test -p shared_app`
Expected: All tests PASS, including Task 1's test and all existing chat_screen / tab_bar tests.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/tab_bar.rs
git commit -m "fix: page keyboard-avoidance uses viewPadding for tab_bar_height

Change tab_bar_height in the page's MediaQueryMutator from
parent.padding.bottom (clamped by keyboard height) to
parent.viewPadding.bottom (raw home-indicator inset). This matches
the actual bar height from the SafeArea fix (Change 1), so chat
content no longer overlaps the tab bar by 34pt mid-animation."
```
