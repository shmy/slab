use thiserror::Error;

#[derive(Debug, Error)]
pub enum WarehouseError {
    #[error("warehouse_not_found")]
    NotFound,
    #[error("warehouse_code_duplicate")]
    CodeDuplicate,
    #[error("inventory_not_found")]
    InventoryNotFound,
    #[error("insufficient_inventory_for_operation")]
    InsufficientInventory,
}
