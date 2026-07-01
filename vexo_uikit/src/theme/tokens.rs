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

    // Desktop sizing (matches macOS SwiftUI .bordered, regular control size)
    pub const CORNER_RADIUS_DESKTOP: f32 = 5.0;
    pub const PADDING_H_DESKTOP: f32 = 12.0;
    pub const PADDING_V_DESKTOP: f32 = 4.0;
    pub const FONT_SIZE_DESKTOP: f32 = 13.0;

    // Mobile sizing (matches iOS SwiftUI .bordered, regular control size)
    pub const CORNER_RADIUS_MOBILE: f32 = 8.0;
    pub const PADDING_H_MOBILE: f32 = 16.0;
    pub const PADDING_V_MOBILE: f32 = 8.0;
    pub const FONT_SIZE_MOBILE: f32 = 17.0;
}

pub mod navigation {
    use vexo::Color;

    // Sidebar
    pub const SIDEBAR_BG: Color = Color::rgb(0.95, 0.95, 0.97);
    pub const SIDEBAR_WIDTH: f32 = 240.0;
    pub const COLLAPSED_WIDTH: f32 = 44.0;

    // Sidebar header
    pub const HEADER_BG: Color = Color::rgb(0.9, 0.9, 0.92);
    pub const HEADER_TEXT_COLOR: Color = Color::rgb(0.2, 0.2, 0.2);
    pub const HEADER_PADDING: f32 = 12.0;
    pub const HEADER_FONT_SIZE: f32 = 16.0;

    // Sidebar rows
    pub const ROW_PADDING: f32 = 10.0;
    pub const ROW_FONT_SIZE: f32 = 16.0;
    pub const ROW_BG: Color = Color::TRANSPARENT;
    pub const ROW_TEXT_COLOR: Color = Color::rgb(0.1, 0.1, 0.1);
    pub const SELECTED_BG: Color = Color::rgb(0.0, 0.478, 1.0);
    pub const SELECTED_TEXT_COLOR: Color = Color::WHITE;

    // Detail pane
    pub const DETAIL_BG: Color = Color::WHITE;
    pub const DIVIDER_COLOR: Color = Color::rgb(0.85, 0.85, 0.85);
    pub const PLACEHOLDER_TEXT_COLOR: Color = Color::rgb(0.6, 0.6, 0.6);
    pub const PLACEHOLDER_FONT_SIZE: f32 = 16.0;
}
