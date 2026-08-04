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
pub(crate) struct OperationInput {
    pub name: String,
    pub sequence: i16,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateWorkOrderRequest {
    pub bom_id: ID,
    pub item_id: ID,
    pub planned_qty: i64,
    pub due_date: Option<chrono::NaiveDate>,
    pub remark: Option<String>,
    pub operations: Vec<OperationInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateWorkOrderResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(post, path = "/api/v1/work-orders",
    operation_id = "work_order_create", tag = "work-order",
    request_body = CreateWorkOrderRequest,
    responses((status = 200, body = JsonResponse<CreateWorkOrderResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateWorkOrderRequest>,
) -> JsonResponseType<CreateWorkOrderResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateWorkOrderRequest,
) -> rootcause::Result<CreateWorkOrderResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let code = CodeGen::next_code(&mut conn, "seq_work_order", "MO").await?;

    let mut txn = conn.begin().await?;

    sqlx::query!(
        r#"INSERT INTO work_orders (id, code, bom_id, item_id, planned_qty, due_date, remark, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, 0)"#,
        &*id, code, &*request.bom_id, &*request.item_id,
        request.planned_qty, request.due_date, request.remark,
    ).execute(&mut *txn).await?;

    for op in &request.operations {
        let op_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO work_order_operations (id, work_order_id, name, sequence, planned_qty, status)
               VALUES ($1, $2, $3, $4, $5, 0)"#,
            &*op_id, &*id, op.name, op.sequence, request.planned_qty,
        ).execute(&mut *txn).await?;
    }

    // BOM 展开 → 写入物料需求
    let bom_items = sqlx::query!(
        r#"SELECT item_id, quantity, unit, wastage_rate FROM bom_items WHERE bom_id = $1"#,
        &*request.bom_id
    )
    .fetch_all(&mut *txn)
    .await?;

    for bi in bom_items {
        let mat_id = ID::new();
        let qty = bi.quantity * request.planned_qty;
        sqlx::query!(
            r#"INSERT INTO work_order_materials (id, work_order_id, item_id, required_qty, picked_qty)
               VALUES ($1, $2, $3, $4, 0)"#,
            &*mat_id, &*id, bi.item_id, qty,
        ).execute(&mut *txn).await?;
    }

    txn.commit().await?;
    Ok(CreateWorkOrderResponse { id, code })
}
