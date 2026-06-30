pub mod button {
    use vexo::Color;

    // Primary variant
    pub const PRIMARY_BG: Color = Color::rgb(0.0, 0.478, 1.0);
    pub const PRIMARY_BG_HOVER: Color = Color::rgb(0.224, 0.612, 1.0);
    pub const PRIMARY_BG_PRESSED: Color = Color::rgb(0.0, 0.353, 0.85);
    pub const PRIMARY_TEXT: Color = Color::WHITE;

    // Secondary variant
    pub const SECONDARY_BG: Color = Color::TRANSPARENT;
    pub const SECONDARY_BORDER: Color = Color::rgb(0.78, 0.78, 0.8);
    pub const SECONDARY_TEXT: Color = Color::rgb(0.0, 0.478, 1.0);

    // Destructive variant
    pub const DESTRUCTIVE_BG: Color = Color::rgb(1.0, 0.231, 0.188);
    pub const DESTRUCTIVE_BG_HOVER: Color = Color::rgb(1.0, 0.388, 0.341);
    pub const DESTRUCTIVE_BG_PRESSED: Color = Color::rgb(0.88, 0.18, 0.14);
    pub const DESTRUCTIVE_TEXT: Color = Color::WHITE;

    // Ghost variant
    pub const GHOST_BG: Color = Color::TRANSPARENT;
    pub const GHOST_TEXT: Color = Color::rgb(0.0, 0.478, 1.0);
    pub const GHOST_TEXT_HOVER: Color = Color::rgb(0.224, 0.612, 1.0);

    // Disabled
    pub const DISABLED_OPACITY: f32 = 0.5;

    // Desktop sizing
    pub const CORNER_RADIUS_DESKTOP: f32 = 6.0;
    pub const PADDING_H_DESKTOP: f32 = 16.0;
    pub const PADDING_V_DESKTOP: f32 = 8.0;

    // Mobile sizing
    pub const CORNER_RADIUS_MOBILE: f32 = 12.0;
    pub const PADDING_H_MOBILE: f32 = 20.0;
    pub const PADDING_V_MOBILE: f32 = 12.0;
}
