use shared_contract::value_object::id::ID;

/// 付款/收款记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Payment {
    pub id: ID,
    pub code: String,
    pub payment_type: i16,    // 1=AR(收款) 2=AP(付款)
    pub invoice_type: String, // 'purchase_invoice' / 'sales_invoice'
    pub invoice_id: ID,
    pub amount: i64, // 分
    pub payment_date: chrono::NaiveDate,
    pub payment_method: Option<String>,
    pub remark: Option<String>,
}
