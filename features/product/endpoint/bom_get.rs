use axum::extract::State;
use db::PgPool;
use product_contract::entity::{Bom, BomItem};
use product_contract::error::ProductError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetBomPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct BomDetail {
    pub bom: Bom,
    pub items: Vec<BomItem>,
}

#[utoipa::path(get, path = "/api/v1/boms/{id}", operation_id = "bom_get", tag = "bom",
    params(GetBomPath), responses((status = 200, body = JsonResponse<BomDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetBomPath>,
) -> JsonResponseType<BomDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetBomPath) -> rootcause::Result<BomDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!("SELECT id, code, name, item_id, version, status, total_qty, remark FROM boms WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn).await?.ok_or(ProductError::BomNotFound)?;
    let items = sqlx::query!("SELECT id, bom_id, item_id, quantity, unit, wastage_rate, parent_item_id, sort_order, remark FROM bom_items WHERE bom_id = $1 ORDER BY sort_order", &*path.id)
        .fetch_all(&mut *conn).await?;
    Ok(BomDetail {
        bom: Bom {
            id: ID::new_unchecked(row.id),
            code: row.code,
            name: row.name,
            item_id: ID::new_unchecked(row.item_id),
            version: row.version,
            status: row.status,
            total_qty: row.total_qty,
            remark: row.remark,
        },
        items: items
            .into_iter()
            .map(|r| BomItem {
                id: ID::new_unchecked(r.id),
                bom_id: ID::new_unchecked(r.bom_id),
                item_id: ID::new_unchecked(r.item_id),
                quantity: r.quantity,
                unit: r.unit,
                wastage_rate: r.wastage_rate.unwrap_or(0),
                parent_item_id: r.parent_item_id.map(ID::new_unchecked),
                sort_order: r.sort_order,
                remark: r.remark,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_bom_get_with_items(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-BOMG-1").await;

        let bom_id = ID::new();
        let line_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO boms (id, code, name, item_id, status, total_qty) VALUES ($1, 'BOM-GET-1', 'BOM', $2, 0, 5)",
            &*bom_id,
            &*item_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO bom_items (id, bom_id, item_id, quantity, unit, wastage_rate, sort_order) VALUES ($1, $2, $3, 3, 'kg', 0, 0)",
            &*line_id,
            &*bom_id,
            &*item_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let detail = execute(&state.pg_pool, GetBomPath { id: bom_id })
            .await
            .unwrap();
        assert_eq!(detail.bom.code, "BOM-GET-1");
        assert_eq!(detail.bom.total_qty, 5);
        assert_eq!(detail.items.len(), 1);
        assert_eq!(detail.items[0].quantity, 3);
    }

    #[sqlx::test]
    async fn test_bom_get_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(&state.pg_pool, GetBomPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("bom_not_found"));
    }
}
