use shared_contract::value_object::id::ID;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct WorkOrder {
    pub id: ID,
    pub code: String,
    pub bom_id: ID,
    pub item_id: ID,
    pub planned_qty: i64,
    pub completed_qty: i64,
    pub scrap_qty: i64,
    pub status: i16, // WorkOrderStatus（生命周期状态）
    pub due_date: Option<chrono::NaiveDate>,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct WorkOrderMaterial {
    pub id: ID,
    pub work_order_id: ID,
    pub item_id: ID,
    pub required_qty: i64,
    pub picked_qty: i64,
    pub warehouse_id: Option<ID>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct WorkOrderOperation {
    pub id: ID,
    pub work_order_id: ID,
    pub name: String,
    pub sequence: i16,
    pub planned_qty: i64,
    pub completed_qty: i64,
    pub scrap_qty: i64,
    pub status: i16, // WorkOrderOperationStatus（生命周期状态）
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ProductionReceipt {
    pub id: ID,
    pub code: String,
    pub work_order_id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
    pub batch_number: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ScrapRecord {
    pub id: ID,
    pub code: String,
    pub work_order_id: Option<ID>,
    pub operation_id: Option<ID>,
    pub item_id: ID,
    pub quantity: i64,
    pub reason: Option<String>,
    pub severity: i16,
}
