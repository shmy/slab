use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use product_contract::entity::Mold;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateMoldRequest {
    pub name: String,
    pub item_id: ID,
    pub cavity_count: Option<i32>,
    pub life_expectancy: Option<i64>,
    pub maintenance_cycle: Option<i32>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateMoldResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(post, path = "/api/v1/molds",
    operation_id = "mold_create", tag = "mold",
    request_body = CreateMoldRequest,
    responses((status = 200, body = JsonResponse<CreateMoldResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateMoldRequest>,
) -> JsonResponseType<CreateMoldResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateMoldRequest,
) -> rootcause::Result<CreateMoldResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let code = DocNumberer::next_number(&mut conn, "seq_bom", "MOLD").await?;

    sqlx::query!(
        r#"INSERT INTO molds (id, code, name, item_id, cavity_count, life_expectancy, maintenance_cycle, remark)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        &*id, code, request.name, &*request.item_id,
        request.cavity_count.unwrap_or(1), request.life_expectancy,
        request.maintenance_cycle, request.remark,
    ).execute(&mut *conn).await?;
    let mold: Mold = sqlx::query_as!(
        Mold,
        r#"SELECT id, code, name, item_id, cavity_count, life_expectancy, life_used, status, maintenance_cycle, remark
           FROM molds WHERE id = $1"#,
        &*id
    )
    .fetch_one(&mut *conn)
    .await?;
    AuditService::record_create(&mut conn, "mold", &id, &ctx, &mold).await?;
    Ok(CreateMoldResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_mold_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-MOLD-1").await;

        let req = CreateMoldRequest {
            name: "外壳模具".into(),
            item_id,
            cavity_count: None, // 默认 1
            life_expectancy: Some(100_000),
            maintenance_cycle: Some(5000),
            remark: None,
        };
        let resp = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("MOLD-"));

        let row = sqlx::query!(
            "SELECT cavity_count, status, life_used FROM molds WHERE id = $1",
            &*resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.cavity_count, 1);
        assert_eq!(row.status, 0);
        assert_eq!(row.life_used.unwrap_or(-1), 0);

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
        assert_eq!(after["name"], "外壳模具");
        assert_eq!(after["cavity_count"], 1);
        assert_eq!(after["status"], 0);
    }
}
