use item_contract::entity::{Item, ItemType};
use item_contract::error::ItemError;
use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::endpoint::item_update::UpdateItemRequest;

pub struct ItemRepository;

impl ItemRepository {
    pub async fn generate_code(conn: &mut PgConnection, item_type: ItemType) -> Result<String> {
        let (prefix, seq_val) = match item_type {
            ItemType::RawMaterial => {
                let s = sqlx::query_scalar!("SELECT nextval('seq_item_raw')")
                    .fetch_one(&mut *conn)
                    .await?
                    .unwrap_or(0);
                ("RAW", s)
            }
            ItemType::MadeInHouse => {
                let s = sqlx::query_scalar!("SELECT nextval('seq_item_mft')")
                    .fetch_one(&mut *conn)
                    .await?
                    .unwrap_or(0);
                ("MFT", s)
            }
            ItemType::Purchased => {
                let s = sqlx::query_scalar!("SELECT nextval('seq_item_pur')")
                    .fetch_one(&mut *conn)
                    .await?
                    .unwrap_or(0);
                ("PUR", s)
            }
            ItemType::SemiFinished => {
                let s = sqlx::query_scalar!("SELECT nextval('seq_item_sub')")
                    .fetch_one(&mut *conn)
                    .await?
                    .unwrap_or(0);
                ("SUB", s)
            }
            ItemType::FinishedGood => {
                let s = sqlx::query_scalar!("SELECT nextval('seq_item_prd')")
                    .fetch_one(&mut *conn)
                    .await?
                    .unwrap_or(0);
                ("PRD", s)
            }
            ItemType::Packaging => {
                let s = sqlx::query_scalar!("SELECT nextval('seq_item_pkg')")
                    .fetch_one(&mut *conn)
                    .await?
                    .unwrap_or(0);
                ("PKG", s)
            }
            ItemType::Consumable => {
                let s = sqlx::query_scalar!("SELECT nextval('seq_item_con')")
                    .fetch_one(&mut *conn)
                    .await?
                    .unwrap_or(0);
                ("CON", s)
            }
        };
        Ok(format!("{}-{:06}", prefix, seq_val))
    }

    pub async fn create(conn: &mut PgConnection, item: &Item) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO items (id, code, name, category_id, item_type, base_unit, parent_item_id, spec, is_active, version, reorder_point, safety_stock)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
            &*item.id,
            item.code,
            item.name,
            &*item.category_id,
            item.item_type as i16,
            item.base_unit,
            item.parent_item_id.map(|v| *v),
            item.spec,
            item.is_active,
            item.version,
            item.reorder_point,
            item.safety_stock,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    pub async fn update(
        conn: &mut PgConnection,
        id: &ID,
        request: &UpdateItemRequest,
    ) -> Result<bool> {
        let current = sqlx::query!(
            r#"SELECT name, category_id, base_unit, parent_item_id, spec, is_active,
                      reorder_point, safety_stock
               FROM items WHERE id = $1"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ItemError::NotFound)?;

        let name = request.name.as_deref().unwrap_or(&current.name);
        let category_id: Option<i64> = request
            .category_id
            .map(|v| Some(*v))
            .unwrap_or(current.category_id);
        let base_unit = request.base_unit.as_deref().unwrap_or(&current.base_unit);
        let parent_item_id: Option<i64> = match &request.parent_item_id {
            Some(Some(v)) => Some(**v),
            Some(None) => None,
            None => current.parent_item_id,
        };
        let spec: Option<&str> = match &request.spec {
            Some(Some(v)) => Some(v.as_str()),
            Some(None) => None,
            None => current.spec.as_deref(),
        };
        let is_active = request.is_active.unwrap_or(current.is_active);
        let reorder_point = request.reorder_point.unwrap_or(current.reorder_point);
        let safety_stock = request.safety_stock.unwrap_or(current.safety_stock);

        sqlx::query!(
            r#"UPDATE items SET name = $1, category_id = $2, base_unit = $3,
                parent_item_id = $4, spec = $5, is_active = $6,
                reorder_point = $7, safety_stock = $8
                WHERE id = $9"#,
            name,
            category_id,
            base_unit,
            parent_item_id,
            spec,
            is_active,
            reorder_point,
            safety_stock,
            id as _
        )
        .execute(&mut *conn)
        .await?;
        Ok(true)
    }

    pub async fn delete(conn: &mut PgConnection, id: &ID) -> Result<bool> {
        let affected = sqlx::query!("UPDATE items SET is_active = FALSE WHERE id = $1", id as _)
            .execute(&mut *conn)
            .await?;
        Ok(affected.rows_affected() > 0)
    }
}
