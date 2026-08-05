use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use warehouse_contract::port::WarehousePort;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::warehouse_repository::WarehouseRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteWarehousePath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteWarehouseResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete,
    path = "/api/v1/warehouses/{id}",
    operation_id = "warehouse_delete",
    tag = "warehouse",
    params(DeleteWarehousePath),
    responses((status = 200, body = JsonResponse<DeleteWarehouseResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<DeleteWarehousePath>,
) -> JsonResponseType<DeleteWarehouseResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: DeleteWarehousePath,
) -> rootcause::Result<DeleteWarehouseResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 删除前读旧值用于变更历史；仓库不存在时幂等删除，不产生记录
    let before = WarehousePort::by_id(&mut txn, &path.id).await?;
    let deleted = WarehouseRepository::delete(&mut txn, &path.id).await?;
    if let Some(before) = before {
        AuditService::record_deleted(&mut txn, "warehouse", &path.id, &ctx, &before).await?;
    }
    txn.commit().await?;
    Ok(DeleteWarehouseResponse { deleted })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_delete_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let warehouse_id = tests::insert_test_warehouse(&state.pg_pool, "W-DEL").await;
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteWarehousePath { id: warehouse_id },
        )
        .await
        .unwrap();
        assert!(response.deleted);

        // 变更历史：delete 类型，after 为空
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *warehouse_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 3); // Deleted
        let before: serde_json::Value = audit_row.before.unwrap();
        assert_eq!(before["code"], "W-DEL");
        assert!(audit_row.after.is_none());
    }
}
