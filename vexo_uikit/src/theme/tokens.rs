pub mod button {
    use vexo::{Color, ThemeData};

    /// Theme-aware button colors resolved from a `ThemeData`.
    ///
    /// Produced by [`colors`]. Hover/pressed shades are derived via
    /// `Color::lerp` so they stay correct if `primary`/`error` change.
    pub struct ButtonColors {
        pub primary_bg: Color,
        pub primary_bg_hover: Color,
        pub primary_bg_pressed: Color,
        pub primary_text: Color,
        pub secondary_bg: Color,
        pub secondary_border: Color,
        pub secondary_text: Color,
        pub destructive_bg: Color,
        pub destructive_bg_hover: Color,
        pub destructive_bg_pressed: Color,
        pub destructive_text: Color,
        pub ghost_bg: Color,
        pub ghost_text: Color,
        pub ghost_text_hover: Color,
    }

    /// Resolve button colors from a `ThemeData`.
    pub fn colors(t: &ThemeData) -> ButtonColors {
        ButtonColors {
            primary_bg: t.primary,
            primary_bg_hover: Color::lerp(t.primary, Color::WHITE, 0.15),
            primary_bg_pressed: Color::lerp(t.primary, Color::BLACK, 0.15),
            primary_text: t.on_primary,
            secondary_bg: Color::TRANSPARENT,
            secondary_border: t.outline,
            secondary_text: t.primary,
            destructive_bg: t.error,
            destructive_bg_hover: Color::lerp(t.error, Color::WHITE, 0.15),
            destructive_bg_pressed: Color::lerp(t.error, Color::BLACK, 0.15),
            destructive_text: t.on_error,
            ghost_bg: Color::TRANSPARENT,
            ghost_text: t.primary,
            ghost_text_hover: Color::lerp(t.primary, Color::WHITE, 0.15),
        }
    }

    // Theme-independent constants (sizing, padding, font sizes).

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

    // Mobile (push/pop) detail page header.
    // On mobile the sidebar and detail are never shown side-by-side; selecting
    // an item pushes the detail page, which has its own header with a back
    // chevron + label and a title reflecting the selected item.
    pub const MOBILE_HEADER_BG: Color = Color::rgb(0.98, 0.98, 0.98);
    pub const MOBILE_HEADER_HEIGHT: f32 = 44.0;
    pub const MOBILE_HEADER_PADDING: f32 = 8.0;
    pub const MOBILE_HEADER_DIVIDER: Color = Color::rgb(0.85, 0.85, 0.85);

    // Back chevron + label (iOS-style tint blue, matches SELECTED_BG)
    pub const BACK_CHEVRON: &str = "\u{2039}"; // ‹
    pub const BACK_LABEL: &str = "Back";
    pub const BACK_FONT_SIZE: f32 = 17.0;
    pub const BACK_COLOR: Color = Color::rgb(0.0, 0.478, 1.0);

    // Detail page title (selected item's label)
    pub const MOBILE_TITLE_FONT_SIZE: f32 = 17.0;
    pub const MOBILE_TITLE_COLOR: Color = Color::rgb(0.1, 0.1, 0.1);
}

#[cfg(test)]
mod tests {
    use super::button::{colors, ButtonColors};
    use vexo::{Color, ThemeData};

    #[test]
    fn button_colors_light_maps_roles() {
        let t = ThemeData::light();
        let c = colors(&t);
        assert_eq!(c.primary_bg, t.primary);
        assert_eq!(c.primary_text, t.on_primary);
        assert_eq!(c.secondary_bg, Color::TRANSPARENT);
        assert_eq!(c.secondary_border, t.outline);
        assert_eq!(c.secondary_text, t.primary);
        assert_eq!(c.destructive_bg, t.error);
        assert_eq!(c.destructive_text, t.on_error);
        assert_eq!(c.ghost_bg, Color::TRANSPARENT);
        assert_eq!(c.ghost_text, t.primary);
    }

    #[test]
    fn button_colors_hover_pressed_are_lerp() {
        let t = ThemeData::light();
        let c = colors(&t);
        assert_eq!(
            c.primary_bg_hover,
            Color::lerp(t.primary, Color::WHITE, 0.15)
        );
        assert_eq!(
            c.primary_bg_pressed,
            Color::lerp(t.primary, Color::BLACK, 0.15)
        );
        assert_eq!(
            c.destructive_bg_hover,
            Color::lerp(t.error, Color::WHITE, 0.15)
        );
        assert_eq!(
            c.destructive_bg_pressed,
            Color::lerp(t.error, Color::BLACK, 0.15)
        );
        assert_eq!(
            c.ghost_text_hover,
            Color::lerp(t.primary, Color::WHITE, 0.15)
        );
    }

    #[test]
    fn button_colors_dark_maps_roles() {
        let t = ThemeData::dark();
        let c = colors(&t);
        assert_eq!(c.primary_bg, t.primary);
        assert_eq!(c.destructive_bg, t.error);
        assert_eq!(c.secondary_border, t.outline);
    }

    #[test]
    fn button_colors_is_a_struct() {
        // Compile-time check that ButtonColors is nameable and field-accessible.
        let _ = ButtonColors {
            primary_bg: Color::WHITE,
            primary_bg_hover: Color::WHITE,
            primary_bg_pressed: Color::WHITE,
            primary_text: Color::WHITE,
            secondary_bg: Color::WHITE,
            secondary_border: Color::WHITE,
            secondary_text: Color::WHITE,
            destructive_bg: Color::WHITE,
            destructive_bg_hover: Color::WHITE,
            destructive_bg_pressed: Color::WHITE,
            destructive_text: Color::WHITE,
            ghost_bg: Color::WHITE,
            ghost_text: Color::WHITE,
            ghost_text_hover: Color::WHITE,
        };
    }
}
