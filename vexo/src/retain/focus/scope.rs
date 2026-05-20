use super::key::FocusNodeKey;
use super::traversal::TraversalPolicy;

pub enum UnfocusDisposition {
    RestorePrevious,
    Clear,
}

pub struct FocusScopeData {
    pub focused_child: Option<FocusNodeKey>,
    pub focused_child_history: Vec<FocusNodeKey>,
    pub traversal_policy: TraversalPolicy,
}

impl FocusScopeData {
    pub fn new() -> Self {
        Self {
            focused_child: None,
            focused_child_history: Vec::new(),
            traversal_policy: TraversalPolicy::WidgetOrder,
        }
    }

    pub fn push_focused_child(&mut self, child: FocusNodeKey) {
        if self.focused_child != Some(child) {
            if let Some(old) = self.focused_child.take() {
                self.focused_child_history.push(old);
            }
            self.focused_child = Some(child);
        }
    }

    pub fn pop_focused_child(&mut self) -> Option<FocusNodeKey> {
        self.focused_child.take().or_else(|| self.focused_child_history.pop())
    }
}

impl Default for FocusScopeData {
    fn default() -> Self {
        Self::new()
    }
}
