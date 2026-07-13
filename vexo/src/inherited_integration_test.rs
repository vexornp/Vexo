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
    use crate::core::Color;
    use crate::stateful_widget::RenderContext;
    use crate::widgets::{Flex, Text, Theme, ThemeData};
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

    // ========================================================================
    // 5. Regression: provider propagates through an intermediate container
    // ========================================================================
    //
    // This is the test that would have caught the InheritedMap propagation
    // bug: only `InheritedElement::mount` wrote to `inherited_map_storage`,
    // so a non-provider parent (like `Flex`) left no map for its children.
    // The reader then got an empty map and `Theme::of` fell back to light().
    //
    // After the fix, every element stores its computed map at mount, so the
    // reader sees the dark theme through the intermediate `Flex`.

    #[test]
    fn theme_propagates_through_intermediate_container() {
        let slot = new_slot();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Theme(dark) → Flex → ThemeReader
        let tree = Theme::new(
            ThemeData::dark(),
            Flex::new().push(ThemeReader { slot: slot.clone() }),
        );
        pipeline.reconcile(Box::new(tree));

        assert_eq!(
            slot.lock().unwrap().clone(),
            Some(ThemeData::dark()),
            "ThemeReader nested under a non-provider Flex should still see the dark Theme \
             from its grandparent — InheritedMap must propagate through intermediate containers"
        );
    }

    // ========================================================================
    // 6. Nested-provider update disjointness: outer update does not change
    //    the value the reader observes (inner provider still wins)
    // ========================================================================

    /// App that holds the OUTER theme data and wraps the reader in
    /// `Theme(outer) → Flex → Theme(light, inner) → ThemeReader`. The inner
    /// theme is constant so the reader always depends on `light()`.
    #[derive(Clone)]
    struct OuterThemeApp {
        outer: ThemeData,
        slot: Slot,
    }

    #[derive(Default)]
    struct OuterThemeAppState;

    impl ComponentState for OuterThemeAppState {}

    impl Component for OuterThemeApp {
        type State = OuterThemeAppState;

        fn render(
            &self,
            _state: &mut OuterThemeAppState,
            _ctx: &mut RenderContext,
        ) -> Box<dyn Widget> {
            Box::new(Theme::new(
                self.outer.clone(),
                Flex::new().push(Theme::new(
                    ThemeData::light(),
                    ThemeReader {
                        slot: self.slot.clone(),
                    },
                )),
            ))
        }
    }

    #[test]
    fn nested_provider_update_does_not_override_inner() {
        let slot = new_slot();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Initial: outer = dark, inner = light. Reader should see light.
        pipeline.reconcile(Box::new(OuterThemeApp {
            outer: ThemeData::dark(),
            slot: slot.clone(),
        }));

        assert_eq!(
            slot.lock().unwrap().clone(),
            Some(ThemeData::light()),
            "reader under the inner (light) Theme should see light, not the outer dark"
        );

        // Update the OUTER theme to a different dark variant. The outer
        // InheritedElement notifies its dependents — but the reader is a
        // dependent of the INNER provider, not the outer. The reader should
        // still observe the inner light theme.
        //
        // We craft a distinct ThemeData (still "dark-ish") so the outer
        // provider's `update_should_notify` returns true.
        let mut other_dark = ThemeData::dark();
        other_dark.primary = Color::from_hex(0x000000FF);
        pipeline.reconcile(Box::new(OuterThemeApp {
            outer: other_dark,
            slot: slot.clone(),
        }));
        pipeline.perform_rebuilds();

        assert_eq!(
            slot.lock().unwrap().clone(),
            Some(ThemeData::light()),
            "reader should still see the inner light theme after the outer theme changes — \
             nested-provider update disjointness"
        );
    }

    // ========================================================================
    // 7. Provider unmount cleanup: removing the Theme provider makes the
    //    reader fall back to light()
    // ========================================================================

    #[test]
    fn provider_unmount_falls_back_to_light() {
        let slot = new_slot();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Mount: Theme(dark) → ThemeReader. Reader sees dark.
        pipeline.reconcile(Box::new(Theme::new(
            ThemeData::dark(),
            ThemeReader { slot: slot.clone() },
        )));
        assert_eq!(
            slot.lock().unwrap().clone(),
            Some(ThemeData::dark()),
            "reader should see dark while the Theme provider is mounted"
        );

        // Reconcile with a bare ThemeReader (no Theme wrapper). The old root
        // (Theme) cannot update to a ThemeReader, so the whole tree is
        // unmounted and a fresh ThemeReader is mounted. With the provider
        // gone, `Theme::of(ctx)` must fall back to light().
        pipeline.reconcile(Box::new(ThemeReader { slot: slot.clone() }));

        assert_eq!(
            slot.lock().unwrap().clone(),
            Some(ThemeData::light()),
            "after the Theme provider unmounts, reader should fall back to light() \
             (provider entry must be cleaned up from both the registry and inherited_map_storage)"
        );
    }
}
