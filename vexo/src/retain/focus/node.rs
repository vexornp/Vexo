use super::key::FocusNodeKey;
use crate::core::Bounds;
use crate::core::Logical;
use crate::retain::id::ElementKey;

pub struct FocusNodeData {
    pub parent: Option<FocusNodeKey>,
    pub children: Vec<FocusNodeKey>,
    pub on_focus_gained: Option<Box<dyn Fn()>>,
    pub on_focus_lost: Option<Box<dyn Fn()>>,
    pub can_request_focus: bool,
    pub skip_traversal: bool,
    pub keyboard_token: bool,
    pub element_key: Option<ElementKey>,
    pub layout_rect: Option<Bounds<Logical>>,
}

impl FocusNodeData {
    pub fn new() -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            on_focus_gained: None,
            on_focus_lost: None,
            can_request_focus: true,
            skip_traversal: false,
            keyboard_token: false,
            element_key: None,
            layout_rect: None,
        }
    }

    pub fn has_primary_focus(&self, primary: Option<FocusNodeKey>, own_key: FocusNodeKey) -> bool {
        primary == Some(own_key)
    }

    pub fn consume_keyboard_token(&mut self) -> bool {
        let token = self.keyboard_token;
        self.keyboard_token = false;
        token
    }
}

impl Default for FocusNodeData {
    fn default() -> Self {
        Self::new()
    }
}
