#[derive(Debug, thiserror::Error)]
pub enum ImageKitError {
    #[error("decode image failed: {0}")]
    Decode(image::ImageError),
    #[error("encode image failed: {0}")]
    Encode(image::ImageError),
}
