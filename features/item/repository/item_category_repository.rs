use item_contract::entity::ItemCategory;
use item_contract::error::ItemError;
use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::endpoint::item_category_update::UpdateCategoryRequest;

pub struct ItemCategoryRepository;

impl ItemCategoryRepository {
    pub async fn create(conn: &mut PgConnection, category: &ItemCategory) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO item_categories (id, name, parent_id, sort_order, is_active)
               VALUES ($1, $2, $3, $4, $5)"#,
            &*category.id,
            category.name,
            category.parent_id.map(|v| *v),
            category.sort_order,
            category.is_active,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    pub async fn update(
        conn: &mut PgConnection,
        id: &ID,
        request: &UpdateCategoryRequest,
    ) -> Result<bool> {
        if let Some(name) = &request.name {
            sqlx::query!(
                "UPDATE item_categories SET name = $1 WHERE id = $2",
                name,
                id as _
            )
            .execute(&mut *conn)
            .await?;
        }
        // Option<Option<ID>>: None=not_provided, Some(None)=set_null, Some(Some(v))=set_value
        match &request.parent_id {
            Some(Some(parent_id)) => {
                sqlx::query!(
                    "UPDATE item_categories SET parent_id = $1 WHERE id = $2",
                    parent_id as _,
                    id as _
                )
                .execute(&mut *conn)
                .await?;
            }
            Some(None) => {
                sqlx::query!(
                    "UPDATE item_categories SET parent_id = NULL WHERE id = $1",
                    id as _
                )
                .execute(&mut *conn)
                .await?;
            }
            None => {}
        }
        if let Some(sort) = request.sort_order {
            sqlx::query!(
                "UPDATE item_categories SET sort_order = $1 WHERE id = $2",
                sort,
                id as _
            )
            .execute(&mut *conn)
            .await?;
        }
        Ok(true)
    }

    pub async fn delete(conn: &mut PgConnection, id: &ID) -> Result<bool> {
        let child_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM item_categories WHERE parent_id = $1",
            id as _
        )
        .fetch_one(&mut *conn)
        .await?;
        if child_count.unwrap_or(0) > 0 {
            return Err(ItemError::CategoryNotEmpty.into());
        }
        let affected = sqlx::query!("DELETE FROM item_categories WHERE id = $1", id as _)
            .execute(&mut *conn)
            .await?;
        Ok(affected.rows_affected() > 0)
    }
}
