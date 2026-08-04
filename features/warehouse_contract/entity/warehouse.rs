use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct Warehouse {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub r#type: WarehouseType,
    pub is_active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize_repr, Deserialize_repr, ToSchema)]
#[repr(i16)]
pub enum WarehouseType {
    RawMaterial = 1,
    SemiFinished = 2,
    FinishedGood = 3,
    Packaging = 4,
    Consumable = 5,
}
