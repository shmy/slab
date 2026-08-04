use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct Inventory {
    pub id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    /// 可用数量，单位 1/1000
    pub quantity: i64,
    /// 锁定量，单位 1/1000
    pub locked_qty: i64,
    pub version: i64,
}
