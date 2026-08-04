use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::entity::{Item, ItemType};

pub struct ItemPort;

impl ItemPort {
    pub async fn by_id(conn: &mut PgConnection, id: &ID) -> Result<Option<Item>> {
        let row = sqlx::query!(
            r#"SELECT id, code, name, category_id, item_type, base_unit, parent_item_id,
                      spec, is_active, reorder_point, safety_stock, version
               FROM items WHERE id = $1"#,
            id as _
        )
        .fetch_optional(conn)
        .await?;
        Ok(row.map(|r| Item {
            id: ID::new_unchecked(r.id),
            code: r.code,
            name: r.name,
            category_id: ID::new_unchecked(r.category_id.unwrap_or(0)),
            item_type: match r.item_type {
                1 => ItemType::RawMaterial,
                2 => ItemType::MadeInHouse,
                3 => ItemType::Purchased,
                4 => ItemType::SemiFinished,
                5 => ItemType::FinishedGood,
                6 => ItemType::Packaging,
                _ => ItemType::Consumable,
            },
            base_unit: r.base_unit,
            parent_item_id: r.parent_item_id.map(ID::new_unchecked),
            spec: r.spec,
            is_active: r.is_active,
            reorder_point: r.reorder_point,
            safety_stock: r.safety_stock,
            version: r.version,
        }))
    }

    pub async fn by_code(conn: &mut PgConnection, code: &str) -> Result<Option<Item>> {
        let row = sqlx::query!(
            r#"SELECT id, code, name, category_id, item_type, base_unit, parent_item_id,
                      spec, is_active, reorder_point, safety_stock, version
               FROM items WHERE code = $1"#,
            code
        )
        .fetch_optional(conn)
        .await?;
        Ok(row.map(|r| Item {
            id: ID::new_unchecked(r.id),
            code: r.code,
            name: r.name,
            category_id: ID::new_unchecked(r.category_id.unwrap_or(0)),
            item_type: match r.item_type {
                1 => ItemType::RawMaterial,
                2 => ItemType::MadeInHouse,
                3 => ItemType::Purchased,
                4 => ItemType::SemiFinished,
                5 => ItemType::FinishedGood,
                6 => ItemType::Packaging,
                _ => ItemType::Consumable,
            },
            base_unit: r.base_unit,
            parent_item_id: r.parent_item_id.map(ID::new_unchecked),
            spec: r.spec,
            is_active: r.is_active,
            reorder_point: r.reorder_point,
            safety_stock: r.safety_stock,
            version: r.version,
        }))
    }

    pub async fn exists_by_category(conn: &mut PgConnection, category_id: &ID) -> Result<bool> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM items WHERE category_id = $1"#,
            category_id as _
        )
        .fetch_one(conn)
        .await?;
        Ok(count.unwrap_or(0) > 0)
    }
}
