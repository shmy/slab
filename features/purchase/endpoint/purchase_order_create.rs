use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use purchase_contract::port::PurchasePort;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateOrderLine {
    pub item_id: ID,
    pub quantity: i64,
    pub unit: String,
    pub unit_price: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreatePurchaseOrderRequest {
    pub supplier_id: ID,
    pub expected_delivery_date: Option<chrono::NaiveDate>,
    pub payment_terms: Option<String>,
    pub remark: Option<String>,
    pub lines: Vec<CreateOrderLine>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreatePurchaseOrderResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-orders",
    operation_id = "purchase_order_create",
    tag = "purchase-order",
    request_body = CreatePurchaseOrderRequest,
    responses((status = 200, body = JsonResponse<CreatePurchaseOrderResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreatePurchaseOrderRequest>,
) -> JsonResponseType<CreatePurchaseOrderResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreatePurchaseOrderRequest,
) -> rootcause::Result<CreatePurchaseOrderResponse> {
    if request.lines.is_empty() {
        return Err(purchase_contract::error::PurchaseError::EmptyOrder.into());
    }

    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let code = DocNumberer::next_number(&mut txn, "seq_purchase_order", "PO").await?;

    let mut total_amount: i64 = 0;
    let mut lines: Vec<(ID, &CreateOrderLine)> = Vec::new();
    for line in request.lines.iter() {
        let line_id = ID::new();
        let line_total = line.quantity * line.unit_price;
        total_amount += line_total;
        lines.push((line_id, line));
    }

    sqlx::query!(
        r#"INSERT INTO purchase_orders
               (id, code, supplier_id, status, expected_delivery_date,
                payment_terms, remark, total_amount)
           VALUES ($1, $2, $3, 0, $4, $5, $6, $7)"#,
        &*id,
        code,
        &*request.supplier_id,
        request.expected_delivery_date,
        request.payment_terms,
        request.remark,
        total_amount,
    )
    .execute(&mut *txn)
    .await?;

    for (line_no, (line_id, line)) in lines.into_iter().enumerate() {
        let line_total = line.quantity * line.unit_price;
        sqlx::query!(
            r#"INSERT INTO purchase_order_lines
                   (id, order_id, line_no, item_id, quantity, unit,
                    unit_price, line_total, remark)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            &*line_id,
            &*id,
            line_no as i16 + 1,
            &*line.item_id,
            line.quantity,
            line.unit,
            line.unit_price,
            line_total,
            line.remark,
        )
        .execute(&mut *txn)
        .await?;
    }

    // 变更历史：同事务读回写入后的订单作为快照（order_date / currency 为库内默认值）
    let order = PurchasePort::order_by_id(&mut txn, &id).await?;
    AuditService::record_create(&mut txn, "purchase_order", &id, &ctx, &order).await?;

    txn.commit().await?;
    Ok(CreatePurchaseOrderResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;
    use shared_contract::value_object::id::ID;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let supplier_id = ID::new();
        let item_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, 'S-POC1', 'Test', true)",
            &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO items (id, code, name, item_type, base_unit) VALUES ($1, 'I-POC1', 'T', 1, 'kg')",
            &*item_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let resp = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            CreatePurchaseOrderRequest {
                supplier_id,
                expected_delivery_date: None,
                payment_terms: None,
                remark: None,
                lines: vec![CreateOrderLine {
                    item_id,
                    quantity: 10,
                    unit: "kg".to_string(),
                    unit_price: 100,
                    remark: None,
                }],
            },
        )
        .await
        .unwrap();
        assert!(i64::from(resp.id) > 0);
        assert!(resp.code.starts_with("PO"));

        // 变更历史：create 类型，before 为空
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["status"], 0);
        assert_eq!(after["total_amount"], 1000);
    }
}
