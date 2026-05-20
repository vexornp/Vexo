use super::key::FocusNodeKey;

#[derive(Clone, Debug, PartialEq)]
pub enum TraversalPolicy {
    WidgetOrder,
    ReadingOrder,
}

impl TraversalPolicy {
    pub fn find_first(&self, scope: FocusNodeKey, manager: &super::manager::FocusManager) -> Option<FocusNodeKey> {
        match self {
            TraversalPolicy::WidgetOrder => {
                let children = manager.children(scope);
                for &child in &children {
                    if manager.can_request_focus(child) && !manager.skip_traversal(child) {
                        if manager.is_scope(child) {
                            if let Some(first) = self.find_first(child, manager) {
                                return Some(first);
                            }
                        } else {
                            return Some(child);
                        }
                    }
                }
                None
            }
            TraversalPolicy::ReadingOrder => None,
        }
    }

    pub fn find_last(&self, scope: FocusNodeKey, manager: &super::manager::FocusManager) -> Option<FocusNodeKey> {
        match self {
            TraversalPolicy::WidgetOrder => {
                let children = manager.children(scope);
                for &child in children.iter().rev() {
                    if manager.can_request_focus(child) && !manager.skip_traversal(child) {
                        if manager.is_scope(child) {
                            if let Some(last) = self.find_last(child, manager) {
                                return Some(last);
                            }
                        } else {
                            return Some(child);
                        }
                    }
                }
                None
            }
            TraversalPolicy::ReadingOrder => None,
        }
    }

    pub fn next(&self, current: FocusNodeKey, scope: FocusNodeKey, manager: &super::manager::FocusManager) -> Option<FocusNodeKey> {
        match self {
            TraversalPolicy::WidgetOrder => {
                let children = manager.children(scope);
                let current_idx = children.iter().position(|&c| c == current)?;
                let len = children.len();
                for i in 1..=len {
                    let idx = (current_idx + i) % len;
                    let child = children[idx];
                    if manager.can_request_focus(child) && !manager.skip_traversal(child) {
                        if manager.is_scope(child) {
                            if let Some(first) = self.find_first(child, manager) {
                                return Some(first);
                            }
                        } else {
                            return Some(child);
                        }
                    }
                }
                None
            }
            TraversalPolicy::ReadingOrder => None,
        }
    }

    pub fn previous(&self, current: FocusNodeKey, scope: FocusNodeKey, manager: &super::manager::FocusManager) -> Option<FocusNodeKey> {
        match self {
            TraversalPolicy::WidgetOrder => {
                let children = manager.children(scope);
                let current_idx = children.iter().position(|&c| c == current)?;
                let len = children.len();
                for i in 1..=len {
                    let idx = (current_idx + len - i) % len;
                    let child = children[idx];
                    if manager.can_request_focus(child) && !manager.skip_traversal(child) {
                        if manager.is_scope(child) {
                            if let Some(last) = self.find_last(child, manager) {
                                return Some(last);
                            }
                        } else {
                            return Some(child);
                        }
                    }
                }
                None
            }
            TraversalPolicy::ReadingOrder => None,
        }
    }
}
