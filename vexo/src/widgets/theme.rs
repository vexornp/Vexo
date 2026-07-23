//! `Theme` — an `InheritedWidget` exposing `ThemeData` to descendants.
//!
//! Proves the ergonomic lookup pattern: descendants call `Theme::of(ctx)`
//! to read the nearest theme and auto-rebuild when it changes.
//!
//! See `docs/superpowers/specs/2026-07-12-inherited-widget-design.md`.

use crate::core::Color;
use crate::inherited_widget::{impl_widget_for_inherited, InheritedWidget};
use crate::key::WidgetKey;
use crate::stateful_widget::RenderContext;
use crate::widgets::Widget;

/// Whether a `ThemeData` is light or dark.
///
/// Mirrors Flutter's `Brightness` on `ThemeData`: lets tokens resolve
/// mode-specific values (e.g. pure-black chrome bars in dark) without the
/// app having to thread a separate `is_dark` signal alongside the theme.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Brightness {
    Light,
    Dark,
}

/// Immutable theme data exposed to descendants by `Theme`.
///
/// Core Material-ish color roles only. Additive: new fields don't break
/// dependents.
#[derive(Clone, PartialEq, Debug)]
pub struct ThemeData {
    pub primary: Color,
    pub on_primary: Color,
    pub background: Color,
    pub on_background: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub surface_variant: Color,
    pub outline: Color,
    pub on_surface_variant: Color,
    pub error: Color,
    pub on_error: Color,
    /// Backdrop for grouped (iOS-style) lists. Cards sit on top of this.
    pub grouped_background: Color,
    /// Overall brightness of this theme. Set by `light()` / `dark()`.
    pub brightness: Brightness,
}

impl ThemeData {
    /// Light preset. Used as the fallback when no `Theme` ancestor exists.
    pub fn light() -> Self {
        Self {
            primary: Color::from_hex(0x6775FFFF),
            on_primary: Color::WHITE,
            background: Color::WHITE,
            on_background: Color::BLACK,
            surface: Color::from_hex(0xFFFFFFFF),
            on_surface: Color::from_hex(0x1C1B1FFF),
            surface_variant: Color::from_hex(0xE6E6EBFF),
            outline: Color::from_hex(0xC7C7CCFF),
            on_surface_variant: Color::from_hex(0x999999FF),
            error: Color::from_hex(0xB3261EFF),
            on_error: Color::WHITE,
            grouped_background: Color::from_hex(0xF2F2F7FF),
            brightness: Brightness::Light,
        }
    }

    /// Dark preset.
    pub fn dark() -> Self {
        Self {
            primary: Color::from_hex(0x6775FFFF),
            on_primary: Color::WHITE,
            background: Color::from_hex(0x000000FF),
            on_background: Color::WHITE,
            surface: Color::from_hex(0x1C1C1EFF),
            on_surface: Color::WHITE,
            surface_variant: Color::from_hex(0x2C2C2EFF),
            outline: Color::from_hex(0x49454FFF),
            on_surface_variant: Color::from_hex(0x9E9CA6FF),
            error: Color::from_hex(0xF2B8B5FF),
            on_error: Color::BLACK,
            grouped_background: Color::from_hex(0x000000FF),
            brightness: Brightness::Dark,
        }
    }

    /// `true` when this theme is the dark preset (or any theme constructed
    /// with `brightness: Brightness::Dark`).
    pub fn is_dark(&self) -> bool {
        matches!(self.brightness, Brightness::Dark)
    }
}

impl Default for ThemeData {
    fn default() -> Self {
        Self::light()
    }
}

/// An `InheritedWidget` that exposes `ThemeData` to its subtree.
pub struct Theme {
    data: ThemeData,
    child: Box<dyn Widget>,
    key: Option<WidgetKey>,
}

