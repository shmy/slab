use thiserror::Error;

#[derive(Debug, Error)]
pub enum QualityError {
    #[error("inspection_template_not_found")]
    TemplateNotFound,
    #[error("inspection_order_not_found")]
    InspectionNotFound,
    #[error("non_conformance_not_found")]
    NonConformanceNotFound,
    #[error("invalid_status_transition")]
    InvalidStatus,
}
