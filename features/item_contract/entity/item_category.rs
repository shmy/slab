use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ItemCategory {
    pub id: ID,
    pub name: String,
    pub parent_id: Option<ID>,
    pub sort_order: i32,
    pub is_active: bool,
}
