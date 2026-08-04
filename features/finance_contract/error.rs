use thiserror::Error;

#[derive(Debug, Error)]
pub enum FinanceError {
    #[error("payment_not_found")]
    PaymentNotFound,
    #[error("invoice_not_found")]
    InvoiceNotFound,
    #[error("invalid_invoice_type")]
    InvalidInvoiceType,
    #[error("invoice_already_fully_paid")]
    InvoiceAlreadyFullyPaid,
    #[error("invalid_payment_amount")]
    InvalidPaymentAmount,
    #[error("invalid_period")]
    InvalidPeriod,
}
