//! Component context and key path for auto-scoping.

use crate::core::{Scale, WidgetId};
use crate::component::ComponentStateStorage;
use glyphon::FontSystem;
use std::cell::Cell;

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

/// Context provided to components during `view()`.
pub struct ComponentContext<'a, M: Clone + std::fmt::Debug + Send> {
    key_path: KeyPath,
    state_storage: &'a mut ComponentStateStorage,
    font_system: &'a mut FontSystem,
    scale: Scale,
    auto_id_counter: Cell<u32>,
    _marker: std::marker::PhantomData<M>,
}

impl<'a, M: Clone + std::fmt::Debug + Send> ComponentContext<'a, M> {
    pub fn new(
        key_path: KeyPath,
        state_storage: &'a mut ComponentStateStorage,
        font_system: &'a mut FontSystem,
        scale: Scale,
    ) -> Self {
        Self {
            key_path,
            state_storage,
            font_system,
            scale,
            auto_id_counter: Cell::new(0),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn widget_id(&self, local_key: &str) -> WidgetId {
        WidgetId::from_key(&self.key_path.scoped(local_key))
    }

    pub fn auto_id(&self) -> WidgetId {
        let n = self.auto_id_counter.get();
        self.auto_id_counter.set(n + 1);
        self.widget_id(&format!("auto_{}", n))
    }

    pub fn child_context<N: Clone + std::fmt::Debug + Send>(
        &mut self,
        component_key: &str,
    ) -> ComponentContext<'_, N> {
        ComponentContext {
            key_path: self.key_path.child(component_key),
            state_storage: self.state_storage,
            font_system: self.font_system,
            scale: self.scale,
            auto_id_counter: Cell::new(0),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn key_path(&self) -> &KeyPath {
        &self.key_path
    }

    pub fn scale(&self) -> Scale {
        self.scale
    }

    pub fn font_system(&mut self) -> &mut FontSystem {
        self.font_system
    }

    pub fn state_storage(&mut self) -> &mut ComponentStateStorage {
        self.state_storage
    }

    /// Get or create component state from storage.
    pub fn get_or_create_state<S: Default + 'static>(&mut self, key: &str) -> &mut S {
        self.state_storage.get_or_create(key)
    }
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
