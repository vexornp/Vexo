use std::cell::RefCell;
use std::rc::Rc;

use vexo::{
    AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox, FlexDirection, Layout,
    RenderContext, Signal, Style, Text, Theme, Widget, WithLayout,
};

use crate::platform::Platform;
use crate::theme::tokens;
use crate::theme::tokens::button::ButtonColors;

/// Visual style variant for a Button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Filled background, white text.
    Primary,
    /// Outlined border, no fill, blue text.
    Secondary,
    /// Red filled background, white text.
    Destructive,
    /// No border, no fill, blue text.
    Ghost,
}

impl Default for ButtonVariant {
    fn default() -> Self {
        ButtonVariant::Primary
    }
}

/// State for the Button component.
///
/// Tracks hover and press state via reactive Signals.
/// Auto-wired by `#[derive(ComponentState)]`.
#[derive(ComponentState, Default)]
pub struct ButtonState {
    pub is_pressed: Signal<bool>,
    pub is_hovered: Signal<bool>,
}

/// A platform-adaptive button component.
///
/// # Example
///
/// ```ignore
/// Button::new("Submit")
///     .variant(ButtonVariant::Primary)
///     .on_tap(|| submit())
///     .boxed()
/// ```
#[derive(Clone)]
pub struct Button {
    label: String,
    on_tap: Rc<RefCell<dyn FnMut()>>,
    variant: ButtonVariant,
    disabled: bool,
    platform: Option<Platform>,
    shadows: Vec<BoxShadow>,
}

impl Button {
    /// Create a new button with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_tap: Rc::new(RefCell::new(|| {})),
            variant: ButtonVariant::Primary,
            disabled: false,
            platform: None,
            shadows: Vec::new(),
        }
    }

    /// Set the visual variant.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set the tap action callback. Fires when the tap is recognized
    /// (pointer up, having won the gesture arena) — does NOT fire if a
    /// drag wins instead.
    pub fn on_tap(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_tap = Rc::new(RefCell::new(callback));
        self
    }

    /// Set whether the button is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Override the platform for this button.
    ///
    /// If not set, uses `Platform::current()`.
    pub fn platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Append a single box shadow to the button's decoration.
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadows.push(shadow);
        self
    }

    /// Replace the button's box shadows with the given list.
    pub fn shadows(mut self, shadows: Vec<BoxShadow>) -> Self {
        self.shadows = shadows;
        self
    }

    /// Get the variant.
    pub fn get_variant(&self) -> &ButtonVariant {
        &self.variant
    }

    /// Get whether the button is disabled.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Trigger the press callback programmatically. No-op if disabled.
    ///
    /// Primarily useful for testing.
    pub fn press(&self) {
        if !self.disabled {
            (self.on_tap.borrow_mut())();
        }
    }

    fn effective_platform(&self) -> Platform {
        self.platform.unwrap_or_else(Platform::current)
    }

    fn resolve_bg(&self, c: &ButtonColors, is_pressed: bool, is_hovered: bool) -> Color {
        match self.variant {
            ButtonVariant::Primary => {
                if is_pressed {
                    c.primary_bg_pressed
                } else if is_hovered && self.effective_platform() == Platform::Desktop {
                    c.primary_bg_hover
                } else {
                    c.primary_bg
                }
            }
            ButtonVariant::Secondary => c.secondary_bg,
            ButtonVariant::Destructive => {
                if is_pressed {
                    c.destructive_bg_pressed
                } else if is_hovered && self.effective_platform() == Platform::Desktop {
                    c.destructive_bg_hover
                } else {
                    c.destructive_bg
                }
            }
            ButtonVariant::Ghost => c.ghost_bg,
        }
    }

    fn resolve_border(&self, c: &ButtonColors) -> (Color, f32) {
        match self.variant {
            ButtonVariant::Secondary => (c.secondary_border, 1.0),
            _ => (Color::TRANSPARENT, 0.0),
        }
    }

    fn resolve_text_color(&self, c: &ButtonColors, is_hovered: bool) -> Color {
        match self.variant {
            ButtonVariant::Primary => c.primary_text,
            ButtonVariant::Destructive => c.destructive_text,
            ButtonVariant::Secondary => c.secondary_text,
            ButtonVariant::Ghost => {
                if is_hovered && self.effective_platform() == Platform::Desktop {
                    c.ghost_text_hover
                } else {
                    c.ghost_text
                }
            }
        }
    }

    fn resolve_corner_radius(&self) -> f32 {
        match self.effective_platform() {
            Platform::Desktop => tokens::button::CORNER_RADIUS_DESKTOP,
            Platform::Mobile => tokens::button::CORNER_RADIUS_MOBILE,
        }
    }

    fn resolve_font_size(&self) -> f32 {
        match self.effective_platform() {
            Platform::Desktop => tokens::button::FONT_SIZE_DESKTOP,
            Platform::Mobile => tokens::button::FONT_SIZE_MOBILE,
        }
    }

    /// Returns (top, right, bottom, left) for padding_each (TRBL order).
    fn resolve_padding(&self) -> (f32, f32, f32, f32) {
        match self.effective_platform() {
            Platform::Desktop => (
                tokens::button::PADDING_V_DESKTOP,
                tokens::button::PADDING_H_DESKTOP,
                tokens::button::PADDING_V_DESKTOP,
                tokens::button::PADDING_H_DESKTOP,
            ),
            Platform::Mobile => (
                tokens::button::PADDING_V_MOBILE,
                tokens::button::PADDING_H_MOBILE,
                tokens::button::PADDING_V_MOBILE,
                tokens::button::PADDING_H_MOBILE,
            ),
        }
    }
}

