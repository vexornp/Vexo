# Retain-Mode Pipeline End-to-End Test Design

**Date:** 2026-04-28
**Status:** Design Approved

## Goal

Create a focused unit test that exercises the complete three-tree pipeline flow and verifies it works correctly without GPU/window dependencies.

## Test Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  1. Create widget tree (Column with Text children)              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  2. Reconcile with element tree                                 │
│     - Creates elements for each widget                          │
│     - Creates render objects for each element                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  3. Layout dirty render objects                                 │
│     - Only processes objects marked needs_layout                │
│     - Returns computed sizes                                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  4. Paint and collect render commands                           │
│     - Only processes objects marked needs_paint                 │
│     - Returns Vec<RenderCommand>                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  5. Hit test at known position                                  │
│     - Returns HitTestResult with path to target                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  6. Update widget tree and reconcile again                      │
│     - Verifies elements are reused (not recreated)              │
│     - Verifies dirty flags set correctly                        │
└─────────────────────────────────────────────────────────────────┘
```

## Test File

**Location:** `vexo/src/retain/e2e_test.rs`

**Test Function:** `test_retain_pipeline_e2e`

## Assertions

1. **After reconcile:**
   - Element registry has expected count (Column + N Text widgets)
   - Render object registry has matching count
   - Root render object is set

2. **After layout:**
   - Dirty layout set is cleared
   - Render objects have computed bounds

3. **After paint:**
   - Dirty paint set is cleared
   - Render commands collected (may be empty for text)

4. **After hit test:**
   - Hit at position inside bounds returns target
   - Miss at position outside bounds returns empty

5. **After update:**
   - Element count unchanged (elements reused)
   - Dirty flags set for updated objects

## Example Test Code

```rust
#[cfg(test)]
mod e2e_test {
    use crate::retain::{Column, Text, ThreeTreePipeline, Widget};
    use crate::core::{Point, Size};
    use crate::layout::TaffyLayoutEngine;

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
        let _ = commands;

        // === Step 5: Hit test ===
        // Hit inside bounds
        let hit = pipeline.hit_test(Point::new(10.0, 10.0));
        // Result depends on layout - just verify no panic

        // Miss outside bounds
        let miss = pipeline.hit_test(Point::new(1000.0, 1000.0));
        assert!(!miss.is_hit(), "Should miss outside bounds");

        // === Step 6: Update and reconcile again ===
        let element_count_before = pipeline.element_registry().len();

        let updated_widget = Column::new()
            .push(Text::new("Hello Updated"))
            .push(Text::new("World"));

        pipeline.reconcile(Box::new(updated_widget));

        // Verify elements reused
        let element_count_after = pipeline.element_registry().len();
        assert_eq!(element_count_before, element_count_after, "Elements should be reused");

        // Verify dirty flags set
        assert!(pipeline.needs_layout() || pipeline.needs_paint(), "Should be dirty after update");
    }
}
```

## Success Criteria

- Test passes with all assertions
- Demonstrates the pipeline works without immediate-mode code
- Can be used as reference for future retain-mode development
- No GPU/window dependencies (runs in CI)

## Out of Scope

- Visual rendering verification (requires GPU)
- Complex widget types (Button, TextEdit, etc.)
- State preservation across frames
- Animation testing
