use std::cell::RefCell;
use std::rc::Rc;

use vexo::{
    AlignSelf, Color, Component, ComponentState, DecoratedContainer, RenderContext, Signal, Text,
    Widget,
};

use crate::platform::Platform;
use crate::theme::tokens;

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
///     .on_press(|| submit())
///     .boxed()
/// ```
#[derive(Clone)]
pub struct Button {
    label: String,
    on_press: Rc<RefCell<dyn FnMut()>>,
    variant: ButtonVariant,
    disabled: bool,
    platform: Option<Platform>,
}

impl Button {
    /// Create a new button with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_press: Rc::new(RefCell::new(|| {})),
            variant: ButtonVariant::Primary,
            disabled: false,
            platform: None,
        }
    }

    /// Set the visual variant.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set the press callback.
    pub fn on_press(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_press = Rc::new(RefCell::new(callback));
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
            (self.on_press.borrow_mut())();
        }
    }

    fn effective_platform(&self) -> Platform {
        self.platform.unwrap_or_else(Platform::current)
    }

    fn resolve_bg(&self, is_pressed: bool, is_hovered: bool) -> Color {
        match self.variant {
            ButtonVariant::Primary => {
                if is_pressed {
                    tokens::button::PRIMARY_BG_PRESSED
                } else if is_hovered && self.effective_platform() == Platform::Desktop {
                    tokens::button::PRIMARY_BG_HOVER
                } else {
                    tokens::button::PRIMARY_BG
                }
            }
            ButtonVariant::Secondary => tokens::button::SECONDARY_BG,
            ButtonVariant::Destructive => {
                if is_pressed {
                    tokens::button::DESTRUCTIVE_BG_PRESSED
                } else if is_hovered && self.effective_platform() == Platform::Desktop {
                    tokens::button::DESTRUCTIVE_BG_HOVER
                } else {
                    tokens::button::DESTRUCTIVE_BG
                }
            }
            ButtonVariant::Ghost => tokens::button::GHOST_BG,
        }
    }

    fn resolve_border(&self) -> (Color, f32) {
        match self.variant {
            ButtonVariant::Secondary => (tokens::button::SECONDARY_BORDER, 1.0),
            _ => (Color::TRANSPARENT, 0.0),
        }
    }

    fn resolve_text_color(&self, is_hovered: bool) -> Color {
        match self.variant {
            ButtonVariant::Primary => tokens::button::PRIMARY_TEXT,
            ButtonVariant::Destructive => tokens::button::DESTRUCTIVE_TEXT,
            ButtonVariant::Secondary => tokens::button::SECONDARY_TEXT,
            ButtonVariant::Ghost => {
                if is_hovered && self.effective_platform() == Platform::Desktop {
                    tokens::button::GHOST_TEXT_HOVER
                } else {
                    tokens::button::GHOST_TEXT
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

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_pressed = state.is_pressed.get();
        let is_hovered = state.is_hovered.get();

        let bg = self.resolve_bg(is_pressed, is_hovered);
        let (border_color, border_width) = self.resolve_border();
        let text_color = self.resolve_text_color(is_hovered);
        let corner_radius = self.resolve_corner_radius();
        let (pt, pr, pb, pl) = self.resolve_padding();
        let opacity = if self.disabled {
            tokens::button::DISABLED_OPACITY
        } else {
            1.0
        };

        let disabled = self.disabled;
        let on_press_cb = self.on_press.clone();
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

        // All decoration on the container. DecoratedContainer defaults to
        // align_self(Start).flex_shrink(0.0), so the container sizes to its
        // content (text intrinsic width + padding + border).
        // Note: layout_builder_methods!()'s padding_each takes (left, right, top, bottom),
        // unlike modifier_methods!()'s (top, right, bottom, left) on Text.
        let mut container = DecoratedContainer::new(text)
            .background(bg)
            .corner_radius(corner_radius)
            .padding_each(pl, pr, pt, pb);

        if border_width > 0.0 {
            container = container.border(border_color, border_width);
        }

        container
            .boxed()
            .on_press(move || {
                if !disabled {
                    is_pressed_signal.set(true);
                    (on_press_cb.borrow_mut())();
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
            .opacity(opacity)
            .align_self(AlignSelf::Start)
    }
}
