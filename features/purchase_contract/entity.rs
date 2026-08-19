use shared_contract::value_object::id::ID;

/// 采购订单
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PurchaseOrder {
    pub id: ID,
    pub code: String,
    pub supplier_id: ID,
    pub status: i16, // PurchaseOrderStatus（审批流状态）
    pub order_date: chrono::NaiveDate,
    pub expected_delivery_date: Option<chrono::NaiveDate>,
    pub currency: String,
    pub total_amount: i64,
    pub payment_terms: Option<String>,
    pub remark: Option<String>,
    pub created_by: Option<ID>,
}

/// 采购订单行
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PurchaseOrderLine {
    pub id: ID,
    pub order_id: ID,
    pub line_no: i16,
    pub item_id: ID,
    pub quantity: i64,
    pub unit: String,
    pub unit_price: i64,
    pub line_total: i64,
    pub received_qty: i64,
    pub returned_qty: i64,
    pub closed: bool,
    pub remark: Option<String>,
}

/// 采购收货
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PurchaseReceipt {
    pub id: ID,
    pub code: String,
    pub order_id: ID,
    pub supplier_id: ID,
    pub receipt_date: chrono::NaiveDate,
    pub status: i16, // PurchaseReceiptStatus（生命周期状态）
    pub remark: Option<String>,
}

/// 采购收货行
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PurchaseReceiptLine {
    pub id: ID,
    pub receipt_id: ID,
    pub order_line_id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
    pub batch_number: Option<String>,
    pub unit_cost: i64,
}

/// 采购退货
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PurchaseReturn {
    pub id: ID,
    pub code: String,
    pub order_id: ID,
    pub supplier_id: ID,
    pub return_date: chrono::NaiveDate,
    pub status: i16, // PurchaseReturnStatus（审批流状态）
    pub reason: Option<String>,
    pub remark: Option<String>,
}

/// 采购退货行
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PurchaseReturnLine {
    pub id: ID,
    pub return_id: ID,
    pub receipt_line_id: ID,
    pub item_id: ID,
    pub quantity: i64,
    pub reason: Option<String>,
}

/// 采购发票
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PurchaseInvoice {
    pub id: ID,
    pub code: String,
    pub order_id: ID,
    pub supplier_id: ID,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<chrono::NaiveDate>,
    pub amount: i64,
    pub tax_amount: i64,
    pub total_amount: i64,
    pub status: i16, // PurchaseInvoiceStatus（生命周期状态）
    pub remark: Option<String>,
}
