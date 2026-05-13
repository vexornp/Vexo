use super::key::Key;
use super::id::{ElementId, RenderObjectKey};

#[test]
fn test_key_creation() {
    let key = Key::new("my-widget");
    assert_eq!(key.as_str(), "my-widget");
}

#[test]
fn test_key_equality() {
    let key1 = Key::new("widget-a");
    let key2 = Key::new("widget-a");
    let key3 = Key::new("widget-b");

    assert_eq!(key1, key2);
    assert_ne!(key1, key3);
}

#[test]
fn test_key_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();

    set.insert(Key::new("key1"));
    set.insert(Key::new("key1")); // Duplicate

    assert_eq!(set.len(), 1);
}

#[test]
fn test_key_from_string() {
    let key: Key = "my-key".into();
    assert_eq!(key.as_str(), "my-key");
}

// === ElementId tests ===

#[test]
fn test_element_id_uniqueness() {
    let id1 = ElementId::new();
    let id2 = ElementId::new();

    assert_ne!(id1, id2);
}

#[test]
fn test_render_object_key_uniqueness() {
    let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
    let id1 = sm.insert(());
    let id2 = sm.insert(());

    assert_ne!(id1, id2);
}

#[test]
fn test_element_id_in_hashmap() {
    use std::collections::HashMap;
    let mut map = HashMap::new();

    let id = ElementId::new();
    map.insert(id, "test");

    assert_eq!(map.get(&id), Some(&"test"));
}