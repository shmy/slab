use shared_contract::value_object::id::ID;

/// 销售订单
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct SalesOrder {
    pub id: ID,
    pub code: String,
    pub customer_id: ID,
    pub status: i16, // SalesOrderStatus（审批流状态）
    pub order_date: chrono::NaiveDate,
    pub currency: String,
    pub total_amount: i64,
    pub remark: Option<String>,
    pub created_by: Option<ID>,
}

/// 销售订单行
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct SalesOrderLine {
    pub id: ID,
    pub order_id: ID,
    pub line_no: i16,
    pub item_id: ID,
    pub quantity: i64,
    pub unit: String,
    pub unit_price: i64,
    pub line_total: i64,
    pub delivered_qty: i64,
    pub returned_qty: i64,
    pub closed: bool,
    pub remark: Option<String>,
}

/// 销售发货
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct SalesDelivery {
    pub id: ID,
    pub code: String,
    pub order_id: ID,
    pub customer_id: ID,
    pub delivery_date: chrono::NaiveDate,
    pub status: i16, // SalesDeliveryStatus（生命周期状态）
    pub remark: Option<String>,
}

/// 销售发货行
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct SalesDeliveryLine {
    pub id: ID,
    pub delivery_id: ID,
    pub order_line_id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
    pub batch_number: Option<String>,
}

/// 销售退货
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SalesReturn {
    pub id: ID,
    pub code: String,
    pub order_id: ID,
    pub customer_id: ID,
    pub return_date: chrono::NaiveDate,
    pub status: i16, // SalesReturnStatus（生命周期状态）
    pub reason: Option<String>,
    pub remark: Option<String>,
}

/// 销售退货行
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SalesReturnLine {
    pub id: ID,
    pub return_id: ID,
    pub delivery_line_id: ID,
    pub item_id: ID,
    pub quantity: i64,
    pub reason: Option<String>,
}

/// 销售发票
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SalesInvoice {
    pub id: ID,
    pub code: String,
    pub order_id: ID,
    pub customer_id: ID,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<chrono::NaiveDate>,
    pub amount: i64,
    pub tax_amount: i64,
    pub total_amount: i64,
    pub status: i16, // SalesInvoiceStatus（生命周期状态）
    pub remark: Option<String>,
}
