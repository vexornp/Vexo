use super::*;

/// Helper: create a FocusManager with N child nodes under the root scope.
/// Returns (manager, [node_keys]).
fn make_nodes(n: usize) -> (FocusManager, Vec<FocusNodeKey>) {
    let mut mgr = FocusManager::new();
    let keys: Vec<FocusNodeKey> = (0..n)
        .map(|_| mgr.create_node(None))
        .collect();
    (mgr, keys)
}

#[test]
fn test_create_node() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    // Newly created node is not focused.
    assert!(!mgr.is_focused(node));
    // can_request_focus defaults to true.
    assert!(mgr.can_request_focus(node));
}

#[test]
fn test_create_scope() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    assert!(mgr.is_scope(scope));
}

#[test]
fn test_request_focus_basic() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, true);
    assert!(mgr.is_focused(node));
    assert_eq!(mgr.primary_focus(), Some(node));
}

#[test]
fn test_request_focus_replaces_previous() {
    let (mut mgr, nodes) = make_nodes(2);
    let a = nodes[0];
    let b = nodes[1];

    mgr.request_focus(a, true);
    assert!(mgr.is_focused(a));

    mgr.request_focus(b, true);
    assert!(!mgr.is_focused(a));
    assert!(mgr.is_focused(b));
    assert_eq!(mgr.primary_focus(), Some(b));
}

#[test]
fn test_request_focus_can_request_focus_false() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.set_can_request_focus(node, false);
    mgr.request_focus(node, true);
    // Focus request should be a no-op.
    assert!(!mgr.is_focused(node));
    assert_eq!(mgr.primary_focus(), None);
}

#[test]
fn test_unfocus_clear() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, true);
    assert!(mgr.is_focused(node));

    mgr.unfocus(UnfocusDisposition::Clear);
    assert!(!mgr.is_focused(node));
    assert_eq!(mgr.primary_focus(), None);
}

#[test]
fn test_unfocus_restore_previous() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let a = mgr.create_node(Some(scope));
    let b = mgr.create_node(Some(scope));

    // Focus a, then b. The scope tracks history.
    mgr.request_focus(a, true);
    mgr.request_focus(b, true);
    assert!(mgr.is_focused(b));

    // Unfocus b with RestorePrevious should restore a.
    mgr.unfocus(UnfocusDisposition::RestorePrevious);
    assert!(mgr.is_focused(a));
    assert!(!mgr.is_focused(b));
    assert_eq!(mgr.primary_focus(), Some(a));
}

#[test]
fn test_focus_chain() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let node = mgr.create_node(Some(scope));

    mgr.request_focus(node, true);

    let chain = mgr.focus_chain();
    // Chain should contain the focused node and its ancestors up to root.
    assert!(chain.contains(&node));
    assert!(chain.contains(&scope));
}

#[test]
fn test_has_focus_ancestor() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let node = mgr.create_node(Some(scope));

    mgr.request_focus(node, true);

    // The scope should have_focus because its focused_child chain leads to
    // the primary focus.
    assert!(mgr.has_focus(scope));
    assert!(mgr.has_focus(node));
}

#[test]
fn test_keyboard_token_user_initiated() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, true);

    // First consume should return true (user-initiated).
    assert!(mgr.consume_keyboard_token(node));
    // Second consume should return false (already consumed).
    assert!(!mgr.consume_keyboard_token(node));
}

#[test]
fn test_keyboard_token_programmatic() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, false);

    // Programmatic focus: token should be false.
    assert!(!mgr.consume_keyboard_token(node));
}

#[test]
fn test_remove_node() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, true);
    assert_eq!(mgr.primary_focus(), Some(node));

    mgr.remove_node(node);
    assert_eq!(mgr.primary_focus(), None);
}

#[test]
fn test_remove_node_from_parent_children() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let node = mgr.create_node(Some(scope));

    // Node should be in the scope's children.
    assert!(mgr.children(scope).contains(&node));

    mgr.remove_node(node);

    // Node should no longer be in the scope's children.
    assert!(!mgr.children(scope).contains(&node));
}

#[test]
fn test_skip_traversal() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);

    // Default is false.
    assert!(!mgr.skip_traversal(node));

    mgr.set_skip_traversal(node, true);
    assert!(mgr.skip_traversal(node));

    mgr.set_skip_traversal(node, false);
    assert!(!mgr.skip_traversal(node));
}

