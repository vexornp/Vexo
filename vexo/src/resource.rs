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
