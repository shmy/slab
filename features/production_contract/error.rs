use thiserror::Error;
#[derive(Debug, Error)]
pub enum ProductionError {
    #[error("work_order_not_found")]
    NotFound,
    #[error("invalid_status_transition")]
    InvalidStatus,
    #[error("insufficient_materials")]
    InsufficientMaterials,
}
