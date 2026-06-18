pub mod file {
    pub const FONT: &[u8] = include_bytes!(".././font.ttf");
    pub const WGSL: &str = include_str!("./shader.wgsl");
    pub const IMAGE_WGSL: &str = include_str!("image_shader.wgsl");
}