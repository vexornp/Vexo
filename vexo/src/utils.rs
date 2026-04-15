use core::fmt;

// ============================================================================
// UNIFIED POINT SYSTEM
// ============================================================================

/// Marker type for logical (DPI-independent) coordinates
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Logical;

/// Marker type for physical (screen pixel) coordinates
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Physical;

/// A 2D point in either logical or physical coordinates
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T> {
    pub x: f32,
    pub y: f32,
    _marker: std::marker::PhantomData<T>,
}

/// A 2D size in either logical or physical coordinates
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T> {
    pub width: f32,
    pub height: f32,
    _marker: std::marker::PhantomData<T>,
}

/// A rectangle with origin and size in either logical or physical coordinates
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<T> {
    pub origin: Point<T>,
    pub size: Size<T>,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Point<T> {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Size<T> {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Rect<T> {
    pub fn new(origin: Point<T>, size: Size<T>) -> Self {
        Self {
            origin,
            size,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new(Point::new(x, y), Size::new(width, height))
    }
}

impl Point<Logical> {
    /// Convert logical point to physical pixels
    pub fn to_physical(self, scale: f32) -> Point<Physical> {
        Point::new(self.x * scale, self.y * scale)
    }

    /// Convert from Taffy's Point type
    pub fn from_taffy(p: taffy::Point<f32>) -> Self {
        Point::new(p.x, p.y)
    }

    /// Convert to Taffy's Point type
    pub fn to_taffy(self) -> taffy::Point<f32> {
        taffy::Point { x: self.x, y: self.y }
    }

    /// Convert to array for GPU buffers
    pub fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

impl Point<Physical> {
    /// Convert physical point to logical coordinates
    pub fn to_logical(self, scale: f32) -> Point<Logical> {
        Point::new(self.x / scale, self.y / scale)
    }

    /// Convert to array for GPU buffers
    pub fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

impl Size<Logical> {
    /// Convert logical size to physical pixels
    pub fn to_physical(self, scale: f32) -> Size<Physical> {
        Size::new(self.width * scale, self.height * scale)
    }

    /// Convert from Taffy's Size type
    pub fn from_taffy(s: taffy::Size<f32>) -> Self {
        Size::new(s.width, s.height)
    }

    /// Convert to Taffy's Size type
    pub fn to_taffy(self) -> taffy::Size<f32> {
        taffy::Size { width: self.width, height: self.height }
    }

    /// Convert to array for GPU buffers
    pub fn to_array(self) -> [f32; 2] {
        [self.width, self.height]
    }
}

impl Size<Physical> {
    /// Convert physical size to logical coordinates
    pub fn to_logical(self, scale: f32) -> Size<Logical> {
        Size::new(self.width / scale, self.height / scale)
    }

    /// Convert to array for GPU buffers
    pub fn to_array(self) -> [f32; 2] {
        [self.width, self.height]
    }
}

impl Rect<Logical> {
    /// Convert logical rect to physical pixels
    pub fn to_physical(self, scale: f32) -> Rect<Physical> {
        Rect::new(
            self.origin.to_physical(scale),
            self.size.to_physical(scale),
        )
    }

    /// Create from layout result
    pub fn from_layout(location: taffy::Point<f32>, size: taffy::Size<f32>) -> Self {
        Rect::new(Point::from_taffy(location), Size::from_taffy(size))
    }
}

impl Rect<Physical> {
    /// Convert physical rect to logical coordinates
    pub fn to_logical(self, scale: f32) -> Rect<Logical> {
        Rect::new(
            self.origin.to_logical(scale),
            self.size.to_logical(scale),
        )
    }
}

impl<T> std::ops::Add for Point<T> {
    type Output = Point<T>;

    fn add(self, other: Point<T>) -> Point<T> {
        Point::new(self.x + other.x, self.y + other.y)
    }
}

impl<T> std::ops::AddAssign for Point<T> {
    fn add_assign(&mut self, other: Point<T>) {
        self.x += other.x;
        self.y += other.y;
    }
}

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

#[derive(Clone, Copy)]
pub struct PhysicalLocation(Point<Physical>);

impl PhysicalLocation {
    pub fn new(pos: winit::dpi::PhysicalPosition<f64>) -> Self {
        Self(Point::new(pos.x as f32, pos.y as f32))
    }

    pub fn default() -> Self {
        Self(Point::new(0.0, 0.0))
    }

    pub fn x(&self) -> f64 {
        self.0.x as f64
    }

    pub fn y(&self) -> f64 {
        self.0.y as f64
    }

    pub fn to_logical(self, scale: &Scale) -> Point<Logical> {
        self.0.to_logical(scale.factor())
    }

    fn to_taffy_point(&self, scale: &Scale) -> taffy::Point<f32> {
        self.to_logical(scale).to_taffy()
    }
}

// Check if a physical position is inside a TaffyQuad, considering the scale factor
pub fn is_location_inside_quad(
    location: &PhysicalLocation,
    scale: &Scale,
    quad: &TaffyQuad,
) -> bool {
    let logical_pos = location.to_logical(scale);
    let x = logical_pos.x;
    let y = logical_pos.y;

    x >= quad.location.x
        && x <= quad.location.x + quad.size.width
        && y >= quad.location.y
        && y <= quad.location.y + quad.size.height
}
