# Flutter-Style Focus System Design for Vexo

## Goal

Adopt Flutter's focus tree architecture for Vexo's retain-mode pipeline, replacing the current flat single-slot focus model with a sparse focus tree that supports scopes, traversal, callbacks, and keyboard tokens.

## Scope

- Retain-mode pipeline only (immediate-mode pipeline is legacy and will be deprecated)
- Core focus tree (nodes + scopes), traversal, focus change events, keyboard token
- Deferred focus changes (between frames, like Flutter)

## Approach

Create a standalone `FocusManager` module with a sparse focus tree of `FocusNode` and `FocusScopeNode` data stored in a slotmap. Elements opt in via `FocusElement` and `FocusScopeElement`. The focus tree mirrors the element tree structure but only at points where these elements exist.

---

## 1. Core Data Model

### FocusNodeKey

An opaque slotmap key into `FocusManager`'s storage. Elements hold this to reference their focus node.

### FocusNodeData

Stored in the slotmap:

- `parent: Option<FocusNodeKey>` — link to parent node (or scope)
- `children: Vec<FocusNodeKey>` — ordered children (determines traversal order)
- `on_focus_gained: Option<Box<dyn Fn()>>` — callback when node gains primary focus
- `on_focus_lost: Option<Box<dyn Fn()>>` — callback when node loses primary focus
- `can_request_focus: bool` — whether this node can receive focus (default true)
- `skip_traversal: bool` — excluded from Tab traversal but can still receive direct focus
- `keyboard_token: bool` — set true on explicit user focus request, false on automatic focus assignment
- `element_key: Option<ElementKey>` — the element this node is associated with (for dispatching keyboard events)
- `layout_rect: Option<Rect>` — cached layout rect for reading-order traversal (updated during layout phase)

### FocusScopeData

Stored in a `SecondaryMap<FocusNodeKey, FocusScopeData>`, extends `FocusNodeData`:

- `focused_child: Option<FocusNodeKey>` — remembers which child was last focused in this scope
- `focused_child_history: Vec<FocusNodeKey>` — stack of previously focused children (for restore-on-unfocus)
- `traversal_policy: TraversalPolicy` — how Tab/Shift-Tab navigates within this scope

### FocusManager

Owned by `ThreeTreePipeline`:

- `nodes: SlotMap<FocusNodeKey, FocusNodeData>` — all nodes (including scopes)
- `scopes: SecondaryMap<FocusNodeKey, FocusScopeData>` — extra data for scope nodes
- `primary_focus: Option<FocusNodeKey>` — the currently focused node
- `root_scope: FocusNodeKey` — the top-level scope (created on init)
- `pending_focus_gained: Vec<FocusNodeKey>` — nodes that gained focus this frame (deferred callbacks)
- `pending_focus_lost: Vec<FocusNodeKey>` — nodes that lost focus this frame (deferred callbacks)

**Key invariant:** The focus tree is sparse — only elements that opt in via `FocusElement`/`FocusScopeElement` appear in it.

---

## 2. Focus Tree Construction & Lifecycle

### How Elements Opt In

A `FocusElement` wraps a child element and registers a focus node during `mount()`. A `FocusScopeElement` registers a scope node. These mirror Flutter's `Focus` and `FocusScope` widgets.

### Lifecycle

1. **Mount** — `FocusElement::mount()` calls `ctx.focus_manager.create_node()`, gets a `FocusNodeKey`. Attaches to its parent focus node (found by walking up the element tree to the nearest element holding a `FocusNodeKey`). If none found, attaches to the root scope.

2. **Rebuild** — The focus node persists across rebuilds. The `FocusNodeKey` is stored on the element, not the widget configuration. If the element's position in the element tree changes, `reparent()` updates the focus tree parent.

3. **Unmount** — `FocusElement::unmount()` calls `ctx.focus_manager.remove_node(key)`. Removes the node from its parent's children list. If the node was focused, focus moves to the next focusable sibling or clears.

### Parent Resolution

When a `FocusElement` mounts, it walks up the element tree (via `ElementContext::parent()`) until it finds an element holding a `FocusNodeKey` — that's its focus parent. If none found, it attaches to the root scope. This means the focus tree structure mirrors the element tree, but only at the sparse subset where `FocusElement`/`FocusScopeElement` exist.

---

## 3. Focus Requests & Scope Containment

### Requesting Focus

`FocusManager::request_focus(key, user_initiated: bool)`:

