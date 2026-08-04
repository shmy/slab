use shared_contract::value_object::id::ID;

/// BOM
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Bom {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub item_id: ID,
    pub version: i32,
    pub status: i16,
    pub total_qty: i64,
    pub remark: Option<String>,
}

/// BOM 物料明细
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct BomItem {
    pub id: ID,
    pub bom_id: ID,
    pub item_id: ID,
    pub quantity: i64,
    pub unit: String,
    pub wastage_rate: i64, // 万分比: 5% = 500
    pub parent_item_id: Option<ID>,
    pub sort_order: i16,
    pub remark: Option<String>,
}

/// 模具
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Mold {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub item_id: ID,
    pub cavity_count: i32,
    pub life_expectancy: Option<i64>,
    pub life_used: Option<i64>,
    pub status: i16,
    pub maintenance_cycle: Option<i32>,
    pub remark: Option<String>,
}

/// 模具保养记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct MoldMaintenance {
    pub id: ID,
    pub mold_id: ID,
    pub r#type: i16,
    pub description: Option<String>,
    pub cost: Option<i64>,
    pub maintained_at: chrono::NaiveDate,
}
