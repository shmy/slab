use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use quality_contract::entity::NonConformance;
use quality_contract::value_object::NonConformanceStatus;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateNCRequest {
    pub inspection_id: Option<ID>,
    pub item_id: ID,
    pub quantity: i64,
    pub severity: i16,
    pub disposition: Option<i16>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateNCResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/non-conformances",
    operation_id = "non_conformance_create",
    tag = "non-conformance",
    request_body = CreateNCRequest,
    responses((status = 200, body = JsonResponse<CreateNCResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateNCRequest>,
) -> JsonResponseType<CreateNCResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateNCRequest,
) -> rootcause::Result<CreateNCResponse> {
    let mut conn = pg_pool.acquire().await?;
    let code = DocNumberer::next_number(&mut conn, "seq_non_conformance", "NC").await?;

    let id = ID::new();
    sqlx::query!(
        r#"INSERT INTO non_conformances
               (id, code, inspection_id, item_id, quantity, severity, disposition, status, remark)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        &*id,
        code,
        request.inspection_id as _,
        &*request.item_id,
        request.quantity,
        request.severity,
        request.disposition,
        NonConformanceStatus::Open as i16,
        request.remark,
    )
    .execute(&mut *conn)
    .await?;

    // 变更历史：该端点无显式事务（单条 INSERT 自提交），审计写入与业务写共用同一连接
    let nc = sqlx::query_as!(
        NonConformance,
        r#"SELECT id AS "id: ID", code, inspection_id AS "inspection_id: ID", item_id AS "item_id: ID", quantity, severity,
                  disposition, status, remark
           FROM non_conformances WHERE id = $1"#,
        &*id,
    )
    .fetch_one(&mut *conn)
    .await?;
    AuditService::record_create(&mut conn, "non_conformance", &id, &ctx, &nc).await?;

    Ok(CreateNCResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_nc_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-NC-1").await;

        let req = CreateNCRequest {
            inspection_id: None,
            item_id,
            quantity: 5,
            severity: 2,          // major
            disposition: Some(2), // rework
            remark: Some("尺寸超差".into()),
        };
        let resp = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("NC-"));

        let row = sqlx::query!(
            "SELECT status, severity FROM non_conformances WHERE id = $1",
            &*resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.status, NonConformanceStatus::Open as i16);
        assert_eq!(row.severity, 2);

        // 变更历史：create 类型
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["code"], resp.code);
        assert_eq!(after["status"], 0);
        assert_eq!(after["severity"], 2);
    }
}
