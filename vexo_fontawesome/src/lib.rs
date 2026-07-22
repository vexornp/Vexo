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
/// match the family name in the OTF file's name table. The bundled asset is
/// FontAwesome **7** Free Solid, whose family name is `"Font Awesome 7 Free"`.
/// If the asset is ever swapped, keep this in sync — a mismatch makes
/// `Family::Name(...)` unresolvable and lets cosmic-text's fallback chain pick
/// system symbol fonts (e.g. macOS Webdings/Party LET) for the PUA
/// codepoints, producing wrong icon shapes.
pub const FONT_FAMILY: &str = "Font Awesome 7 Free";

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
        assert_eq!(FONT_FAMILY, "Font Awesome 7 Free");
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

    /// Regression guard: the hardcoded `FONT_FAMILY` constant MUST match the
    /// family name that fontdb parses out of the embedded `fa-solid-900.otf`.
    /// If the asset is swapped for a different FontAwesome major version
    /// (e.g. FA6 → FA7) without updating the constant, `Family::Name(...)`
    /// stops resolving and icon glyphs get served by cosmic-text's fallback
    /// chain — which on macOS picks Webdings/Party LET for the PUA
    /// codepoints, producing wrong icon shapes (while iOS, lacking those
    /// system fonts, still renders correctly via fallback).
    #[test]
    fn font_family_constant_matches_embedded_otf() {
        let mut db = glyphon::fontdb::Database::new();
        db.load_font_source(glyphon::fontdb::Source::Binary(std::sync::Arc::new(
            include_bytes!("../assets/fa-solid-900.otf").to_vec(),
        )));
        let matches: Vec<_> = db
            .faces()
            .filter(|f| f.families.iter().any(|(n, _)| n == FONT_FAMILY))
            .collect();
        assert!(
            !matches.is_empty(),
            "FONT_FAMILY constant {:?} does not match any family in the embedded OTF. \
             Actual families: {:?}. Update FONT_FAMILY (lib.rs) and the codegen constant \
             (build.rs) to match the asset.",
            FONT_FAMILY,
            db.faces()
                .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
                .collect::<std::collections::BTreeSet<_>>()
        );
    }

    /// Regression guard: shaping a tab-bar icon codepoint with `FONT_FAMILY`
    /// MUST resolve to the embedded FontAwesome font, NOT a system fallback
    /// font. On macOS, a broken family name previously let Webdings/Party LET
    /// serve the PUA codepoints (U+F007/F013/F075), rendering wrong icon
    /// shapes. This test reproduces the app's font setup (including system
    /// fonts) and asserts the winning face is the embedded binary.
    #[test]
    fn icon_shaping_selects_fontawesome_not_system_fallback() {
        use glyphon::{Buffer, Family, Metrics, Shaping};

        let mut fs = vexo::resource::new_font_system();
        register_fonts(&mut fs);

        for icon in [Icons::Comment, Icons::User, Icons::Gear] {
            let cp = icon.codepoint();
            let mut buf = Buffer::new(&mut fs, Metrics::new(22.0, 22.0 * 1.2));
            let attrs = glyphon::Attrs::new().family(Family::Name(FONT_FAMILY));
            buf.set_text(cp, &attrs, Shaping::Advanced, None);
            buf.shape_until_scroll(&mut fs, true);

            let mut winners: Vec<&str> = Vec::new();
            for run in buf.layout_runs() {
                for g in run.glyphs.iter() {
                    if let Some(info) = fs.db().face(g.font_id) {
                        let src = match &info.source {
                            glyphon::fontdb::Source::Binary(_) => "binary",
                            glyphon::fontdb::Source::File(_) => "file",
                            glyphon::fontdb::Source::SharedFile(_, _) => "shared",
                        };
                        let fam = info
                            .families
                            .first()
                            .map(|(n, _)| n.as_str())
                            .unwrap_or("?");
                        winners.push(match src {
                            "binary" => fam,
                            _ => "<system>",
                        });
                    }
                }
            }
            assert!(
                winners.iter().all(|w| *w == FONT_FAMILY),
                "icon {:?} ({:?}): expected glyph to come from the embedded FontAwesome \
                 font ({}), but winning fonts were {:?}. The family name is not resolving \
                 and a system fallback is shadowing the icon.",
                icon,
                cp,
                FONT_FAMILY,
                winners
            );
        }
    }
}
