# Generic Container Widgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `Column` and `Row` widgets generic over message type `M` to enable composition with message-emitting widgets like `Button<M>`.

**Architecture:** Add generic type parameter `M` to `Column` and `Row` structs with default `M = ()` for backward compatibility. Follow the pattern established by `GestureDetector<M>` and `Button<M>`.

**Tech Stack:** Rust, retain mode widget system

---

## File Structure

- **Modify:** `vexo/src/retain/widgets/container.rs` - Make Column and Row generic over M

---

### Task 1: Make Column Generic Over M

**Files:**
- Modify: `vexo/src/retain/widgets/container.rs:10-83`

- [ ] **Step 1: Update Column struct to be generic**

Replace the Column struct definition (lines 10-13):

```rust
/// Column widget - arranges children vertically.
pub struct Column<M: Clone + Send + 'static = ()> {
    key: Option<Key>,
    children: Vec<Box<dyn Widget<M>>>,
}
```

- [ ] **Step 2: Update Column impl block**

Replace the Column impl block (lines 15-39):

```rust
impl<M: Clone + Send + 'static> Column<M> {
    /// Create a new empty column.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child widget.
    pub fn push(mut self, child: impl Widget<M> + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget<M>>] {
        &self.children
    }
}
```

- [ ] **Step 3: Update Column Default impl**

Replace the Default impl (lines 42-45):

```rust
impl<M: Clone + Send + 'static> Default for Column<M> {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Update Column Clone impl**

Replace the Clone impl (lines 48-54):

```rust
impl<M: Clone + Send + 'static> Clone for Column<M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_box()).collect(),
        }
    }
}
```

- [ ] **Step 5: Update Column Widget impl**

Replace the Widget impl (lines 57-83):

```rust
impl<M: Clone + Send + 'static> Widget<M> for Column<M> {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ContainerElement::<M>::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_column())
    }

    fn clone_box(&self) -> Box<dyn Widget<M>> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget<M>>] {
        &self.children
    }
}
```

---

### Task 2: Make Row Generic Over M

**Files:**
- Modify: `vexo/src/retain/widgets/container.rs:86-159`

- [ ] **Step 1: Update Row struct to be generic**

Replace the Row struct definition (lines 86-89):

```rust
/// Row widget - arranges children horizontally.
pub struct Row<M: Clone + Send + 'static = ()> {
    key: Option<Key>,
    children: Vec<Box<dyn Widget<M>>>,
}
```

- [ ] **Step 2: Update Row impl block**

Replace the Row impl block (lines 91-115):

```rust
impl<M: Clone + Send + 'static> Row<M> {
    /// Create a new empty row.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child widget.
    pub fn push(mut self, child: impl Widget<M> + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget<M>>] {
        &self.children
    }
}
```

- [ ] **Step 3: Update Row Default impl**

Replace the Default impl (lines 118-121):

```rust
impl<M: Clone + Send + 'static> Default for Row<M> {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Update Row Clone impl**

Replace the Clone impl (lines 124-130):

```rust
impl<M: Clone + Send + 'static> Clone for Row<M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_box()).collect(),
        }
    }
}
```

- [ ] **Step 5: Update Row Widget impl**

Replace the Widget impl (lines 133-159):

```rust
impl<M: Clone + Send + 'static> Widget<M> for Row<M> {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ContainerElement::<M>::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_row())
    }

    fn clone_box(&self) -> Box<dyn Widget<M>> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget<M>>] {
        &self.children
    }
}
```

---

### Task 3: Verify Build and Tests

**Files:**
- Test: `vexo/src/retain/widgets/container.rs` (existing tests)

- [ ] **Step 1: Run cargo build**

Run: `cargo build -p vexo`
Expected: Build succeeds with no errors

- [ ] **Step 2: Run cargo test**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 3: Commit changes**

```bash
git add vexo/src/retain/widgets/container.rs
git commit -m "feat: make Column and Row widgets generic over message type M"
```
