use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct Item {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub category_id: ID,
    pub item_type: ItemType,
    pub base_unit: String,
    pub parent_item_id: Option<ID>,
    pub spec: Option<String>,
    pub is_active: bool,
    pub reorder_point: i64,
    pub safety_stock: i64,
    pub version: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize_repr, Deserialize_repr, ToSchema)]
#[repr(i16)]
pub enum ItemType {
    RawMaterial = 1,
    MadeInHouse = 2,
    Purchased = 3,
    SemiFinished = 4,
    FinishedGood = 5,
    Packaging = 6,
    Consumable = 7,
}
