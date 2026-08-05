//! ScrollView widget - a scrollable container for content that overflows.

use std::any::Any;

use crate::animation::{SpringDescription, Tolerance};
use crate::element::Element;
use crate::elements::RenderObjectElement;
use crate::elements::ScrollViewElement;
use crate::key::WidgetKey;
use crate::render_object::RenderObject;
use crate::render_objects::ScrollViewRenderObject;
use crate::widgets::scroll_controller::ScrollController;
use crate::widgets::Widget;
use crate::UpdateResult;

/// Physics configuration for a `ScrollView`. Fixes ROADMAP §9
/// "no ScrollPhysics abstraction" — physics was previously hardcoded
/// inline in `ScrollViewElement` (`STIFFNESS=340`, `TAU=0.325`, etc.).
#[derive(Debug, Clone, Copy)]
pub struct ScrollPhysics {
    /// Spring for bounce-back / overscroll return.
    pub spring: SpringDescription,
    /// Drag time-constant `τ` for `FrictionSimulation` (fling decay).
    pub friction: f64,
    /// Minimum fling velocity (px/s) — below this, a pointer-up does not fling.
    pub fling_min_velocity: f32,
    /// Px-scale settle tolerance for scroll sims.
    pub settle: Tolerance,
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self {
            spring: SpringDescription::ios(340.0, 1.0), // today's STIFFNESS/DAMPING_RATIO
            friction: 0.325,                            // today's TAU
            fling_min_velocity: 50.0,                   // today's V_MIN_FLING (fling start gate)
            settle: Tolerance::SCROLL,                  // today's X_SETTLE/V_SETTLE/MAX_DURATION
        }
    }
}

pub struct ScrollView {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    controller: Option<ScrollController>,
    physics: ScrollPhysics,
}

impl ScrollView {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            controller: None,
            physics: ScrollPhysics::default(),
        }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }

    pub fn controller_ref(&self) -> Option<&ScrollController> {
        self.controller.as_ref()
    }

    pub fn physics(mut self, physics: ScrollPhysics) -> Self {
        self.physics = physics;
        self
    }

    pub fn physics_ref(&self) -> ScrollPhysics {
        self.physics
    }
}

impl Clone for ScrollView {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            controller: self.controller.clone(),
            physics: self.physics,
        }
    }
}

impl Widget for ScrollView {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ScrollViewElement::new();
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ScrollViewRenderObject::new())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        other.as_any().downcast_ref::<ScrollView>().is_some()
    }

    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        // Always request paint because the scroll offset may have changed
        // via Cell::set in apply_scroll_offset, which bypasses the normal
        // dirty-tracking path. Returning PAINT ensures that when
        // mark_needs_build triggers a rebuild, the render object is
        // marked as needing paint so the painter re-traverses it.
        UpdateResult::PAINT
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}
