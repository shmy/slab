use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateInspectionOrderRequest {
    pub template_id: ID,
    pub source_type: String,
    pub source_id: i64,
    pub item_id: ID,
    pub lot_qty: i64,
    pub sample_qty: Option<i64>,
    pub inspector: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateInspectionOrderResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/inspection-orders",
    operation_id = "inspection_order_create",
    tag = "inspection-order",
    request_body = CreateInspectionOrderRequest,
    responses((status = 200, body = JsonResponse<CreateInspectionOrderResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateInspectionOrderRequest>,
) -> JsonResponseType<CreateInspectionOrderResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateInspectionOrderRequest,
) -> rootcause::Result<CreateInspectionOrderResponse> {
    let mut conn = pg_pool.acquire().await?;
    let code = CodeGen::next_code(&mut conn, "seq_inspection_order", "IQC").await?;

    let id = ID::new();
    sqlx::query!(
        r#"INSERT INTO inspection_orders
               (id, code, template_id, source_type, source_id, item_id,
                lot_qty, sample_qty, inspector, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 0)"#,
        &*id,
        code,
        &*request.template_id,
        request.source_type,
        request.source_id,
        &*request.item_id,
        request.lot_qty,
        request.sample_qty.unwrap_or(request.lot_qty),
        request.inspector,
    )
    .execute(&mut *conn)
    .await?;

    Ok(CreateInspectionOrderResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_inspection_order_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let template_id = tests::insert_test_template(&state.pg_pool, "TPL-IQ-1").await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-IQ-1").await;

        let req = CreateInspectionOrderRequest {
            template_id,
            source_type: "purchase_receipt".into(),
            source_id: 1001,
            item_id,
            lot_qty: 100,
            sample_qty: None, // 默认等于 lot_qty
            inspector: Some("张三".into()),
        };
        let resp = execute(&state.pg_pool, req).await.unwrap();
        assert!(resp.code.starts_with("IQC-"));

        let row = sqlx::query!(
            "SELECT status, sample_qty FROM inspection_orders WHERE id = $1",
            &*resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.status, 0);
        assert_eq!(row.sample_qty, 100);
    }
}
