//! 完成工单。

use axum::extract::State;
use db::PgPool;
use production_contract::error::ProductionError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct WOPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WOResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/work-orders/{id}/complete",
    operation_id = "work_order_complete", tag = "work-order",
    params(WOPath),
    responses((status = 200, body = JsonResponse<WOResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<WOPath>,
) -> JsonResponseType<WOResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: WOPath) -> rootcause::Result<WOResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 锁定读 + 状态机校验：只有 released(1) / in_progress(2) 可完成；
    // 草稿不可完成，已完成/已关闭不可重复完成
    let row = sqlx::query!(
        "SELECT status FROM work_orders WHERE id = $1 FOR UPDATE",
        &*path.id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(ProductionError::NotFound)?;
    if row.status < 1 || row.status >= 3 {
        return Err(ProductionError::InvalidStatus.into());
    }

    sqlx::query!("UPDATE work_orders SET status = 3 WHERE id = $1", &*path.id)
        .execute(&mut *txn)
        .await?;

    txn.commit().await?;
    Ok(WOResponse { success: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use migration::run_migrations;

    async fn seed_work_order(pool: &sqlx::PgPool, code: &str, status: i16) -> ID {
        let item_id = tests::insert_test_item(pool, &format!("I-{code}")).await;
        let bom_id = tests::insert_test_bom(pool, &format!("BOM-{code}"), &item_id).await;
        let id = ID::new();
        sqlx::query!(
            r#"INSERT INTO work_orders (id, code, bom_id, item_id, planned_qty, status)
               VALUES ($1, $2, $3, $4, 10, $5)"#,
            &*id,
            code,
            &*bom_id,
            &*item_id,
            status,
        )
        .execute(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        id
    }

    #[sqlx::test]
    async fn test_complete_released_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_work_order(&pool, "MO-CMP-1", 1).await;

        let resp = execute(&pool, WOPath { id }).await.unwrap();
        assert!(resp.success);

        let status = sqlx::query_scalar!("SELECT status FROM work_orders WHERE id = $1", &*id)
            .fetch_one(&mut *pool.acquire().await.unwrap())
            .await
            .unwrap();
        assert_eq!(status, 3);
    }

    #[sqlx::test]
    async fn test_complete_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_work_order(&pool, "MO-CMP-2", 0).await;

        let err = execute(&pool, WOPath { id }).await.unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));

        let status = sqlx::query_scalar!("SELECT status FROM work_orders WHERE id = $1", &*id)
            .fetch_one(&mut *pool.acquire().await.unwrap())
            .await
            .unwrap();
        assert_eq!(status, 0);
    }

    #[sqlx::test]
    async fn test_complete_already_completed_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_work_order(&pool, "MO-CMP-3", 3).await;

        let err = execute(&pool, WOPath { id }).await.unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_complete_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");

        let err = execute(&pool, WOPath { id: ID::new() }).await.unwrap_err();
        assert!(err.to_string().contains("work_order_not_found"));
    }
}
