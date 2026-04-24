//! Component context and key path for auto-scoping.

/// Hierarchical key path for automatic WidgetId scoping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPath {
    segments: Vec<String>,
}

impl KeyPath {
    pub fn root() -> Self {
        Self { segments: Vec::new() }
    }

    pub fn child(&self, segment: &str) -> Self {
        let mut segments = self.segments.clone();
        segments.push(segment.to_string());
        Self { segments }
    }

    pub fn scoped(&self, local_key: &str) -> String {
        let mut result = self.segments.join("/");
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str(local_key);
        result
    }
}

impl Default for KeyPath {
    fn default() -> Self {
        Self::root()
    }
}

// ComponentContext will be implemented in a future task
// This is a placeholder to satisfy the mod.rs export
/// Context provided to component view functions.
pub struct ComponentContext<'a, M> {
    _marker: std::marker::PhantomData<&'a M>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypath_root() {
        let root = KeyPath::root();
        assert_eq!(root.scoped("widget"), "widget");
    }

    #[test]
    fn test_keypath_child() {
        let root = KeyPath::root();
        let child = root.child("login");
        assert_eq!(child.scoped("username"), "login/username");
    }

    #[test]
    fn test_keypath_nested() {
        let root = KeyPath::root();
        let app = root.child("app");
        let login = app.child("login");
        assert_eq!(login.scoped("username"), "app/login/username");
    }

    #[test]
    fn test_keypath_multiple_widgets() {
        let form = KeyPath::root().child("form");
        let id1 = form.scoped("field1");
        let id2 = form.scoped("field2");
        assert_ne!(id1, id2);
        assert_eq!(id1, "form/field1");
        assert_eq!(id2, "form/field2");
    }
}
