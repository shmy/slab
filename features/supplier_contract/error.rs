use thiserror::Error;

#[derive(Debug, Error)]
pub enum SupplierError {
    #[error("supplier_not_found")]
    NotFound,
    #[error("supplier_code_duplicate")]
    CodeDuplicate,
}
