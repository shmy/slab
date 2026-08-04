use thiserror::Error;

#[derive(Debug, Error)]
pub enum SalesError {
    #[error("sales_document_not_found")]
    NotFound,
    /// 发货时订单尚未批准。
    #[error("sales_order_not_approved")]
    OrderNotApproved,
    /// 发货明细引用的订单行不存在。
    #[error("sales_order_line_not_found")]
    LineNotFound,
    /// 发货数量超过订单行剩余数量。
    #[error("sales_over_delivery")]
    OverDelivery,
    /// 订单至少需要一行明细。
    #[error("sales_order_empty")]
    EmptyOrder,
    #[error("sales_insufficient_inventory")]
    InsufficientInventory,
}
