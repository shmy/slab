use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ItemCost {
    pub id: ID,
    pub item_id: ID,
    pub cost_type: CostType,
    pub unit_cost: i64,
    pub currency: String,
    pub effective_at: DateTime<Utc>,
    pub is_current: bool,
    pub remark: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize_repr, Deserialize_repr, ToSchema)]
#[repr(i16)]
pub enum CostType {
    Reference = 1,
    LatestPurchase = 2,
    Manual = 3,
    WeightedAverage = 10,
}
