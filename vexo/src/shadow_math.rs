//! Rust port of the WGSL shadow alpha formula, for testing.
//!
//! The WGSL fragment shader (`vexo/src/shader.wgsl`) mirrors this formula
//! exactly. Tests here verify that the formula matches Flutter's Skia
//! `MaskFilter.blur(BlurStyle.normal, blurSigma)` where `blurSigma = blur_radius / 2`.

/// Compute shadow alpha at a given distance from the silhouette edge.
///
/// - `distance_from_silhouette`: SDF distance. Negative = inside silhouette
///   (alpha = full). Zero = on edge. Positive = outside.
/// - `blur_px`: blur radius in physical pixels.
/// - `alpha`: the shadow color's alpha channel (0.0 to 1.0).
///
/// Returns the final alpha (0.0 to 1.0).
#[allow(dead_code)]
pub fn shadow_alpha(distance_from_silhouette: f32, blur_px: f32, alpha: f32) -> f32 {
    let sigma = (blur_px * 0.5).max(0.5);
    let d = distance_from_silhouette.max(0.0);
    let falloff = (-d * d / (2.0 * sigma * sigma)).exp();
    falloff * alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_alpha_at_silhouette_edge() {
        // distance = 0 → falloff = exp(0) = 1.0 → alpha = full alpha
        let result = shadow_alpha(0.0, 8.0, 0.5);
        assert!(
            (result - 0.5).abs() < 0.001,
            "alpha at edge should equal full alpha"
        );
    }

    #[test]
    fn test_shadow_alpha_at_blur_radius() {
        // distance = blur_radius, sigma = blur/2
        // falloff = exp(-blur^2 / (2 * (blur/2)^2)) = exp(-blur^2 / (blur^2/2)) = exp(-2) ≈ 0.135
        // alpha = 0.135 * 0.5 ≈ 0.0675
        let result = shadow_alpha(8.0, 8.0, 0.5);
        let expected = 0.135 * 0.5;
        assert!(
            (result - expected).abs() < 0.01,
            "alpha at blur_radius should be ~0.135 * alpha (matches Flutter), got {}",
            result
        );
    }

    #[test]
    fn test_shadow_alpha_at_zero_blur() {
        // blur = 0, sigma clamped to 0.5
        // distance > 0 → falloff = exp(-d^2 / 0.5) — decays very fast (sharp edge)
        let result = shadow_alpha(1.0, 0.0, 1.0);
        assert!(
            result < 0.2,
            "at zero blur, distance=1 should be near-zero alpha"
        );
    }

    #[test]
    fn test_shadow_alpha_inside_silhouette() {
        // distance < 0 → clamped to 0 → falloff = 1.0 → alpha = full alpha
        let result = shadow_alpha(-10.0, 8.0, 0.5);
        assert!(
            (result - 0.5).abs() < 0.001,
            "inside silhouette, alpha = full alpha"
        );
    }

    #[test]
    fn test_shadow_alpha_zero_alpha_input() {
        let result = shadow_alpha(0.0, 8.0, 0.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_shadow_alpha_decays_with_distance() {
        let near = shadow_alpha(4.0, 8.0, 1.0);
        let far = shadow_alpha(16.0, 8.0, 1.0);
        assert!(near > far, "alpha should decay with distance");
        assert!(near > 0.0 && near < 1.0);
        assert!(far > 0.0 && far < near);
    }
}
