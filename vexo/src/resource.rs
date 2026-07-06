pub mod file {
    pub const FONT: &[u8] = include_bytes!(".././font.ttf");
    pub const WGSL: &str = include_str!("./shader.wgsl");
    pub const IMAGE_WGSL: &str = include_str!("image_shader.wgsl");
}

/// The family name of the embedded `FONT`. Used to override cosmic-text's
/// hardcoded defaults ("Open Sans" / "DejaVu Serif" / "Noto Sans Mono"),
/// which don't exist on iOS and would otherwise cause shaping to panic
/// after exhausting the (Linux-style) platform fallback chain.
pub const EMBEDDED_FONT_FAMILY: &str = "Roboto";

/// Construct a `FontSystem` preloaded with the embedded font and with the
/// default sans-serif/serif/monospace families pointed at it.
///
/// Use this everywhere a `FontSystem` is needed (window state, throwaway
/// controllers, tests) so the iOS font-resolution bug can't recur.
pub fn new_font_system() -> glyphon::FontSystem {
    let font_data = file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(alloc::sync::Arc::new(font_data));
    let mut font_system = glyphon::FontSystem::new_with_fonts([binary]);
    let db = font_system.db_mut();
    db.set_sans_serif_family(EMBEDDED_FONT_FAMILY);
    db.set_serif_family(EMBEDDED_FONT_FAMILY);
    db.set_monospace_family(EMBEDDED_FONT_FAMILY);
    font_system
}

/// Register an additional font (e.g. an icon font) with an existing
/// `FontSystem`.
///
/// The font's family name (read from its name table) is what
/// [`crate::widgets::Text::with_font_family`] references. The bytes are
/// copied into an `Arc<Vec<u8>>` owned by the font database, so the caller's
/// `bytes` slice need not be kept alive after this call returns.
///
/// # Example
///
/// ```ignore
/// impl Application for MyApp {
///     fn register_fonts(fs: &mut glyphon::FontSystem) {
///         vexo::resource::register_font(fs, include_bytes!("../assets/iconfont.ttf"));
///     }
/// }
/// ```
pub fn register_font(font_system: &mut glyphon::FontSystem, bytes: &[u8]) {
    let source = glyphon::fontdb::Source::Binary(alloc::sync::Arc::new(bytes.to_vec()));
    font_system.db_mut().load_font_source(source);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_font_increases_face_count() {
        let mut fs = new_font_system();
        let before = fs.db().faces().count();
        // Register the embedded font again as an additional source. It's a
        // valid TTF, so the face count must grow.
        register_font(&mut fs, file::FONT);
        let after = fs.db().faces().count();
        assert!(after > before, "register_font must add a face to the db");
    }

    #[test]
    fn register_font_makes_family_resolvable() {
        // The embedded font's family is "Roboto". Registering it and then
        // querying for that family must yield at least one match.
        let mut fs = glyphon::FontSystem::new();
        register_font(&mut fs, file::FONT);
        let query = glyphon::fontdb::Query {
            families: &[glyphon::fontdb::Family::Name(EMBEDDED_FONT_FAMILY)],
            weight: glyphon::fontdb::Weight::NORMAL,
            stretch: glyphon::fontdb::Stretch::Normal,
            style: glyphon::fontdb::Style::Normal,
        };
        let id = fs.db().query(&query);
        assert!(id.is_some(), "registered family must be resolvable");
    }
}
