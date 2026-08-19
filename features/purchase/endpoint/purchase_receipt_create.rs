use audit_contract::AuditService;
use axum::extract::State;
use costing::CostCalculator;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use inventory_ledger::{InventoryLedger, LedgerCommand, TransactionType};
use purchase_contract::entity::PurchaseReceipt;
use purchase_contract::error::PurchaseError;
use purchase_contract::value_object::{PurchaseOrderStatus, PurchaseReceiptStatus};
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateReceiptLine {
    pub order_line_id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
    pub batch_number: Option<String>,
    pub unit_cost: i64,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateReceiptRequest {
    pub order_id: ID,
    pub receipt_date: Option<chrono::NaiveDate>,
    pub remark: Option<String>,
    pub lines: Vec<CreateReceiptLine>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateReceiptResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-receipts",
    operation_id = "purchase_receipt_create",
    tag = "purchase-receipt",
    request_body = CreateReceiptRequest,
    responses((status = 200, body = JsonResponse<CreateReceiptResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateReceiptRequest>,
) -> JsonResponseType<CreateReceiptResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateReceiptRequest,
) -> rootcause::Result<CreateReceiptResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 验证订单存在且已审批
    let order = sqlx::query!(
        r#"SELECT id, supplier_id, status FROM purchase_orders WHERE id = $1 FOR UPDATE"#,
        &*request.order_id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(PurchaseError::NotFound)?;

    if order.status != PurchaseOrderStatus::Approved as i16 {
        return Err(PurchaseError::OrderNotApproved.into());
    }

    // 生成编码
    let code = DocNumberer::next_number(&mut txn, "seq_purchase_receipt", "RCV").await?;

    // 收货日期缺省为当天
    let receipt_date = request
        .receipt_date
        .unwrap_or_else(shared_contract::value_object::today::today_naive);

    let receipt_id = ID::new();
    sqlx::query!(
        r#"INSERT INTO purchase_receipts (id, code, order_id, supplier_id, status, receipt_date, remark)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        &*receipt_id,
        code,
        &*request.order_id,
        &*ID::new_unchecked(order.supplier_id),
        PurchaseReceiptStatus::Posted as i16,
        receipt_date,
        request.remark,
    )
    .execute(&mut *txn)
    .await?;

    for line in &request.lines {
        let line_id = ID::new();

        // 验证 order_line 不超收
        let ol = sqlx::query!(
            r#"SELECT quantity, received_qty FROM purchase_order_lines
               WHERE id = $1 AND order_id = $2 FOR UPDATE"#,
            &*line.order_line_id,
            &*request.order_id,
        )
        .fetch_optional(&mut *txn)
        .await?
        .ok_or(PurchaseError::LineNotFound)?;

        let new_received = ol.received_qty + line.quantity;
        if new_received > ol.quantity {
            return Err(PurchaseError::OverReceipt.into());
        }

        // 插入收货行
        sqlx::query!(
            r#"INSERT INTO purchase_receipt_lines
                   (id, receipt_id, order_line_id, item_id, warehouse_id, quantity, batch_number, unit_cost)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            &*line_id,
            &*receipt_id,
            &*line.order_line_id,
            &*line.item_id,
            &*line.warehouse_id,
            line.quantity,
            line.batch_number,
            line.unit_cost,
        )
        .execute(&mut *txn)
        .await?;

        // 更新 order_line 已收量
        sqlx::query!(
            r#"UPDATE purchase_order_lines
               SET received_qty = $1, closed = ($1 >= quantity)
               WHERE id = $2"#,
            new_received,
            &*line.order_line_id,
        )
        .execute(&mut *txn)
        .await?;

        InventoryLedger::receive(
            &mut txn,
            &LedgerCommand {
                item_id: &line.item_id,
                warehouse_id: &line.warehouse_id,
                quantity: line.quantity,
                tx_type: TransactionType::Inbound,
                reference_type: "purchase_receipt",
                reference_id: &line_id,
                batch_number: line.batch_number.as_deref(),
            },
        )
        .await?;

        // 加权平均重算
        CostCalculator::recalc_weighted_average(
            txn.as_mut(),
            &line.item_id,
            line.quantity,
            line.unit_cost,
        )
        .await?;
    }

    // 变更历史：同事务读回写入后的收货单作为快照
    let receipt = sqlx::query_as!(
        PurchaseReceipt,
        r#"SELECT id, code, order_id, supplier_id, receipt_date, status, remark
           FROM purchase_receipts WHERE id = $1"#,
        &*receipt_id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_create(&mut txn, "purchase_receipt", &receipt_id, &ctx, &receipt).await?;

    txn.commit().await?;
    Ok(CreateReceiptResponse {
        id: receipt_id,
        code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::run_migrations;
    use shared_contract::value_object::id::ID;

    #[sqlx::test]
    async fn test_receipt_date_defaults_to_today(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");

        // 造一个已审批的采购订单
        let supplier_id = ID::new();
        let po_id = ID::new();
        let item_id = ID::new();
        let wh_id = ID::new();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, 'WH-RD1', 'Main', 1, true)",
            &*wh_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, 'S-RD1', 'Test', true)",
            &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO items (id, code, name, item_type, base_unit) VALUES ($1, 'I-RD1', 'T', 1, 'kg')",
            &*item_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount) VALUES ($1, 'PO-RD1', $2, 3, CURRENT_DATE, 'CNY', 100)",
            &*po_id, &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let line_id = ID::new();
        sqlx::query!(
            "INSERT INTO purchase_order_lines (id, order_id, item_id, quantity, unit, unit_price, line_total) VALUES ($1, $2, $3, 10, 'kg', 10, 100)",
            &*line_id, &*po_id, &*item_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let resp = execute(
            &pool,
            crate::tests::test_operator_context(),
            CreateReceiptRequest {
                order_id: po_id,
                receipt_date: None,
                remark: None,
                lines: vec![CreateReceiptLine {
                    order_line_id: line_id,
                    item_id,
                    warehouse_id: wh_id,
                    quantity: 10,
                    batch_number: None,
                    unit_cost: 10,
                }],
            },
        )
        .await
        .unwrap();
        assert_ne!(resp.id, ID::new());

        let mut conn = pool.acquire().await.unwrap();
        let row = sqlx::query!(
            "SELECT receipt_date FROM purchase_receipts WHERE id = $1",
            &*resp.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            row.receipt_date,
            shared_contract::value_object::today::today_naive()
        );

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
        assert_eq!(after["status"], 1);
    }
}
