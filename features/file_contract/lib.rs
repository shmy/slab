use crate::error::FileError;
use chrono::Utc;
use imageformat::{ImageFormat, detect_image_format};
use rootcause::Result;

pub mod error;

macro_rules! dir_with_prefix {
    ($dir:literal) => {
        pub const STAGING_DIR: &str = $dir;
        pub const STORAGE_DIR_PREFIX: &str = concat!($dir, "/");
    };
}

dir_with_prefix!("staging");

#[inline(always)]
fn random_file_name() -> String {
    tempoid::TempoId::generate().to_string()
}

pub fn strip_staging_prefix(path: String) -> String {
    path.strip_prefix(STORAGE_DIR_PREFIX)
        .map(|s| s.to_string())
        .unwrap_or(path)
}

pub fn staging_dated_file_path(extension: &str) -> String {
    format!(
        "{}{}/{}.{}",
        STORAGE_DIR_PREFIX,
        Utc::now().format("%Y/%m"),
        random_file_name(),
        extension
    )
}

pub fn ensure_image_reader<R: std::io::Read>(reader: &mut R) -> Result<()> {
    let Ok(format) = detect_image_format(reader) else {
        return Err(FileError::NotImage.into());
    };
    match format {
        ImageFormat::Jpeg | ImageFormat::JpegXl | ImageFormat::Png | ImageFormat::Webp => Ok(()),
        _ => Err(FileError::NotImage.into()),
    }
}
