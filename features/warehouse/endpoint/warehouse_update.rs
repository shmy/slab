use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use warehouse_contract::entity::WarehouseType;
use warehouse_contract::error::WarehouseError;
use warehouse_contract::port::WarehousePort;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateWarehousePath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateWarehouseRequest {
    pub name: Option<String>,
    pub r#type: Option<WarehouseType>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UpdateWarehouseResponse {
    pub updated: bool,
}

#[utoipa::path(
    patch, path = "/api/v1/warehouses/{id}", operation_id = "warehouse_update", tag = "warehouse",
    params(UpdateWarehousePath), request_body = UpdateWarehouseRequest,
    responses((status = 200, body = JsonResponse<UpdateWarehouseResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<UpdateWarehousePath>,
    ValidJson(request): ValidJson<UpdateWarehouseRequest>,
) -> JsonResponseType<UpdateWarehouseResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: UpdateWarehousePath,
    request: UpdateWarehouseRequest,
) -> rootcause::Result<UpdateWarehouseResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    // 写前读整行作为变更历史 before 快照
    let before = WarehousePort::by_id(&mut txn, &path.id)
        .await?
        .ok_or(WarehouseError::NotFound)?;

    let name = request.name.unwrap_or_else(|| before.name.clone());
    let wh_type = request
        .r#type
        .map(|t| t as i16)
        .unwrap_or(before.r#type as i16);
    let is_active = request.is_active.unwrap_or(before.is_active);

    sqlx::query!(
        r#"UPDATE warehouses SET name = $1, type = $2, is_active = $3 WHERE id = $4"#,
        name,
        wh_type,
        is_active,
        &*path.id
    )
    .execute(txn.as_mut())
    .await?;

    // 写后同事务读回整行作为 after 快照
    let after = WarehousePort::by_id(&mut txn, &path.id)
        .await?
        .ok_or(WarehouseError::NotFound)?;
    AuditService::record_updated(&mut txn, "warehouse", &path.id, &ctx, &before, &after).await?;

    txn.commit().await?;
    Ok(UpdateWarehouseResponse { updated: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_update_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let warehouse_id = tests::insert_test_warehouse(&state.pg_pool, "W-UPD").await;
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            UpdateWarehousePath { id: warehouse_id },
            UpdateWarehouseRequest {
                name: Some("Updated Warehouse".into()),
                r#type: None,
                is_active: None,
            },
        )
        .await
        .unwrap();
        assert!(response.updated);

        // 变更历史：update 类型，before/after 快照字段正确
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *warehouse_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["name"], "TestWH");
        assert_eq!(after["name"], "Updated Warehouse");
        assert_eq!(after["type"], 3);
        assert_eq!(before["code"], "W-UPD");
    }
}
