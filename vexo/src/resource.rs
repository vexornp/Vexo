pub mod file {
    pub const FONT: &'static [u8] = include_bytes!(".././font.ttf");
    pub const WGSL: &str = include_str!("./shader.wgsl");
}
