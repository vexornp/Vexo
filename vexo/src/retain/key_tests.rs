use super::key::Key;

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