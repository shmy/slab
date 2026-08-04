use thiserror::Error;

#[derive(Debug, Error)]
pub enum PurchaseError {
    #[error("purchase_order_not_found")]
    NotFound,
    /// 收货时订单尚未批准。
    #[error("purchase_order_not_approved")]
    OrderNotApproved,
    /// 删除时订单不是草稿。
    #[error("purchase_order_not_draft")]
    NotDraft,
    /// 订单至少需要一行明细。
    #[error("purchase_order_empty")]
    EmptyOrder,
    #[error("purchase_order_line_not_found")]
    LineNotFound,
    #[error("purchase_over_receipt")]
    OverReceipt,
}