impl Theme {
    /// Create a `Theme` that exposes `data` to `child`'s subtree.
    pub fn new(data: ThemeData, child: impl Widget + 'static) -> Self {
        Self {
            data,
            child: Box::new(child),
            key: None,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Read the nearest ancestor `Theme`. Establishes a dependency:
    /// caller rebuilds when the theme data changes.
    ///
    /// Falls back to `ThemeData::light()` when no `Theme` ancestor exists,
    /// so tests and small demos that don't wrap a `Theme` get sensible colors.
    pub fn of(ctx: &mut RenderContext) -> ThemeData {
        ctx.depend_on_inherited_widget::<ThemeData>()
            .unwrap_or_else(ThemeData::light)
    }
}

impl Clone for Theme {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            child: self.child.clone_boxed(),
            key: self.key.clone(),
        }
    }
}

impl InheritedWidget for Theme {
    type Value = ThemeData;

    fn value(&self) -> &ThemeData {
        &self.data
    }

    fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }
}

impl_widget_for_inherited!(Theme);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn theme_data_light_and_dark_differ() {
        assert_ne!(ThemeData::light(), ThemeData::dark());
    }

    #[test]
    fn theme_data_default_is_light() {
        assert_eq!(ThemeData::default(), ThemeData::light());
    }

    #[test]
    fn theme_data_has_new_roles() {
        let l = ThemeData::light();
        // New fields must be non-default (not pure black/white/transparent).
        assert_ne!(l.surface_variant, Color::TRANSPARENT);
        assert_ne!(l.outline, Color::TRANSPARENT);
        assert_ne!(l.on_surface_variant, Color::TRANSPARENT);
    }

    #[test]
    fn theme_data_light_and_dark_differ_on_new_roles() {
        let l = ThemeData::light();
        let d = ThemeData::dark();
        assert_ne!(l.surface_variant, d.surface_variant);
        assert_ne!(l.outline, d.outline);
        assert_ne!(l.on_surface_variant, d.on_surface_variant);
    }

    #[test]
    fn theme_data_dark_primary_is_brand_blue() {
        // dark().primary changed from the placeholder 0x121434 to the same
        // brand blue as light(), so accent stays consistent across modes.
        assert_eq!(ThemeData::dark().primary, ThemeData::light().primary);
        assert_eq!(ThemeData::dark().primary, Color::from_hex(0x6775FFFF));
    }

    #[test]
    fn theme_data_brightness_and_is_dark() {
        assert!(!ThemeData::light().is_dark());
        assert!(ThemeData::dark().is_dark());
        assert_ne!(ThemeData::light().brightness, ThemeData::dark().brightness);
    }

    #[test]
    fn theme_inherited_widget_value() {
        let t = Theme::new(ThemeData::dark(), Text::new("hi"));
        assert_eq!(t.value(), &ThemeData::dark());
    }

    #[test]
    fn theme_inherited_widget_child() {
        let t = Theme::new(ThemeData::dark(), Text::new("hi"));
        assert!(InheritedWidget::child(&t)
            .as_any()
            .downcast_ref::<Text>()
            .is_some());
    }

    #[test]
    fn theme_clone_preserves_data_and_child() {
        let t = Theme::new(ThemeData::dark(), Text::new("hi")).with_key("thm");
        let cloned = t.clone();
        assert_eq!(cloned.value(), t.value());
        assert!(InheritedWidget::child(&cloned)
            .as_any()
            .downcast_ref::<Text>()
            .is_some());
        assert_eq!(InheritedWidget::key(&cloned), InheritedWidget::key(&t));
    }

    #[test]
    fn theme_update_should_notify_default() {
        let t1 = Theme::new(ThemeData::light(), Text::new("a"));
        let t2_same = Theme::new(ThemeData::light(), Text::new("b"));
        let t3_diff = Theme::new(ThemeData::dark(), Text::new("c"));
        // Default impl compares value() — child changes don't notify.
        assert!(!t1.update_should_notify(&t1, &t2_same));
        assert!(t1.update_should_notify(&t1, &t3_diff));
    }
}
