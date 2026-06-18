#[derive(Clone, Debug)]
pub struct ImageData {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageDataError(String);

impl std::fmt::Display for ImageDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ImageDataError {}

impl ImageData {
    pub fn from_bytes(bytes: &[u8]) -> Result<ImageData, ImageDataError> {
        let img = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| ImageDataError(format!("Failed to guess image format: {}", e)))?
            .decode()
            .map_err(|e| ImageDataError(format!("Failed to decode image: {}", e)))?;

        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();

        if width == 0 || height == 0 {
            return Err(ImageDataError("Decoded image has zero dimensions".into()));
        }

        Ok(ImageData {
            pixels: rgba.into_raw(),
            width,
            height,
        })
    }
}
