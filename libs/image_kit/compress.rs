use crate::error::ImageKitError;
use image::GenericImageView;
use webp::Encoder;

#[derive(Debug, Clone, Copy)]
pub struct CompressOptions {
    pub quality: u8,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self { quality: 75 }
    }
}

#[derive(Debug)]
pub struct CompressOutput {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub size: u64,
}

pub struct ImageKit;

impl ImageKit {
    #[tracing::instrument(skip_all)]
    pub fn compress_to_webp(
        input: &[u8],
        options: CompressOptions,
    ) -> Result<CompressOutput, ImageKitError> {
        let image = image::load_from_memory(input).map_err(ImageKitError::Decode)?;
        let (width, height) = image.dimensions();
        let rgba = image.to_rgba8();
        let encoder = Encoder::from_rgba(rgba.as_raw(), width, height);
        let bytes = encoder.encode(f32::from(options.quality)).to_vec();
        let size = bytes.len() as u64;
        Ok(CompressOutput {
            bytes,
            width,
            height,
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::jpeg::JpegEncoder;
    use image::{ImageBuffer, ImageEncoder, Rgb};
    use std::io::Cursor;

    fn create_jpeg(width: u32, height: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(width, height, |_, _| Rgb([230, 120, 32]));
        let mut out = Vec::new();
        let mut cursor = Cursor::new(&mut out);
        let encoder = JpegEncoder::new_with_quality(&mut cursor, 85);
        encoder
            .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgb8)
            .expect("encode jpeg fixture");
        out
    }

    #[test]
    fn convert_jpeg_to_webp() {
        let source = create_jpeg(120, 80);
        let out = ImageKit::compress_to_webp(&source, CompressOptions::default())
            .expect("convert to webp");
        let format = image::guess_format(&out.bytes).expect("guess format");
        assert_eq!(format, image::ImageFormat::WebP);
        assert_eq!((out.width, out.height), (120, 80));
    }

    #[test]
    fn keep_image_size_unchanged() {
        let source = create_jpeg(2400, 1200);
        let out =
            ImageKit::compress_to_webp(&source, CompressOptions { quality: 70 }).expect("compress");
        assert_eq!((out.width, out.height), (2400, 1200));
    }

    #[test]
    fn invalid_input_returns_decode_error() {
        let err = ImageKit::compress_to_webp(b"not an image", CompressOptions { quality: 75 })
            .expect_err("should fail");
        assert!(matches!(err, ImageKitError::Decode(_)));
    }
}
