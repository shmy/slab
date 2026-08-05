use audit_contract::AuditService;
use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use warehouse_contract::entity::{Warehouse, WarehouseType};
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::warehouse_repository::WarehouseRepository;

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateWarehouseRequest {
    pub name: String,
    pub r#type: WarehouseType,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateWarehouseResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post, path = "/api/v1/warehouses", operation_id = "warehouse_create", tag = "warehouse",
    request_body = CreateWarehouseRequest,
    responses((status = 200, body = JsonResponse<CreateWarehouseResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateWarehouseRequest>,
) -> JsonResponseType<CreateWarehouseResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateWarehouseRequest,
) -> rootcause::Result<CreateWarehouseResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let seq = CodeGen::next_seq(txn.as_mut(), "seq_warehouse").await?;
    let code = format!("WH-{:03}", seq);
    let warehouse = Warehouse {
        id,
        code: code.clone(),
        name: request.name,
        r#type: request.r#type,
        is_active: true,
    };
    WarehouseRepository::create(txn.as_mut(), &warehouse).await?;
    AuditService::record_create(&mut txn, "warehouse", &id, &ctx, &warehouse).await?;
    txn.commit().await?;
    Ok(CreateWarehouseResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let request = CreateWarehouseRequest {
            name: "Raw Material Warehouse".into(),
            r#type: WarehouseType::RawMaterial,
        };
        let response = execute(&state.pg_pool, tests::test_operator_context(), request)
            .await
            .unwrap();
        assert!(i64::from(response.id) > 0);
        assert!(response.code.starts_with("WH-"));

        // 变更历史：create 类型，before 为空，快照含名称 / 类型
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *response.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["name"], "Raw Material Warehouse");
        assert_eq!(after["type"], 1);
    }
}
