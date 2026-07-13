//! Integration tests for `InheritedWidget` via the `ThreeTreePipeline`.
//!
//! Drives `pipeline.reconcile()` with widget trees that mount a `Theme`
//! provider and a descendant `Component` that reads it via `Theme::of(ctx)`.
//! Verifies the four core behaviors:
//!   1. A provider's value reaches a descendant.
//!   2. Updating the provider rebuilds dependents with the new value.
//!   3. `Theme::of` falls back to `light()` when no provider is present.
//!   4. With nested providers, the nearest ancestor wins.
//!
//! Each test carries its own `Arc<Mutex<Option<ThemeData>>>` capture slot
//! through the widget tree — no global state, so tests are independent and
//! can run in parallel.

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::animation::AnimationTicker;
    use crate::stateful_widget::RenderContext;
    use crate::widgets::{Text, Theme, ThemeData};
    use crate::{Component, ComponentState, ThreeTreePipeline, Widget};

    /// Per-test capture slot. The reader `Component` writes the `ThemeData`
    /// it observed via `Theme::of(ctx)` here during `render()`.
    type Slot = Arc<Mutex<Option<ThemeData>>>;

    fn new_slot() -> Slot {
        Arc::new(Mutex::new(None))
    }

    // ========================================================================
    // Reader components
    // ========================================================================

    /// Reads `Theme::of(ctx)` and records the value into the slot.
    #[derive(Clone)]
    struct ThemeReader {
        slot: Slot,
    }

    #[derive(Default)]
    struct ThemeReaderState;

    impl ComponentState for ThemeReaderState {}

    impl Component for ThemeReader {
        type State = ThemeReaderState;

        fn render(
            &self,
            _state: &mut ThemeReaderState,
            ctx: &mut RenderContext,
        ) -> Box<dyn Widget> {
            let data = Theme::of(ctx);
            *self.slot.lock().unwrap() = Some(data);
            Box::new(Text::new("reader"))
        }
    }

    // ========================================================================
    // 1. Provider value reaches the descendant
    // ========================================================================

    #[test]
    fn theme_provider_value_reaches_descendant() {
        let slot = new_slot();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Theme(dark) → ThemeReader
        let tree = Theme::new(ThemeData::dark(), ThemeReader { slot: slot.clone() });
        pipeline.reconcile(Box::new(tree));

        // The reader's render() ran during mount and read the dark theme.
        assert!(
            !pipeline.element_registry().is_empty(),
            "tree should have mounted elements"
        );
        assert_eq!(
            slot.lock().unwrap().clone(),
            Some(ThemeData::dark()),
            "ThemeReader should have read the dark theme from its ancestor Theme"
        );
    }

    // ========================================================================
    // 2. Updating the provider rebuilds dependents with the new value
    // ========================================================================

    /// Parent `Component` that holds the `ThemeData` and wraps the reader in
    /// a `Theme`. The same `slot` is forwarded to the reader on every rebuild
    /// so the test can observe what the most recent render saw.
    #[derive(Clone)]
    struct ThemeApp {
        data: ThemeData,
        slot: Slot,
    }

    #[derive(Default)]
    struct ThemeAppState;

    impl ComponentState for ThemeAppState {}

    impl Component for ThemeApp {
        type State = ThemeAppState;

        fn render(&self, _state: &mut ThemeAppState, _ctx: &mut RenderContext) -> Box<dyn Widget> {
            Box::new(Theme::new(
                self.data.clone(),
                ThemeReader {
                    slot: self.slot.clone(),
                },
            ))
        }
    }

    #[test]
    fn theme_update_rebuilds_dependents() {
        let slot = new_slot();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Initial: light theme.
        pipeline.reconcile(Box::new(ThemeApp {
            data: ThemeData::light(),
            slot: slot.clone(),
        }));

        let first_seen = slot.lock().unwrap().clone();
        assert_eq!(
            first_seen,
            Some(ThemeData::light()),
            "reader should have read the light theme on first render"
        );

        // Update: dark theme. The InheritedElement's `update` notices the
        // value changed, marks dependents dirty, and the dependent rebuilds
        // with the new value on the next `perform_rebuilds()`.
        pipeline.reconcile(Box::new(ThemeApp {
            data: ThemeData::dark(),
            slot: slot.clone(),
        }));
        // Flush any pending rebuilds triggered by the notify.
        pipeline.perform_rebuilds();

        let second_seen = slot.lock().unwrap().clone();
        assert_eq!(
            second_seen,
            Some(ThemeData::dark()),
            "reader should have rebuilt with the dark theme after the provider update"
        );
        assert_ne!(
            first_seen, second_seen,
            "the two reads must differ (sanity check)"
        );
    }

    // ========================================================================
    // 3. Theme::of returns the light() fallback without a provider
    // ========================================================================

    #[test]
    fn theme_of_returns_light_fallback_without_provider() {
        let slot = new_slot();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // No Theme ancestor — just the reader.
        pipeline.reconcile(Box::new(ThemeReader { slot: slot.clone() }));

        assert_eq!(
            slot.lock().unwrap().clone(),
            Some(ThemeData::light()),
            "Theme::of should fall back to ThemeData::light() when no Theme ancestor exists"
        );
    }

    // ========================================================================
    // 4. Nested themes: nearest ancestor wins
    // ========================================================================

    #[test]
    fn nested_themes_nearest_wins() {
        let slot = new_slot();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Theme(dark) → Theme(light) → ThemeReader
        let tree = Theme::new(
            ThemeData::dark(),
            Theme::new(ThemeData::light(), ThemeReader { slot: slot.clone() }),
        );
        pipeline.reconcile(Box::new(tree));

        assert_eq!(
            slot.lock().unwrap().clone(),
            Some(ThemeData::light()),
            "nearest (inner) Theme should win — ThemeReader should see light, not dark"
        );
    }
}
