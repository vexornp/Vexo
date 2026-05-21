# Flutter-Style Focus Tree for Vexo

## Goal

Replace the flat `Option<ElementKey>` focus model in Vexo's retain-mode pipeline with a sparse focus tree (FocusNode/FocusScopeNode) that mirrors Flutter's architecture. This migration delivers scope-based focus memory and deferred focus changes, while keeping each step minor and verifiable.

## Scope

- Retain-mode pipeline only (immediate-mode is legacy)
- Focus tree (FocusNode + FocusScopeNode), FocusManager, deferred changes, scope memory
- FocusElement/FocusScopeElement as explicit wrappers (opt-in, not automatic)
- No Tab/Shift-Tab traversal, no directional navigation, no keyboard tokens, no highlight modes (deferred to future work)

## Why This Design

Flutter's focus system uses a separate sparse tree that mirrors the widget tree but only contains nodes that care about focus. This separation means:
- Focus state doesn't pollute the element tree
- Scopes can remember which child was last focused (enabling focus restoration)
- Focus changes can be deferred and batched (preventing inconsistent mid-frame state)
- The tree structure enables future traversal (Tab/arrow keys) without redesign

The previous attempt (reverted in commit 5ba0166) tried to build everything at once. This design takes a vertical-slice approach: build FocusNode + FocusScopeNode together (they're the same type hierarchy), then integrate incrementally.

---

## 1. Focus Tree Architecture

The focus tree is a separate, sparser tree that mirrors the element tree but only contains nodes where elements opt in via FocusElement/FocusScopeElement.

```
Element Tree:              Focus Tree:
RootEl                      RootScope (always present)
├── FocusScopeEl            └── ScopeNode (FocusScope)
│   ├── FocusEl                 ├── FocusNode (TextEdit) ← primary focus
│   │   └── TextEditEl          └── FocusNode (Button)
│   └── FocusEl
│       └── ButtonEl
└── ContainerEl
    └── TextEl              (no focus nodes — nothing opted in)
```

### FocusNode

The core unit in the focus tree:

- `id: FocusNodeId` — slotmap key (stable across removals)
- `element_key: Option<ElementKey>` — links back to the element
- `parent: Option<FocusNodeId>` — tree structure
- `children: Vec<FocusNodeId>` — ordered children (determines future traversal order)
- `can_request_focus: bool` — whether this node can receive focus (default true)
- `skip_traversal: bool` — excluded from Tab traversal but can receive direct focus
- `on_focus_gained: Option<Box<dyn Fn()>>` — callback when node gains primary focus
- `on_focus_lost: Option<Box<dyn Fn()>>` — callback when node loses primary focus

Computed (not stored):
- `has_focus` — true if this node or any descendant is the primary focus
- `has_primary_focus` — true only if this node is the primary focus

### FocusScopeNode

Extends FocusNode (stored in a SecondaryMap since Rust has no inheritance):

- `focused_children: Vec<FocusNodeId>` — stack of recently-focused children (most recent at end)
- `traversal_edge_behavior: TraversalEdgeBehavior` — what happens at Tab boundaries (for future traversal)

The `focused_children` stack is the key mechanism for focus memory. When a node N gains primary focus, walk up all ancestor FocusScopeNodes and push N to each scope's stack. When a scope regains focus, pop the last entry and descend through nested scopes to find the leaf.

### FocusAttachment

The glue between an element and the focus tree:

- Created when a FocusElement mounts
- `reparent()` — called during rebuild to keep focus tree synced with element tree
  - Finds the nearest parent FocusScopeElement in the element tree
  - Falls back to root_scope if none found
  - Removes from old parent, adds to new parent
  - If the reparented node had focus, restores it through the new path
- `detach()` — called on unmount, removes node from focus tree

### TraversalEdgeBehavior (for future use)

```rust
enum TraversalEdgeBehavior {
    ClosedLoop,    // Wrap around within scope
    ParentScope,   // Exit to parent scope
    Stop,          // Stay at current position
}
```

---

## 2. FocusManager

Owned by `ThreeTreePipeline`:

- `nodes: SlotMap<FocusNodeId, FocusNodeData>` — all nodes (including scopes)
- `scopes: SecondaryMap<FocusNodeId, FocusScopeData>` — extra data for scope nodes
- `root_scope: FocusNodeId` — the top-level scope (created on init)
- `primary_focus: Option<FocusNodeId>` — the currently focused node
- `pending_focus_request: Option<FocusNodeId>` — deferred focus change
- `dirty_nodes: HashSet<FocusNodeId>` — nodes whose has_focus changed, need notification

### Deferred Focus Changes

Focus changes are deferred between frames, matching Flutter's microtask batching:

1. `request_focus(node_id)` → sets `pending_focus_request`, marks manager dirty
2. Multiple `request_focus()` calls in one frame → only the last one wins (coalesced)
3. At end of event processing, pipeline calls `apply_focus_changes()`
4. `apply_focus_changes()`:
   - Computes old focus path (ancestors of old primary_focus)
   - Computes new focus path (ancestors of new pending_focus_request)
   - Finds nodes that gained `has_focus` (in new path, not in old)
   - Finds nodes that lost `has_focus` (in old path, not in new)
   - Sets `primary_focus = pending_focus_request`
   - Fires `on_focus_lost` for old node, `on_focus_gained` for new node
   - Notifies dirty nodes

**Why deferred?** If TextEdit A loses focus and TextEdit B gains focus in the same frame, immediate changes would notify shared ancestors twice. Deferred computes the diff once, notifies each node exactly once.

### Unfocus

```rust
enum UnfocusDisposition {
    RestorePrevious,  // Restore scope's previously focused child from history
    Clear,            // Clear focus entirely
}
```

---

## 3. Element Integration

### FocusElement

A new element type that wraps a single child:

- Holds a `FocusNodeId` (or `FocusScopeNodeId` if configured as scope)
- On `mount()`: creates the focus node, attaches to parent scope via FocusAttachment
- On `rebuild()`: calls `attachment.reparent()` to sync with element tree
- On `unmount()`: removes node from focus tree, disposes attachment

### FocusScopeElement

Extends FocusElement:

- Creates a FocusScopeNode instead of FocusNode
- Same mount/rebuild/unmount lifecycle

### Parent Scope Resolution

When a FocusElement mounts, walk up the element tree via parent pointer to find the nearest FocusScopeElement. That's the focus parent. If none found, attach to root_scope.

This is the Vexo equivalent of Flutter's `Focus.maybeOf(context)` (InheritedWidget lookup). Simpler because Vexo elements have explicit parent pointers.

### Widget API

```rust
// Focus wrapper (creates FocusNode)
Focus::new(child).on_focus(|ctx| ...).on_blur(|ctx| ...)

// FocusScope wrapper (creates FocusScopeNode)
FocusScope::new(child).on_focus(|ctx| ...).on_blur(|ctx| ...)

// Usage in app view()
FocusScope::new(
    Column::new(vec![
        Focus::new(TextEdit::new("editor1")),
        Focus::new(TextEdit::new("editor2")),
        Focus::new(Button::new("Submit")),
    ])
)
```

---

## 4. Migration Steps

### Step 1: FocusNode + FocusScopeNode data model (no integration)

Create `vexo/src/retain/focus/` module with pure data structures:
- `FocusNodeId` (slotmap key)
- `FocusNodeData` (parent, children, can_request_focus, skip_traversal, element_key)
- `FocusScopeData` (focused_children stack, via SecondaryMap)
- `FocusManager` (slotmap storage, root_scope, primary_focus)
- Basic operations: `create_node()`, `create_scope()`, `remove_node()`, `reparent()`
- `request_focus()` is immediate in this step (no deferred changes yet)
- Unit tests only — no changes to existing code

**Verify:** `cargo test` passes, no changes to existing code.

### Step 2: FocusManager integration into pipeline

- Add `FocusManager` field to `ThreeTreePipeline`
- Replace `focused_element: Option<ElementKey>` with `focus_manager.primary_focus()`
- Update `sync_focus_to_build_owner()` to read from FocusManager
- Update `EventContext` to read focus state from FocusManager
- No FocusElement yet — TextEdit still uses `context.request_focus(element_key)`, but pipeline routes it through FocusManager
- FocusManager maintains a `HashMap<ElementKey, FocusNodeId>` for this lookup during the transition

**Verify:** Existing TextEdit click-to-focus still works, keyboard input still works.

### Step 3: FocusElement + FocusScopeElement

- Add `FocusElement` and `FocusScopeElement` element types
- Add `Focus` and `FocusScope` widget types
- On mount: create focus node/scope, attach to parent scope
- On rebuild: reparent
- On unmount: remove from focus tree
- No change to TextEdit yet — FocusElement is available but not used

**Verify:** Unit tests for FocusElement mount/unmount/reparent, existing tests still pass.

### Step 4: Migrate TextEdit to use FocusElement

- Wrap TextEdit with `Focus::new()` in the demo app
- TextEdit's `on_event()` no longer calls `context.request_focus()` directly — FocusElement wrapper handles it
- TextEdit's `build()` reads focus from `BuildContext::is_focused()` (unchanged API, different source)
- Remove `focus_request`/`clear_focus_request` from `EventContext`

**Verify:** TextEdit click-to-focus works, keyboard input works, border color changes on focus.

### Step 5: Deferred focus changes

- Add `pending_focus_request` and `dirty_nodes` to FocusManager
- `request_focus()` becomes deferred — sets pending, doesn't commit
- Pipeline calls `apply_focus_changes()` at end of event processing
- Add `on_focus_gained`/`on_focus_lost` callbacks to FocusNodeData
- Callbacks fire during `apply_focus_changes()`

**Verify:** Focus change callbacks fire correctly, multiple requests in one frame are coalesced.

### Step 6: Scope focus memory

- When a node gains focus, walk up ancestor scopes and push to `focused_children`
- When a scope regains focus, descend through `focused_children` to restore the leaf
- `unfocus()` with `RestorePrevious` disposition uses the history stack
- `unfocus()` with `Clear` disposition clears focus entirely

**Verify:** Tab away from a scope and back restores the previously focused child.

---

## 5. Module Structure

```
vexo/src/retain/
├── focus/
│   ├── mod.rs              # Public API exports
│   ├── manager.rs          # FocusManager (slotmap, primary_focus, root_scope, deferred changes)
│   ├── node.rs             # FocusNodeData, FocusNodeId
│   ├── scope.rs            # FocusScopeData, UnfocusDisposition, TraversalEdgeBehavior
│   ├── attachment.rs       # FocusAttachment (reparent/detach glue)
│   ├── element.rs          # FocusElement, FocusScopeElement
│   └── widget.rs           # Focus widget, FocusScope widget
```

---

## 6. Key Differences from Flutter

| Aspect | Flutter | Vexo |
|--------|---------|-------|
| Node ownership | Persistent objects owned by widget State | Slotmap entries owned by FocusManager |
| Node identity | FocusNode object reference | FocusNodeId (opaque slotmap key) |
| Scope extension | FocusScopeNode extends FocusNode (class inheritance) | FocusScopeData stored in SecondaryMap (Rust has no inheritance) |
| Focus change timing | Microtask (Dart async) | Deferred dispatch between frames (no async runtime) |
| Callbacks | Dart Function closures | Rust `Box<dyn Fn()>` closures |
| Parent scope lookup | InheritedWidget (Focus.maybeOf) | Element tree parent walk |

---

## 7. Testing Strategy

### Unit Tests (no GPU, no element tree)

- FocusManager creation, node insertion/removal
- `request_focus()` — basic focus change, `can_request_focus = false` rejection
- Scope containment — focus stays within scope, scope remembers `focused_child`
- Unfocus dispositions — restore previous vs clear
- Deferred changes — pending request coalescing, `apply_focus_changes()` correctness
- Callbacks — `on_focus_gained`/`on_focus_lost` fire correctly

### Integration Tests (with element tree, no GPU)

- FocusElement mount/unmount — node appears/disappears in focus tree
- FocusScopeElement — scope boundary respected
- Click-to-focus — pointer press on a FocusElement requests focus
- Click-outside-to-unfocus — pointer press outside any focusable element clears focus
- Focus-dependent build — `BuildContext::is_focused()` reflects focus state during rebuild

### Deferred for Later

- Tab/Shift-Tab traversal (needs TraversalPolicy)
- ReadingOrder traversal (needs layout rect integration)
- Directional traversal (arrow keys)
- Keyboard token (for iOS soft keyboard)
- Highlight mode (traditional vs touch)
- Immediate-mode pipeline focus (will be deprecated)
