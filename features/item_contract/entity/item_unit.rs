use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ItemUnit {
    pub id: ID,
    pub item_id: ID,
    pub unit: String,
    /// 换算率 (基本单位 × rate = 换算单位)，单位 1/1000000
    pub rate: i64,
}
