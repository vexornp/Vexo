use std::time::{Duration, Instant};

/// Tracks cursor blink timing for focused text inputs.
///
/// Follows Flutter's model: blink is driven by wall-clock time, not by
/// frame ticks. Call `check_and_toggle()` periodically (e.g., from the
/// event loop's `about_to_wait` callback). It returns `true` only when
/// visibility actually toggled, so the caller knows to request a repaint.
pub struct CursorBlinkState {
    /// Time when `visible` last toggled (or was reset).
    last_toggle: Instant,
    /// Whether cursor is currently visible (blink phase).
    visible: bool,
    /// Blink half-period — time between on/off toggles.
    blink_period: Duration,
}

impl Default for CursorBlinkState {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorBlinkState {
    pub fn new() -> Self {
        Self {
            last_toggle: Instant::now(),
            visible: true,
            blink_period: Duration::from_millis(500),
        }
    }

    /// Check if enough wall-clock time has elapsed for a toggle.
    /// Returns `true` if visibility changed (caller should request a repaint).
    ///
    /// This is the Flutter-style approach: `Timer.periodic` fires every
    /// 500ms and toggles the cursor. We don't tick per-frame; instead,
    /// we check elapsed time on each event-loop iteration and only act
    /// when the period has actually elapsed.
    pub fn check_and_toggle(&mut self) -> bool {
        let elapsed = Instant::now() - self.last_toggle;
        if elapsed >= self.blink_period {
            self.last_toggle = Instant::now();
            self.visible = !self.visible;
            true
        } else {
            false
        }
    }

    /// Reset blink to visible state (call on keyboard input or focus gain).
    /// Returns `true` if visibility changed (caller should request a repaint).
    pub fn reset(&mut self) -> bool {
        let changed = !self.visible;
        self.visible = true;
        self.last_toggle = Instant::now();
        changed
    }

    /// Is cursor currently visible?
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}
