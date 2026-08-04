use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::entity::{Inventory, Warehouse, WarehouseType};

pub struct WarehousePort;

impl WarehousePort {
    pub async fn by_id(conn: &mut PgConnection, id: &ID) -> Result<Option<Warehouse>> {
        let row = sqlx::query!(
            r#"SELECT id, code, name, type, is_active FROM warehouses WHERE id = $1"#,
            id as _
        )
        .fetch_optional(conn)
        .await?;
        Ok(row.map(|r| Warehouse {
            id: ID::new_unchecked(r.id),
            code: r.code,
            name: r.name,
            r#type: match r.r#type {
                1 => WarehouseType::RawMaterial,
                2 => WarehouseType::SemiFinished,
                3 => WarehouseType::FinishedGood,
                4 => WarehouseType::Packaging,
                _ => WarehouseType::Consumable,
            },
            is_active: r.is_active,
        }))
    }

    pub async fn inventory_by_item_warehouse(
        conn: &mut PgConnection,
        item_id: &ID,
        warehouse_id: &ID,
    ) -> Result<Option<Inventory>> {
        let row = sqlx::query!(
            r#"SELECT id, item_id, warehouse_id,
                      CAST(quantity AS DOUBLE PRECISION) AS "quantity!",
                      CAST(locked_qty AS DOUBLE PRECISION) AS "locked_qty!",
                      version
               FROM inventories WHERE item_id = $1 AND warehouse_id = $2"#,
            item_id as _,
            warehouse_id as _
        )
        .fetch_optional(conn)
        .await?;
        Ok(row.map(|r| Inventory {
            id: ID::new_unchecked(r.id),
            item_id: ID::new_unchecked(r.item_id),
            warehouse_id: ID::new_unchecked(r.warehouse_id),
            quantity: (r.quantity * 1000.0) as i64,
            locked_qty: (r.locked_qty * 1000.0) as i64,
            version: r.version,
        }))
    }
}
