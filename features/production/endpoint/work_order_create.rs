use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use production_contract::entity::WorkOrder;
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
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateWorkOrderRequest>,
) -> JsonResponseType<CreateWorkOrderResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateWorkOrderRequest,
) -> rootcause::Result<CreateWorkOrderResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let code = DocNumberer::next_number(&mut conn, "seq_work_order", "MO").await?;

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

    // 变更历史：创建工单（同事务写，回滚即消失）
    let work_order = sqlx::query_as!(
        WorkOrder,
        r#"SELECT
               id as "id: ID",
               code,
               bom_id as "bom_id: ID",
               item_id as "item_id: ID",
               planned_qty,
               completed_qty as "completed_qty!",
               scrap_qty as "scrap_qty!",
               status,
               due_date,
               remark
           FROM work_orders
           WHERE id = $1"#,
        &*id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_create(&mut txn, "work_order", &id, &ctx, &work_order).await?;

    txn.commit().await?;
    Ok(CreateWorkOrderResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use production_contract::value_object::WorkOrderStatus;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-WO-CR-1").await;
        let bom_id = tests::insert_test_bom(&state.pg_pool, "BOM-WO-CR-1", &item_id).await;

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CreateWorkOrderRequest {
                bom_id,
                item_id,
                planned_qty: 10,
                due_date: None,
                remark: Some("audit test".to_string()),
                operations: vec![OperationInput {
                    name: "OP10".to_string(),
                    sequence: 10,
                }],
            },
        )
        .await
        .unwrap();
        assert!(response.code.starts_with("MO-"));

        // 变更历史：create 类型，entity = work_order
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT entity, action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *response.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.entity, "work_order");
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["code"].as_str(), Some(response.code.as_str()));
        assert_eq!(after["status"], WorkOrderStatus::Draft as i16);
        assert_eq!(after["planned_qty"], 10);
    }
}
