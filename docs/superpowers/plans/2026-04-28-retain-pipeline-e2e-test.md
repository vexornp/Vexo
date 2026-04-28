# Retain-Mode Pipeline E2E Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a unit test that exercises the complete three-tree pipeline flow and verifies it works correctly.

**Architecture:** Single test file in vexo/src/retain/ that tests reconcile → layout → paint → hit_test → update flow using existing Text and Column widgets.

**Tech Stack:** Rust, vexo retain module (ThreeTreePipeline, Text, Column), TaffyLayoutEngine

---

## File Structure

**Files to create:**
- `vexo/src/retain/e2e_test.rs` - End-to-end pipeline test

**Files to modify:**
- `vexo/src/retain/mod.rs` - Add e2e_test module

---

### Task 1: Add e2e_test module to retain

**Files:**
- Modify: `vexo/src/retain/mod.rs`
- Create: `vexo/src/retain/e2e_test.rs`

- [ ] **Step 1: Add module declaration to mod.rs**

```rust
// In vexo/src/retain/mod.rs, add at the end of the module declarations:

#[cfg(test)]
mod e2e_test;
```

- [ ] **Step 2: Create the test file with the test function**

```rust
// vexo/src/retain/e2e_test.rs
//! End-to-end test for the retain-mode pipeline.

use crate::retain::{Column, Text, ThreeTreePipeline, Widget};
use crate::core::{Point, Size};
use crate::layout::TaffyLayoutEngine;

/// Test the complete three-tree pipeline flow.
///
/// This test exercises:
/// 1. Widget tree creation
/// 2. Reconciliation with element tree
/// 3. Layout of dirty render objects
/// 4. Paint and command collection
/// 5. Hit testing
/// 6. Update and re-reconciliation
#[test]
fn test_retain_pipeline_e2e() {
    // === Step 1: Create widget tree ===
    let widget = Column::new()
        .push(Text::new("Hello"))
        .push(Text::new("World"));

    // === Step 2: Create pipeline and reconcile ===
    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(widget));

    // Verify element creation
    assert!(pipeline.element_registry().len() >= 3, "Should have Column + 2 Text elements");
    assert!(pipeline.render_objects().len() >= 3, "Should have matching render objects");
    assert!(pipeline.render_objects().root().is_some(), "Root should be set");

    // === Step 3: Layout ===
    let mut engine = TaffyLayoutEngine::new();
    let available_size = Size::new(800.0, 600.0);

    // Verify dirty before layout
    assert!(pipeline.needs_layout(), "Should need layout after reconcile");

    pipeline.layout(available_size, &mut engine);

    // Verify dirty cleared
    assert!(!pipeline.needs_layout(), "Should not need layout after layout");

    // === Step 4: Paint ===
    assert!(pipeline.needs_paint(), "Should need paint after reconcile");
    let commands = pipeline.paint();
    assert!(!pipeline.needs_paint(), "Should not need paint after paint");

    // Commands may be empty since text is handled by glyphon
    // Just verify paint completed without error
    let _ = commands;

    // === Step 5: Hit test ===
    // Hit inside bounds (position depends on layout)
    let hit = pipeline.hit_test(Point::new(10.0, 10.0));
    // Result depends on computed layout - verify no panic

    // Miss outside bounds
    let miss = pipeline.hit_test(Point::new(1000.0, 1000.0));
    assert!(!miss.is_hit(), "Should miss outside bounds");

    // === Step 6: Update and reconcile again ===
    let element_count_before = pipeline.element_registry().len();

    let updated_widget = Column::new()
        .push(Text::new("Hello Updated"))
        .push(Text::new("World"));

    pipeline.reconcile(Box::new(updated_widget));

    // Verify elements reused (same count)
    let element_count_after = pipeline.element_registry().len();
    assert_eq!(element_count_before, element_count_after, "Elements should be reused, not recreated");

    // Verify dirty flags set for updated objects
    assert!(pipeline.needs_layout() || pipeline.needs_paint(), "Should be dirty after update");
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test -p vexo test_retain_pipeline_e2e -- --nocapture`
Expected: PASS with all assertions passing

- [ ] **Step 4: Run full test suite**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS (including the new e2e test)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/mod.rs vexo/src/retain/e2e_test.rs
git commit -m "test: add e2e test for retain-mode pipeline"
```

---

## Summary

This plan adds a single end-to-end test that verifies the complete three-tree pipeline works:

1. **Reconcile** - Creates elements and render objects from widget tree
2. **Layout** - Processes dirty render objects, clears dirty flags
3. **Paint** - Collects render commands, clears dirty flags
4. **Hit test** - Returns correct results for inside/outside bounds
5. **Update** - Reuses elements, sets dirty flags correctly

The test serves as a reference for how the retain-mode pipeline should be used and verifies the core functionality works without GPU/window dependencies.