impl Component for Button {
    type State = ButtonState;

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_pressed = state.is_pressed.get();
        let is_hovered = state.is_hovered.get();

        let colors = tokens::button::colors(&Theme::of(ctx));
        let bg = self.resolve_bg(&colors, is_pressed, is_hovered);
        let (border_color, border_width) = self.resolve_border(&colors);
        let text_color = self.resolve_text_color(&colors, is_hovered);
        let corner_radius = self.resolve_corner_radius();
        let (pt, pr, pb, pl) = self.resolve_padding();
        let opacity = if self.disabled {
            tokens::button::DISABLED_OPACITY
        } else {
            1.0
        };

        let disabled = self.disabled;
        let on_tap_cb = self.on_tap.clone();
        let is_pressed_signal = state.is_pressed.clone();
        let is_pressed_signal_release = state.is_pressed.clone();
        let is_pressed_signal_exit = state.is_pressed.clone();
        let is_hovered_signal = state.is_hovered.clone();
        let is_hovered_signal_exit = state.is_hovered.clone();

        // Plain leaf — no modifiers on Text itself.
        // Font size is platform-adaptive to match SwiftUI .bordered defaults
        // (macOS 13pt body, iOS 17pt body).
        let text = Text::new(&self.label)
            .with_font_size(self.resolve_font_size())
            .with_color(text_color);

        // All decoration on the DecoratedBox. The WithLayout inside sets
        // padding + flex_shrink(0.0) so the container sizes to its content
        // (text intrinsic width + padding).
        // Note: Layout::padding_each takes (left, right, top, bottom) argument order.
        let inner = WithLayout::new(
            text,
            Layout::default()
                .flex_direction(FlexDirection::Row)
                .padding_each(pl, pr, pt, pb)
                .flex_shrink(0.0),
        );
        let mut style = Style::default().background(bg).corner_radius(corner_radius);

        if border_width > 0.0 {
            style = style.border(border_color, border_width);
        }

        if !self.shadows.is_empty() {
            style = style.shadows(self.shadows.clone());
        }

        let container = DecoratedBox::with_style(inner, style);

        WithLayout::new(
            container
                .boxed()
                .on_press(move || {
                    if !disabled {
                        is_pressed_signal.set(true);
                    }
                })
                .on_tap(move || {
                    if !disabled {
                        (on_tap_cb.borrow_mut())();
                    }
                })
                .on_release(move || {
                    is_pressed_signal_release.set(false);
                })
                .on_enter(move || {
                    if !disabled {
                        is_hovered_signal.set(true);
                    }
                })
                .on_exit(move || {
                    is_hovered_signal_exit.set(false);
                    is_pressed_signal_exit.set(false);
                })
                .opacity(opacity),
            Layout::default().align_self(AlignSelf::Start),
        )
        .boxed()
    }
}
