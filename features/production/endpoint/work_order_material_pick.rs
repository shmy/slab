use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use inventory_ledger::{InventoryLedger, LedgerCommand, TransactionType};
use production_contract::entity::WorkOrder;
use production_contract::error::ProductionError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct PickPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct PickItem {
    pub material_id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct PickRequest {
    pub items: Vec<PickItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PickResponse {
    pub picked: bool,
}

#[utoipa::path(post, path = "/api/v1/work-orders/{id}/pick",
    operation_id = "work_order_pick", tag = "work-order",
    params(PickPath), request_body = PickRequest,
    responses((status = 200, body = JsonResponse<PickResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<PickPath>,
    ValidJson(request): ValidJson<PickRequest>,
) -> JsonResponseType<PickResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: PickPath,
    request: PickRequest,
) -> rootcause::Result<PickResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 变更历史：锁定读全行作为 before（状态机校验 + 写前快照）
    let before = sqlx::query_as!(
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
           WHERE id = $1
           FOR UPDATE"#,
        &*path.id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(ProductionError::NotFound)?;
    if before.status < 1 {
        return Err(ProductionError::InvalidStatus.into());
    }

    for item in &request.items {
        let mat = sqlx::query!(
            r#"SELECT required_qty, picked_qty FROM work_order_materials WHERE id = $1 AND work_order_id = $2 FOR UPDATE"#,
            &*item.material_id, &*path.id,
        ).fetch_optional(&mut *txn).await?.ok_or(ProductionError::NotFound)?;

        let current_picked = mat.picked_qty.unwrap_or(0);
        let new_picked = current_picked + item.quantity;
        if new_picked > mat.required_qty {
            return Err(ProductionError::InsufficientMaterials.into());
        }

        sqlx::query!(
            "UPDATE work_order_materials SET picked_qty = $1 WHERE id = $2",
            new_picked,
            &*item.material_id
        )
        .execute(&mut *txn)
        .await?;

        // 库存台账统一处理
        InventoryLedger::issue(
            &mut txn,
            &LedgerCommand {
                item_id: &item.item_id,
                warehouse_id: &item.warehouse_id,
                quantity: item.quantity,
                tx_type: TransactionType::MaterialPick,
                reference_type: "work_order_pick",
                reference_id: &path.id,
                batch_number: None,
            },
        )
        .await?;
    }
    // 变更历史：写后重读全行作为 after（同事务，可见自身未提交写入）
    let after = sqlx::query_as!(
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
        &*path.id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_updated(&mut txn, "work_order", &path.id, &ctx, &before, &after).await?;

    txn.commit().await?;
    Ok(PickResponse { picked: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use production_contract::value_object::WorkOrderStatus;

    #[sqlx::test]
    async fn test_pick_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        let item_id = tests::insert_test_item(&state.pg_pool, "I-WO-PK-1").await;
        let bom_id = tests::insert_test_bom(&state.pg_pool, "BOM-WO-PK-1", &item_id).await;
        let wo_id = ID::new();
        let material_id = ID::new();
        let wh_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();

        sqlx::query!(
            r#"INSERT INTO work_orders (id, code, bom_id, item_id, planned_qty, status)
               VALUES ($1, $2, $3, $4, 10, 1)"#,
            &*wo_id,
            "MO-PK-1",
            &*bom_id,
            &*item_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, 'WH-PK1', 'Main', 1, true)",
            &*wh_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            r#"INSERT INTO work_order_materials (id, work_order_id, item_id, required_qty, picked_qty, warehouse_id)
               VALUES ($1, $2, $3, 10, 0, $4)"#,
            &*material_id,
            &*wo_id,
            &*item_id,
            &*wh_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        // 领料出库需先有库存
        sqlx::query!(
            r#"INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version)
               VALUES ($1, $2, $3, 100, 0, 1)"#,
            &*ID::new(),
            &*item_id,
            &*wh_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let resp = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            PickPath { id: wo_id },
            PickRequest {
                items: vec![PickItem {
                    material_id,
                    item_id,
                    warehouse_id: wh_id,
                    quantity: 4,
                }],
            },
        )
        .await
        .unwrap();
        assert!(resp.picked);

        // 变更历史：update 类型，entity = work_order
        let audit_row = sqlx::query!(
            r#"SELECT entity, action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *wo_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.entity, "work_order");
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["status"], WorkOrderStatus::Released as i16);
        assert_eq!(after["status"], WorkOrderStatus::Released as i16);
    }

    #[sqlx::test]
    async fn test_pick_insufficient_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        let item_id = tests::insert_test_item(&state.pg_pool, "I-WO-PK-2").await;
        let bom_id = tests::insert_test_bom(&state.pg_pool, "BOM-WO-PK-2", &item_id).await;
        let wo_id = ID::new();
        let material_id = ID::new();
        let wh_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();

        sqlx::query!(
            r#"INSERT INTO work_orders (id, code, bom_id, item_id, planned_qty, status)
               VALUES ($1, $2, $3, $4, 10, 1)"#,
            &*wo_id,
            "MO-PK-2",
            &*bom_id,
            &*item_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, 'WH-PK2', 'Main', 1, true)",
            &*wh_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            r#"INSERT INTO work_order_materials (id, work_order_id, item_id, required_qty, picked_qty, warehouse_id)
               VALUES ($1, $2, $3, 10, 0, $4)"#,
            &*material_id,
            &*wo_id,
            &*item_id,
            &*wh_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        // 领料超出需求 → 拒绝，无变更历史
        let err = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            PickPath { id: wo_id },
            PickRequest {
                items: vec![PickItem {
                    material_id,
                    item_id,
                    warehouse_id: wh_id,
                    quantity: 99,
                }],
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("insufficient_materials"));

        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM audit_logs WHERE entity_id = $1",
            *wo_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(count, Some(0));
    }
}
