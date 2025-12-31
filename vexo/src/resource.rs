pub mod file {
    pub const FONT: &[u8; 468308] = include_bytes!(".././font.ttf");
    pub const WGSL: &str = include_str!("./shader.wgsl");
}
