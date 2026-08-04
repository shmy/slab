//! 发布 BOM：草稿 → 已发布（status 0 → 1）。
//!
//! MRP 净需求计算只纳入已发布（status = 1）的 BOM；发布是不可逆的
//! 状态前置，供规划域使用。

use axum::extract::State;
use db::PgPool;
use product_contract::error::ProductError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct BomActionPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct BomActionResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/boms/{id}/release",
    operation_id = "bom_release", tag = "bom",
    params(BomActionPath),
    responses((status = 200, body = JsonResponse<BomActionResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<BomActionPath>,
) -> JsonResponseType<BomActionResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool, path: BomActionPath) -> rootcause::Result<BomActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let bom = sqlx::query!("SELECT status FROM boms WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ProductError::BomNotFound)?;
    if bom.status != 0 {
        return Err(ProductError::InvalidStatus.into());
    }
    sqlx::query!("UPDATE boms SET status = 1 WHERE id = $1", &*path.id)
        .execute(&mut *conn)
        .await?;
    Ok(BomActionResponse { success: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_release_draft_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let item_id = tests::insert_test_item(&pool, "BOM-R-1").await;
        let bom_id = tests::insert_test_bom(&pool, "BOM-R-1", &item_id).await;

        let resp = execute(&pool, BomActionPath { id: bom_id }).await.unwrap();
        assert!(resp.success);

        let status = sqlx::query_scalar!("SELECT status FROM boms WHERE id = $1", &*bom_id)
            .fetch_one(&mut *pool.acquire().await.unwrap())
            .await
            .unwrap();
        assert_eq!(status, 1);
    }

    #[sqlx::test]
    async fn test_release_already_released_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let item_id = tests::insert_test_item(&pool, "BOM-R-2").await;
        let bom_id = tests::insert_test_bom(&pool, "BOM-R-2", &item_id).await;

        // 先发布一次
        execute(&pool, BomActionPath { id: bom_id }).await.unwrap();
        let err = execute(&pool, BomActionPath { id: bom_id })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_release_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");

        let err = execute(&pool, BomActionPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("bom_not_found"));
    }
}
