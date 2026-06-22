use crate::core::Color;

pub trait Tween<T: Clone> {
    fn lerp(&self, t: f64) -> T;
}

pub struct ColorTween {
    begin: Color,
    end: Color,
}

impl ColorTween {
    pub fn new(begin: Color, end: Color) -> Self {
        Self { begin, end }
    }
}

impl Tween<Color> for ColorTween {
    fn lerp(&self, t: f64) -> Color {
        Color::lerp(self.begin, self.end, t)
    }
}

pub struct FloatTween {
    begin: f32,
    end: f32,
}

impl FloatTween {
    pub fn new(begin: f32, end: f32) -> Self {
        Self { begin, end }
    }
}

impl Tween<f32> for FloatTween {
    fn lerp(&self, t: f64) -> f32 {
        self.begin + (self.end - self.begin) * t as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_tween_lerp() {
        let tween = ColorTween::new(Color::RED, Color::BLUE);
        let mid = tween.lerp(0.5);
        assert!((mid.r - 0.5).abs() < 0.01);
        assert!((mid.b - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_color_tween_boundaries() {
        let tween = ColorTween::new(Color::RED, Color::BLUE);
        assert_eq!(tween.lerp(0.0), Color::RED);
        assert_eq!(tween.lerp(1.0), Color::BLUE);
    }

    #[test]
    fn test_float_tween_lerp() {
        let tween = FloatTween::new(0.0, 100.0);
        assert!((tween.lerp(0.5) - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_float_tween_boundaries() {
        let tween = FloatTween::new(0.0, 100.0);
        assert!((tween.lerp(0.0)).abs() < 0.01);
        assert!((tween.lerp(1.0) - 100.0).abs() < 0.01);
    }
}
