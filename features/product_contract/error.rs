use thiserror::Error;
#[derive(Debug, Error)]
pub enum ProductError {
    #[error("bom_not_found")]
    BomNotFound,
    #[error("mold_not_found")]
    MoldNotFound,
    #[error("invalid_status_transition")]
    InvalidStatus,
}
