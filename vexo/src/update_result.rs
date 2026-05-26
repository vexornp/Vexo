//! Result type for render object updates.
//!
//! This module provides `UpdateResult`, a bitflags type that describes what
//! changed during a render object update. This enables partial dirty marking
//! for better performance.

bitflags::bitflags! {
    /// Describes what changed during a render object update.
    ///
    /// Returned by `Widget::update_render_object()` to enable partial dirty marking.
    /// This allows the framework to avoid unnecessary layout or paint operations
    /// when properties haven't actually changed.
    ///
    /// # Examples
    ///
    /// ```
    /// use vexo::UpdateResult;
    ///
    /// // No changes
    /// let result = UpdateResult::NONE;
    /// assert!(!result.needs_layout());
    /// assert!(!result.needs_paint());
    ///
    /// // Layout-affecting change
    /// let result = UpdateResult::LAYOUT;
    /// assert!(result.needs_layout());
    /// assert!(!result.needs_paint());
    ///
    /// // Both layout and paint changed
    /// let result = UpdateResult::LAYOUT | UpdateResult::PAINT;
    /// assert!(result.needs_layout());
    /// assert!(result.needs_paint());
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct UpdateResult: u8 {
        /// No properties changed.
        const NONE = 0b000;

        /// A layout-affecting property changed (e.g., size, constraints, text content).
        /// This triggers both layout and paint.
        const LAYOUT = 0b001;

        /// A visual-only property changed (e.g., color, opacity).
        /// This triggers paint only, not layout.
        const PAINT = 0b010;

        /// Convenience: both layout and paint changed.
        const ALL = Self::LAYOUT.bits() | Self::PAINT.bits();
    }
}

impl UpdateResult {
    /// Returns true if layout is needed.
    pub fn needs_layout(self) -> bool {
        self.contains(Self::LAYOUT)
    }

    /// Returns true if paint is needed.
    pub fn needs_paint(self) -> bool {
        self.contains(Self::PAINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none() {
        let result = UpdateResult::NONE;
        assert!(!result.needs_layout());
        assert!(!result.needs_paint());
        assert!(result.is_empty());
    }

    #[test]
    fn test_layout() {
        let result = UpdateResult::LAYOUT;
        assert!(result.needs_layout());
        assert!(!result.needs_paint());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_paint() {
        let result = UpdateResult::PAINT;
        assert!(!result.needs_layout());
        assert!(result.needs_paint());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_all() {
        let result = UpdateResult::ALL;
        assert!(result.needs_layout());
        assert!(result.needs_paint());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_combined() {
        let result = UpdateResult::LAYOUT | UpdateResult::PAINT;
        assert!(result.needs_layout());
        assert!(result.needs_paint());
        assert_eq!(result, UpdateResult::ALL);
    }

    #[test]
    fn test_default() {
        let result: UpdateResult = Default::default();
        assert_eq!(result, UpdateResult::NONE);
    }
}
