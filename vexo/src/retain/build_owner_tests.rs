//! Unit tests for BuildOwner.

use super::build_owner::BuildOwner;
use super::id::ElementKey;

fn make_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn make_two_keys() -> (ElementKey, ElementKey) {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    let k1 = sm.insert(());
    let k2 = sm.insert(());
    (k1, k2)
}

#[test]
fn test_build_owner_new() {
    let owner = BuildOwner::new();

    assert!(!owner.has_pending_rebuilds());
    assert_eq!(owner.dirty_count(), 0);
}

#[test]
fn test_mark_needs_build() {
    let mut owner = BuildOwner::new();
    let element_id = make_key();

    owner.mark_needs_build(element_id);

    assert!(owner.has_pending_rebuilds());
    assert_eq!(owner.dirty_count(), 1);
    assert!(owner.is_dirty(element_id));
}

#[test]
fn test_mark_needs_build_idempotent() {
    let mut owner = BuildOwner::new();
    let element_id = make_key();

    owner.mark_needs_build(element_id);
    owner.mark_needs_build(element_id);

    // Should only be counted once
    assert_eq!(owner.dirty_count(), 1);
}

#[test]
fn test_clear_dirty() {
    let mut owner = BuildOwner::new();
    let element_id = make_key();

    owner.mark_needs_build(element_id);
    owner.clear_dirty();

    assert!(!owner.has_pending_rebuilds());
    assert_eq!(owner.dirty_count(), 0);
}

#[test]
fn test_drain_dirty() {
    let mut owner = BuildOwner::new();
    let (id1, id2) = make_two_keys();

    owner.mark_needs_build(id1);
    owner.mark_needs_build(id2);

    let drained = owner.drain_dirty();

    assert_eq!(drained.len(), 2);
    assert!(!owner.has_pending_rebuilds());
}

#[test]
fn test_build_scope_no_cycle() {
    let mut owner = BuildOwner::new();
    let element_id = make_key();

    // Should succeed for first entry
    assert!(owner.enter_build_scope(element_id));
    assert!(owner.is_building(element_id));

    owner.exit_build_scope(element_id);
    assert!(!owner.is_building(element_id));
}

#[test]
fn test_build_scope_detects_cycle() {
    let mut owner = BuildOwner::new();
    let element_id = make_key();

    // Enter once
    assert!(owner.enter_build_scope(element_id));

    // Try to enter again - should detect cycle
    assert!(!owner.enter_build_scope(element_id));
}

#[test]
fn test_build_scope_nested() {
    let mut owner = BuildOwner::new();
    let (parent, child) = make_two_keys();

    // Enter parent
    assert!(owner.enter_build_scope(parent));

    // Enter child (different element) - should succeed
    assert!(owner.enter_build_scope(child));

    // Exit both
    owner.exit_build_scope(child);
    owner.exit_build_scope(parent);

    assert!(!owner.is_building(parent));
    assert!(!owner.is_building(child));
}

#[test]
fn test_build_owner_focused_element() {
    let owner = BuildOwner::new();

    // Initially no element is focused
    assert!(owner.focused_element().is_none());

    // Set a focused element
    let key = make_key();
    owner.set_focused_element(Some(key));
    assert_eq!(owner.focused_element(), Some(key));

    // Clear the focused element
    owner.set_focused_element(None);
    assert!(owner.focused_element().is_none());
}