1. If `can_request_focus` is false, do nothing.
2. Walk up from the node to find its enclosing `FocusScopeNode`.
3. Set the scope's `focused_child` to this node.
4. Recursively set ancestor scopes' `focused_child` to point toward this node (building the focus chain).
5. Set `primary_focus = Some(key)`.
6. Queue `on_focus_lost` callback for the old primary focus node.
7. Queue `on_focus_gained` callback for the new node.
8. Set `keyboard_token = user_initiated` — `true` when triggered by pointer press or explicit user action, `false` for programmatic requests like autofocus or scope restoration.

### Scope Containment

When a scope has a `focused_child`, focus is "trapped" within that scope. If `request_focus()` is called on a node outside the current scope, it first exits the scope (clearing its `focused_child`), then enters the target scope.

### Unfocus

`FocusManager::unfocus(disposition)`:

- `UnfocusDisposition::RestorePrevious` — restores the scope's previously focused child from the history stack
- `UnfocusDisposition::Clear` — clears focus entirely, sets `primary_focus = None`

### Focus Chain

`FocusManager::focus_chain(key)` — returns the path from `key` up to the root scope. All nodes in this chain have "has focus" status (they contain the primary focus, even though they aren't the primary focus target). Useful for styling (e.g., a form highlighting when any field is focused).

---

## 4. Focus Traversal

### TraversalPolicy

- `WidgetOrder` — Tab order follows the order nodes were added as children of their scope. Default.
- `ReadingOrder` — Tab order follows visual reading order (left-to-right, top-to-bottom based on layout rects). Requires layout data.
- `Custom(Box<dyn TraversalPolicy>)` — user-defined policy.

The `TraversalPolicy` trait:

```rust
trait TraversalPolicy {
    fn find_first(&self, scope: FocusNodeKey, manager: &FocusManager) -> Option<FocusNodeKey>;
    fn find_last(&self, scope: FocusNodeKey, manager: &FocusManager) -> Option<FocusNodeKey>;
    fn next(&self, current: FocusNodeKey, scope: FocusNodeKey, manager: &FocusManager) -> Option<FocusNodeKey>;
    fn previous(&self, current: FocusNodeKey, scope: FocusNodeKey, manager: &FocusManager) -> Option<FocusNodeKey>;
}
```

### Tab Navigation

`FocusManager::traverse_forward()` / `traverse_backward()`:

1. Find the current primary focus node.
2. Find its enclosing scope.
3. Use the scope's traversal policy to determine the next/previous focusable node.
4. If at the boundary of the scope (last child for forward, first for backward), move to the parent scope and continue.
5. If no next node exists, wrap around to the first/last node in the root scope.

### ReadingOrder Implementation

When traversal is requested, collect all focusable nodes in the current scope, sort by layout rects (primary sort by Y, secondary sort by X), and pick the next/previous node. Requires `FocusNodeData.layout_rect` to be populated during the layout phase.

### Skip Traversal

Nodes with `skip_traversal = true` are excluded from Tab navigation but can still receive focus via `request_focus()` (e.g., programmatic focus or click-to-focus).

---

## 5. Focus Change Events & Keyboard Token

### Focus Change Callbacks

- `on_focus_gained: Option<Box<dyn Fn()>>` — fired when this node becomes the primary focus
- `on_focus_lost: Option<Box<dyn Fn()>>` — fired when this node loses primary focus

### Deferred Dispatch

Focus changes are deferred between frames. `FocusManager` maintains `pending_focus_gained` and `pending_focus_lost` queues. After event processing completes, `FocusManager::dispatch_focus_changes()` fires all pending callbacks, then clears the queues. This prevents mid-build mutations.

### Keyboard Token

`FocusNodeData.keyboard_token` — set to `true` when focus is requested via explicit user interaction (pointer press), `false` when focus is assigned automatically (e.g., autofocus, scope restoration, programmatic `request_focus(key, false)`).

`FocusManager::consume_keyboard_token(key)` — returns the token's value and resets it to `false`. This lets platform code (iOS) decide whether to show the soft keyboard only on intentional user focus, not automatic focus shifts.

---

## 6. Integration with the Retain-Mode Pipeline

### Where FocusManager Lives

`FocusManager` is owned by `ThreeTreePipeline`, alongside `ElementRegistry`, `RenderObjectRegistry`, and `BuildOwner`.

### Replacing the Current Flat Focus

The existing `focused_element: Option<ElementKey>` on `ThreeTreePipeline` is removed. Focus state now lives entirely in `FocusManager.primary_focus`. The `ElementKey` → `FocusNodeKey` mapping is stored on each element that holds a focus node.

### Event Dispatch Changes

**Pointer events** — same hit-test + bubbling flow, but:
- When a `FocusElement` is in the hit path and receives a pointer press, it calls `focus_manager.request_focus(my_key, true)` (user-initiated) instead of `context.request_focus(element_key)`
- `EventContext` no longer needs `focus_request` / `clear_focus_request` fields — focus requests go directly to `FocusManager`

**Keyboard events** — routed to the primary focus node's element:
1. `FocusManager::primary_focus()` → `FocusNodeKey`
2. Look up the `ElementKey` from `FocusNodeData.element_key`
3. Dispatch keyboard event to that element
4. If the key is Tab/Shift+Tab, call `traverse_forward()`/`traverse_backward()` instead

**Click-outside-to-unfocus** — if hit test finds no target and the event is a pointer press, call `focus_manager.unfocus(UnfocusDisposition::Clear)`.

### Build-Time Focus Queries

`BuildContext::is_focused()` now checks `FocusManager::is_focused(key)` instead of comparing `ElementKey`. `BuildOwner` no longer stores `focused_element` — it reads from `FocusManager` directly (or receives a snapshot before builds, like the current `sync_focus_to_build_owner()` pattern).

### FocusElement and FocusScopeElement

New element types that wrap a child:

- `FocusElement` — creates a `FocusNode` on mount, holds the `FocusNodeKey`, passes events to child
- `FocusScopeElement` — creates a `FocusScopeNode`, holds traversal policy and scope configuration

Widget API:

```rust
Focus::new(child_widget)                    // focusable wrapper
Focus::new(child_widget).autofocus(true)    // autofocus on mount
FocusScope::new(child_widget)               // scope boundary
FocusScope::new(child_widget).policy(TraversalPolicy::ReadingOrder)
```

### Autofocus

When `Focus::new().autofocus(true)` is used, the `FocusElement` calls `focus_manager.request_focus(my_key, false)` during mount if no other node in its enclosing scope already has focus. The `false` flag means the keyboard token is not set — autofocus is not a user-initiated action.

---

## 7. Testing Strategy

### Unit Tests (no GPU, no element tree)

- `FocusManager` creation, node insertion/removal
- `request_focus()` — basic focus change, `can_request_focus = false` rejection
- Scope containment — focus stays within scope, scope remembers `focused_child`
- Focus chain — correct path from primary to root
- Unfocus dispositions — restore previous vs clear
- Traversal — Tab/Shift+Tab with `WidgetOrder` policy (no layout needed)
- Callbacks — `on_focus_gained`/`on_focus_lost` fire correctly, deferred dispatch
- Keyboard token — set on explicit request, consumed correctly, not set on automatic focus

### Integration Tests (with element tree, no GPU)

- `FocusElement` mount/unmount — node appears/disappears in focus tree
- `FocusScopeElement` — scope boundary respected during traversal
- Click-to-focus — pointer press on a `FocusElement` requests focus
- Click-outside-to-unfocus — pointer press outside any focusable element clears focus
- Tab navigation — Tab key moves focus through elements in correct order
- Focus-dependent build — `BuildContext::is_focused()` reflects focus state during rebuild
- Autofocus — `Focus::new().autofocus(true)` grabs focus on mount

### Deferred for Later

- `ReadingOrder` traversal (needs layout rect integration, defer until layout pipeline is clearer)
- Directional traversal (arrow keys)
- Highlight mode (traditional vs touch)
- Immediate-mode pipeline focus (will be deprecated)

---

## 8. Module Structure

```
vexo/src/retain/
├── focus/
│   ├── mod.rs              # Public API exports
│   ├── manager.rs          # FocusManager (slotmap, primary_focus, root_scope)
│   ├── node.rs             # FocusNodeData, FocusNodeKey
│   ├── scope.rs            # FocusScopeData, UnfocusDisposition
│   ├── traversal.rs        # TraversalPolicy trait, WidgetOrderPolicy, ReadingOrderPolicy
│   ├── element.rs          # FocusElement, FocusScopeElement
│   └── widget.rs           # Focus widget, FocusScope widget
```

`FocusManager` is also accessible via `ElementContext` and `BuildContext` so elements can query and request focus during mount, event handling, and build.

---

## 9. Key Differences from Flutter's Implementation

| Aspect | Flutter | Vexo |
|--------|---------|-------|
| Node ownership | Persistent objects owned by widget State | Slotmap entries owned by FocusManager |
| Node identity | FocusNode object reference | FocusNodeKey (opaque slotmap key) |
| Scope inheritance | FocusScopeNode extends FocusNode (class inheritance) | FocusScopeData stored in SecondaryMap (Rust has no inheritance) |
| Focus change timing | Microtask (Dart async) | Deferred dispatch between frames (Rust has no async microtasks) |
| Traversal policy objects | Dart class hierarchy | Rust trait + enum for built-in policies |
| Callbacks | Dart Function closures | Rust `Box<dyn Fn()>` closures |