use thiserror::Error;

#[derive(Debug, Error)]
pub enum CustomerError {
    #[error("customer_not_found")]
    NotFound,
    #[error("customer_code_duplicate")]
    CodeDuplicate,
}
