use shared_contract::value_object::id::ID;

/// 检验模板
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct InspectionTemplate {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub category: i16, // 1=IQC 2=IPQC 3=OQC
    pub is_active: bool,
}

/// 检验模板项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct InspectionTemplateItem {
    pub id: ID,
    pub template_id: ID,
    pub name: String,
    pub specification: Option<String>,
    pub tolerance_upper: Option<String>,
    pub tolerance_lower: Option<String>,
    pub method: Option<String>,
    pub is_required: bool,
    pub sort_order: i16,
}

/// 检验单
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct InspectionOrder {
    pub id: ID,
    pub code: String,
    pub template_id: Option<ID>,
    pub source_type: String,
    pub source_id: i64,
    pub item_id: ID,
    pub lot_qty: i64,
    pub sample_qty: i64,
    pub inspector: Option<String>,
    pub result: Option<i16>, // Option<Verdict>（检验结论；NULL=待检）
    pub status: i16,         // InspectionOrderStatus（检验单状态）
    pub inspected_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 检验结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct InspectionResult {
    pub id: ID,
    pub inspection_id: ID,
    pub template_item_id: ID,
    pub result: i16, // Verdict::Pass/Fail
    pub actual_value: Option<String>,
    pub remark: Option<String>,
}

/// 不合格处理
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct NonConformance {
    pub id: ID,
    pub code: String,
    pub inspection_id: Option<ID>,
    pub item_id: ID,
    pub quantity: i64,
    pub severity: i16,
    pub disposition: Option<i16>,
    pub status: i16, // NonConformanceStatus（生命周期状态）
    pub remark: Option<String>,
}