#[test]
fn test_traverse_forward_widget_order() {
    let (mut mgr, nodes) = make_nodes(3);
    let a = nodes[0];
    let b = nodes[1];
    let c = nodes[2];

    mgr.request_focus(a, false);
    let next = mgr.traverse_forward();
    assert_eq!(next, Some(b));
    assert!(mgr.is_focused(b));
}

#[test]
fn test_traverse_forward_wraps_around() {
    let (mut mgr, nodes) = make_nodes(2);
    let a = nodes[0];
    let b = nodes[1];

    mgr.request_focus(b, false);
    let next = mgr.traverse_forward();
    assert_eq!(next, Some(a));
    assert!(mgr.is_focused(a));
}

#[test]
fn test_traverse_backward_widget_order() {
    let (mut mgr, nodes) = make_nodes(2);
    let a = nodes[0];
    let b = nodes[1];

    mgr.request_focus(b, false);
    let prev = mgr.traverse_backward();
    assert_eq!(prev, Some(a));
    assert!(mgr.is_focused(a));
}

#[test]
fn test_traverse_skips_non_focusable() {
    let (mut mgr, nodes) = make_nodes(3);
    let a = nodes[0];
    let b = nodes[1];
    let c = nodes[2];

    // Make the middle node non-focusable.
    mgr.set_can_request_focus(b, false);

    mgr.request_focus(a, false);
    let next = mgr.traverse_forward();
    // Should skip b and land on c.
    assert_eq!(next, Some(c));
    assert!(mgr.is_focused(c));
}

#[test]
fn test_traverse_skips_skip_traversal() {
    let (mut mgr, nodes) = make_nodes(3);
    let a = nodes[0];
    let b = nodes[1];
    let c = nodes[2];

    // Mark the middle node as skip_traversal.
    mgr.set_skip_traversal(b, true);

    mgr.request_focus(a, false);
    let next = mgr.traverse_forward();
    // Should skip b and land on c.
    assert_eq!(next, Some(c));
    assert!(mgr.is_focused(c));
}

#[test]
fn test_scope_traversal_policy() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    mgr.set_traversal_policy(scope, TraversalPolicy::WidgetOrder);

    let a = mgr.create_node(Some(scope));
    let b = mgr.create_node(Some(scope));

    mgr.request_focus(a, false);
    let next = mgr.traverse_forward();
    assert_eq!(next, Some(b));
}

#[test]
fn test_deferred_callbacks() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let mut mgr = FocusManager::new();
    let a = mgr.create_node(None);
    let b = mgr.create_node(None);

    let gained_a = Rc::new(RefCell::new(0));
    let lost_a = Rc::new(RefCell::new(0));
    let gained_b = Rc::new(RefCell::new(0));
    let lost_b = Rc::new(RefCell::new(0));

    let ga = gained_a.clone();
    let la = lost_a.clone();
    let gb = gained_b.clone();
    let lb = lost_b.clone();

    mgr.set_callbacks(
        a,
        Some(Box::new(move || { *ga.borrow_mut() += 1; })),
        Some(Box::new(move || { *la.borrow_mut() += 1; })),
    );
    mgr.set_callbacks(
        b,
        Some(Box::new(move || { *gb.borrow_mut() += 1; })),
        Some(Box::new(move || { *lb.borrow_mut() += 1; })),
    );

    // Focus a — should queue gained_a.
    mgr.request_focus(a, true);
    mgr.dispatch_focus_changes();
    assert_eq!(*gained_a.borrow(), 1);
    assert_eq!(*lost_a.borrow(), 0);

    // Focus b — should queue lost_a and gained_b.
    mgr.request_focus(b, true);
    mgr.dispatch_focus_changes();
    assert_eq!(*lost_a.borrow(), 1);
    assert_eq!(*gained_b.borrow(), 1);
}

#[test]
fn test_enclosing_scope() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let node = mgr.create_node(Some(scope));

    // The enclosing scope of node should be `scope`.
    assert_eq!(mgr.enclosing_scope(node), Some(scope));
}

#[test]
fn test_focused_element() {
    use slotmap::SlotMap;
    use crate::retain::id::ElementKey;

    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);

    // Create an ElementKey and assign it to the node.
    let mut elements: SlotMap<ElementKey, ()> = SlotMap::with_key();
    let ek = elements.insert(());
    mgr.set_element_key(node, Some(ek));

    // Before focus, focused_element is None.
    assert_eq!(mgr.focused_element(), None);

    // After focus, focused_element returns the ElementKey.
    mgr.request_focus(node, true);
    assert_eq!(mgr.focused_element(), Some(ek));
}
