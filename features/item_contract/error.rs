use thiserror::Error;

#[derive(Debug, Error)]
pub enum ItemError {
    #[error("item_not_found")]
    NotFound,
    #[error("item_code_duplicate")]
    CodeDuplicate,
    #[error("item_category_not_found")]
    CategoryNotFound,
    #[error("item_category_not_empty")]
    CategoryNotEmpty,
    #[error("item_has_references")]
    ItemReferenceExists,
}
