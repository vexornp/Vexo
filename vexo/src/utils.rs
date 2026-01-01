use core::fmt;
use std::fmt::write;

use winit::dpi::LogicalPosition;

pub struct TaffyQuad {
    location: taffy::Point<f32>,
    size: taffy::Size<f32>,
}

impl fmt::Display for TaffyQuad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(x: {}, y: {}, w: {}, h: {})",
            self.location.x, self.location.y, self.size.width, self.size.height
        )
    }
}

impl TaffyQuad {
    pub fn new(location: taffy::Point<f32>, size: taffy::Size<f32>) -> Self {
        Self { location, size }
    }

    pub fn from(x: f32, y: f32, size: taffy::Size<f32>) -> Self {
        TaffyQuad::new(taffy::Point { x: x, y: y }, size)
    }
}

pub struct Scale(f64);
impl Scale {
    pub fn new(factor: f64) -> Self {
        Self(factor)
    }

    pub fn factor(&self) -> f32 {
        self.0 as f32
    }
}

pub struct PhysicalLocation(winit::dpi::PhysicalPosition<f64>);
impl PhysicalLocation {
    pub fn new(pos: winit::dpi::PhysicalPosition<f64>) -> Self {
        Self(pos)
    }

    pub fn default() -> Self {
        Self(winit::dpi::PhysicalPosition::new(0.0, 0.0))
    }

    pub fn x(&self) -> f64 {
        self.0.x
    }

    pub fn y(&self) -> f64 {
        self.0.y
    }

    fn to_taffy_point(&self, scale: &Scale) -> taffy::Point<f32> {
        let logical_pos: LogicalPosition<f32> = self.0.to_logical(scale.0);
        taffy::Point {
            x: logical_pos.x,
            y: logical_pos.y,
        }
    }
}

// Check if a physical position is inside a TaffyQuad, considering the scale factor
pub fn is_location_inside_quad(
    location: &PhysicalLocation,
    scale: &Scale,
    quad: &TaffyQuad,
) -> bool {
    let logical_pos = location.to_taffy_point(&scale);
    let scaled_x = logical_pos.x;
    let scaled_y = logical_pos.y;

    scaled_x >= quad.location.x
        && scaled_x <= quad.location.x + quad.size.width
        && scaled_y >= quad.location.y
        && scaled_y <= quad.location.y + quad.size.height
}
