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
    use vexo::{Color, ThemeData};

    /// Theme-aware navigation colors resolved from a `ThemeData`.
    pub struct NavColors {
        pub sidebar_bg: Color,
        pub header_bg: Color,
        pub header_text: Color,
        pub row_bg: Color,
        pub row_text: Color,
        pub selected_bg: Color,
        pub selected_text: Color,
        pub detail_bg: Color,
        pub divider: Color,
        pub placeholder_text: Color,
        pub mobile_header_bg: Color,
        pub mobile_title: Color,
        pub back_color: Color,
    }

    /// Resolve navigation colors from a `ThemeData`.
    pub fn colors(t: &ThemeData) -> NavColors {
        // Chrome bars (mobile tab bar, mobile nav bar, desktop sidebar, and
        // desktop panel headers via `titled_container`) use a dark gray in
        // dark mode matching the winit window status bar color (#24282B).
        // Light mode keeps `surface` (white). Other surfaces (rows, cards,
        // page background) stay on `surface`/`background` so only the bars flip.
        let bar_bg = if t.is_dark() {
            Color::from_hex(0x24282BFF)
        } else {
            t.surface
        };
        NavColors {
            sidebar_bg: bar_bg,
            header_bg: bar_bg,
            header_text: t.on_surface,
            row_bg: Color::TRANSPARENT,
            row_text: t.on_surface,
            selected_bg: t.primary,
            selected_text: t.on_primary,
            detail_bg: t.background,
            // Hairline separator color. Opaque (pre-composited) so it renders
            // identically regardless of the backdrop behind it — the Me page
            // divider sits on a `surface` card, the nav hairlines sit on the
            // chrome `bar_bg`; a translucent color would composite differently
            // against each and look inconsistent (especially in dark mode).
            // The value is `outline @ DIVIDER_ALPHA` composited over `surface`,
            // i.e. exactly what the Me page's in-card separator previously
            // rendered as, so light mode is visually unchanged and dark mode
            // now matches the Me page. `t.outline` itself stays opaque for
            // borders (buttons, chat input) that need full strength.
            divider: Color::lerp(t.outline, t.surface, 1.0 - DIVIDER_ALPHA as f64),
            placeholder_text: t.on_surface_variant,
            mobile_header_bg: bar_bg,
            mobile_title: t.on_surface,
            back_color: t.primary,
        }
    }

    // Theme-independent constants (sizing, padding, strings, font sizes).

    pub const SIDEBAR_WIDTH: f32 = 64.0;
    pub const COLLAPSED_WIDTH: f32 = 44.0;
    pub const CONVERSATION_LIST_WIDTH: f32 = 300.0;

    pub const HEADER_PADDING: f32 = 12.0;
    pub const HEADER_FONT_SIZE: f32 = 16.0;

    pub const ROW_PADDING: f32 = 10.0;
    pub const ROW_FONT_SIZE: f32 = 16.0;

    pub const PLACEHOLDER_FONT_SIZE: f32 = 16.0;

    pub const MOBILE_HEADER_HEIGHT: f32 = 44.0;
    pub const MOBILE_HEADER_PADDING: f32 = 8.0;

    /// Thickness (logical px) of the hairline separator along a bar's edge
    /// (nav bar bottom, tab bar top).
    ///
    /// Taffy floors layout dimensions to integers, so a sub-pixel height
    /// (e.g. `1/scale` = 0.5 at 2×) collapses to 0 and renders nothing. 1
    /// logical px is the smallest height that survives layout; it renders as
    /// 1 physical px at 1× and 2 at 2×, matching macOS `Divider`.
    pub const HAIRLINE_THICKNESS: f32 = 1.0;

    /// Alpha applied to `outline` for hairline separators. iOS separators are
    /// translucent (~0.2–0.35); a fully opaque outline reads as a bold rule
    /// rather than a subtle seam. Shared by nav chrome hairlines and the Me
    /// page's in-card row dividers so the two stay identical.
    pub const DIVIDER_ALPHA: f32 = 0.35;

    pub const BACK_CHEVRON: &str = "\u{2039}"; // ‹
    pub const BACK_LABEL: &str = "Back";
    pub const BACK_FONT_SIZE: f32 = 17.0;
    /// Font size (logical px) for the FontAwesome back-chevron icon.
    /// Slightly larger than `BACK_FONT_SIZE` because chevron glyphs have
    /// significant empty space within their em box.
    pub const BACK_ICON_SIZE: f32 = 22.0;
    /// Horizontal gap between the back-chevron icon and the "Back" label.
    pub const BACK_ICON_LABEL_GAP: f32 = 6.0;

    pub const MOBILE_TITLE_FONT_SIZE: f32 = 17.0;

    /// Drop shadow cast by the moving page during mobile push/pop transitions.
    ///
    /// Full-perimeter `BoxShadow` clipped to the nav content area by the
    /// ancestor clip wrapper in `NavigationStackView::render`, so only the
    /// leading-edge strip is visible. Matches iOS native push animation.
    ///
    /// Constructed as `Color::BLACK.with_alpha(PAGE_SHADOW_ALPHA)` with
    /// `.blur(PAGE_SHADOW_BLUR)`; zero offset, zero spread (the ancestor clip
    /// does the edge restriction, not the offset).
    pub const PAGE_SHADOW_ALPHA: f32 = 0.1;
    pub const PAGE_SHADOW_BLUR: f32 = 8.0;
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

    use super::navigation::{colors as nav_colors, NavColors, DIVIDER_ALPHA};

    #[test]
    fn nav_colors_light_maps_roles() {
        let t = ThemeData::light();
        let n = nav_colors(&t);
        assert_eq!(n.sidebar_bg, t.surface);
        assert_eq!(n.header_bg, t.surface);
        assert_eq!(n.header_text, t.on_surface);
        assert_eq!(n.row_bg, Color::TRANSPARENT);
        assert_eq!(n.row_text, t.on_surface);
        assert_eq!(n.selected_bg, t.primary);
        assert_eq!(n.selected_text, t.on_primary);
        assert_eq!(n.detail_bg, t.background);
        assert_eq!(
            n.divider,
            Color::lerp(t.outline, t.surface, 1.0 - DIVIDER_ALPHA as f64)
        );
        assert_eq!(n.placeholder_text, t.on_surface_variant);
        assert_eq!(n.mobile_header_bg, t.surface);
        assert_eq!(n.mobile_title, t.on_surface);
        assert_eq!(n.back_color, t.primary);
    }

    #[test]
    fn nav_colors_dark_maps_roles() {
        let t = ThemeData::dark();
        let n = nav_colors(&t);
        // Chrome bars (sidebar / mobile header / desktop panel header) match
        // the winit window status bar dark-mode color (#24282B), not pure black.
        assert_eq!(n.sidebar_bg, Color::from_hex(0x24282BFF));
        assert_eq!(n.mobile_header_bg, Color::from_hex(0x24282BFF));
        assert_eq!(n.header_bg, Color::from_hex(0x24282BFF));
        assert_eq!(n.selected_bg, t.primary);
        assert_eq!(
            n.divider,
            Color::lerp(t.outline, t.surface, 1.0 - DIVIDER_ALPHA as f64)
        );
    }

    #[test]
    fn nav_colors_is_a_struct() {
        let _ = NavColors {
            sidebar_bg: Color::WHITE,
            header_bg: Color::WHITE,
            header_text: Color::WHITE,
            row_bg: Color::WHITE,
            row_text: Color::WHITE,
            selected_bg: Color::WHITE,
            selected_text: Color::WHITE,
            detail_bg: Color::WHITE,
            divider: Color::WHITE,
            placeholder_text: Color::WHITE,
            mobile_header_bg: Color::WHITE,
            mobile_title: Color::WHITE,
            back_color: Color::WHITE,
        };
    }
}
