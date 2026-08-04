//! 创建盘点单。

use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CheckItemInput {
    pub item_id: ID,
    pub actual_qty: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateCheckRequest {
    pub warehouse_id: ID,
    pub plan_date: chrono::NaiveDate,
    pub remark: Option<String>,
    pub items: Vec<CheckItemInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateCheckResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(post, path = "/api/v1/inventory-checks",
    operation_id = "inventory_check_create", tag = "inventory-check",
    request_body = CreateCheckRequest,
    responses((status = 200, body = JsonResponse<CreateCheckResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateCheckRequest>,
) -> JsonResponseType<CreateCheckResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateCheckRequest,
) -> rootcause::Result<CreateCheckResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    let code = CodeGen::next_code(&mut txn, "seq_inventory_check", "CHK").await?;

    let id = ID::new();
    sqlx::query!(
        r#"INSERT INTO inventory_checks (id, code, warehouse_id, plan_date, remark, status)
           VALUES ($1, $2, $3, $4, $5, 0)"#,
        &*id,
        code,
        &*request.warehouse_id,
        request.plan_date,
        request.remark,
    )
    .execute(&mut *txn)
    .await?;

    for item in &request.items {
        let book = sqlx::query!(
            r#"SELECT quantity FROM inventories WHERE item_id = $1 AND warehouse_id = $2"#,
            &*item.item_id,
            &*request.warehouse_id,
        )
        .fetch_optional(&mut *txn)
        .await?
        .map(|r| r.quantity)
        .unwrap_or(0);

        let diff = item.actual_qty - book;
        let line_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO inventory_check_items
                   (id, check_id, item_id, book_qty, actual_qty, diff_qty, remark)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            &*line_id,
            &*id,
            &*item.item_id,
            book,
            item.actual_qty,
            diff,
            item.remark,
        )
        .execute(&mut *txn)
        .await?;
    }

    txn.commit().await?;
    Ok(CreateCheckResponse { id, code })
}
