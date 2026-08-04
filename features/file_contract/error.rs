#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("file_not_image")]
    NotImage,
    #[error("file_name_missing")]
    FileNameMissing,
}
