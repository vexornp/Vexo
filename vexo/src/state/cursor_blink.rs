use std::time::Instant;

/// Tracks cursor blink timing for focused text inputs.
pub struct CursorBlinkState {
    /// Time of last tick (frame start)
    last_update: Instant,
    /// Accumulated milliseconds since last blink toggle
    accumulator_ms: f32,
    /// Whether cursor is currently visible (blink phase)
    visible: bool,
    /// Blink period in milliseconds (800ms default)
    blink_period_ms: f32,
}

impl Default for CursorBlinkState {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorBlinkState {
    pub fn new() -> Self {
        Self {
            last_update: Instant::now(),
            accumulator_ms: 0.0,
            visible: true,
            blink_period_ms: 800.0,
        }
    }

    /// Call each frame to update blink state based on elapsed time.
    pub fn tick(&mut self) {
        let now = Instant::now();
        let elapsed_ms = (now - self.last_update).as_millis() as f32;
        self.last_update = now;
        self.accumulator_ms += elapsed_ms;

        // Toggle visibility each time we exceed the period
        while self.accumulator_ms >= self.blink_period_ms {
            self.accumulator_ms -= self.blink_period_ms;
            self.visible = !self.visible;
        }
    }

    /// Reset blink to visible state (call on keyboard input).
    pub fn reset(&mut self) {
        self.accumulator_ms = 0.0;
        self.visible = true;
        self.last_update = Instant::now();
    }

    /// Is cursor currently visible?
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}
