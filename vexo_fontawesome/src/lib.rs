//! FontAwesome icon support for the [Vexo](../vexo) UI framework.
//!
//! This crate provides typed FontAwesome 6 Free (Solid) icon widgets.
//! Icons are addressed by a strongly-typed enum ([`Icons`]) rather than raw
//! unicode codepoints, so the compiler catches typos and IDEs can autocomplete
//! icon names.
//!
//! # Quick Start
//!
//! 1. Download the FontAwesome Free assets (see `README.md`) and drop them into
//!    `vexo_fontawesome/assets/`:
//!    - `fa-solid-900.otf`
//!    - `icons.json`
//!
//! 2. Register the font with your app's [`vexo::Application`]:
//!
//!    ```ignore
//!    impl vexo::Application for MyApp {
//!         type State = MyState;
//!         fn new() -> Self::State { /* ... */ }
//!         fn view(state: &mut Self::State) -> Box<dyn vexo::Widget> { /* ... */ }
//!
//!         fn register_fonts(fs: &mut glyphon::FontSystem) {
//!             vexo_fontawesome::register_fonts(fs);
//!         }
//!    }
//!    ```
//!
//! 3. Use [`Icon`] in your widget tree:
//!
//!    ```ignore
//!    use vexo_fontawesome::{Icon, Icons};
//!
//!    Icon::new(Icons::House)
//!         .with_size(24.0)
//!         .with_color(vexo::Color::BLACK)
//!         .boxed()
//!    ```
//!
//! # Styles
//!
//! Only the **Solid** style is currently supported (`fa-solid-900.otf`).
//! FontAwesome's Regular style shares the same family name (`"Font Awesome 6
//! Free"`) but a different weight (400 vs 900); Vexo's `Text` widget does not
//! currently expose font weight, so Regular would collide with Solid and is
//! intentionally omitted. Brands (`"Font Awesome 6 Brands"`) has a distinct
//! family name and could be added in the future without collision.

// Re-export the most commonly needed vexo types for icon consumers.
pub use vexo::{Color, Component, ComponentState, SimpleState, Widget};

mod generated;
mod icon;

pub use generated::Icons;
pub use icon::Icon;

/// The font family name embedded in `fa-solid-900.otf`.
///
/// This is what [`vexo::widgets::Text::with_font_family`] references; it must
/// match the family name in the OTF file's name table (it does for the
/// official FontAwesome Free download).
pub const FONT_FAMILY: &str = "Font Awesome 6 Free";

/// Register the FontAwesome Solid font with an existing `FontSystem`.
///
/// Call this from your [`vexo::Application::register_fonts`] implementation
/// so the icon glyphs are available for shaping before the first frame.
///
/// ```ignore
/// impl vexo::Application for MyApp {
///     fn register_fonts(fs: &mut glyphon::FontSystem) {
///         vexo_fontawesome::register_fonts(fs);
///     }
/// }
/// ```
pub fn register_fonts(font_system: &mut glyphon::FontSystem) {
    vexo::resource::register_font(font_system, include_bytes!("../assets/fa-solid-900.otf"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_family_constant_is_correct() {
        assert_eq!(FONT_FAMILY, "Font Awesome 6 Free");
    }

    #[test]
    fn a_known_icon_has_a_nonempty_codepoint() {
        // House (FA6 renamed "home" → "house") is f015 in FA6 Free Solid.
        assert!(!Icons::House.codepoint().is_empty());
    }

    #[test]
    fn icons_reference_the_solid_font_family() {
        for icon in [Icons::House, Icons::Trash, Icons::ThumbsUp] {
            assert_eq!(icon.family(), FONT_FAMILY);
        }
    }
}
