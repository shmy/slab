use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use inventory_ledger::InventoryLedger;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use warehouse_contract::entity::Inventory;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct InitializeInventoryRequest {
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct InitializeInventoryResponse {
    pub id: ID,
}

#[utoipa::path(
    post, path = "/api/v1/inventories/initial",
    operation_id = "inventory_initial", tag = "inventory",
    request_body = InitializeInventoryRequest,
    responses((status = 200, body = JsonResponse<InitializeInventoryResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<InitializeInventoryRequest>,
) -> JsonResponseType<InitializeInventoryResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: InitializeInventoryRequest,
) -> rootcause::Result<InitializeInventoryResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 变更历史：锁读现有库存行作为 before（首次建账时为 None）
    let before = sqlx::query!(
        r#"SELECT id, item_id, warehouse_id, quantity, locked_qty, version
           FROM inventories
           WHERE item_id = $1 AND warehouse_id = $2 FOR UPDATE"#,
        &*request.item_id,
        &*request.warehouse_id,
    )
    .fetch_optional(&mut *txn)
    .await?
    .map(|r| Inventory {
        id: ID::new_unchecked(r.id),
        item_id: ID::new_unchecked(r.item_id),
        warehouse_id: ID::new_unchecked(r.warehouse_id),
        quantity: r.quantity,
        locked_qty: r.locked_qty,
        version: r.version,
    });

    InventoryLedger::adjust(
        &mut txn,
        &request.item_id,
        &request.warehouse_id,
        request.quantity,
        "inventory_initial",
        &id,
    )
    .await?;

    // 写后同事务读回整行作为 after（adjust 无变化时不产生写，此时无记录）
    let after = sqlx::query!(
        r#"SELECT id, item_id, warehouse_id, quantity, locked_qty, version
           FROM inventories
           WHERE item_id = $1 AND warehouse_id = $2"#,
        &*request.item_id,
        &*request.warehouse_id,
    )
    .fetch_optional(&mut *txn)
    .await?
    .map(|r| Inventory {
        id: ID::new_unchecked(r.id),
        item_id: ID::new_unchecked(r.item_id),
        warehouse_id: ID::new_unchecked(r.warehouse_id),
        quantity: r.quantity,
        locked_qty: r.locked_qty,
        version: r.version,
    });

    match (before, after) {
        (None, Some(after)) => {
            AuditService::record_create(&mut txn, "inventory", &after.id, &ctx, &after).await?;
        }
        (Some(before), Some(after)) => {
            AuditService::record_updated(&mut txn, "inventory", &after.id, &ctx, &before, &after)
                .await?;
        }
        _ => {} // 无变化（如对不存在的行初始化 0 数量）：不产生变更记录
    }

    txn.commit().await?;
    Ok(InitializeInventoryResponse { id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_initial_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let wh = tests::insert_test_warehouse(&state.pg_pool, "INV-I").await;
        let item = tests::insert_test_item(&state.pg_pool, "INV-IT").await;
        let request = InitializeInventoryRequest {
            item_id: item,
            warehouse_id: wh,
            quantity: 500,
        };
        let response = execute(&state.pg_pool, tests::test_operator_context(), request)
            .await
            .unwrap();
        assert!(i64::from(response.id) > 0);

        // 变更历史：首次建账 → create 类型，before 为空，after 数量为初始化值
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let inv_row = sqlx::query!(
            r#"SELECT id FROM inventories WHERE item_id = $1 AND warehouse_id = $2"#,
            &*item,
            &*wh
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            inv_row.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["quantity"], 500);
        assert_eq!(after["item_id"], item.to_string());
        assert_eq!(after["warehouse_id"], wh.to_string());
    }

    #[sqlx::test]
    async fn test_initial_existing_updates(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let wh = tests::insert_test_warehouse(&state.pg_pool, "INV-U").await;
        let item = tests::insert_test_item(&state.pg_pool, "INV-IT2").await;
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let inv_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version)
               VALUES ($1, $2, $3, 100, 0, 1)"#,
            &*inv_id,
            &*item,
            &*wh,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let request = InitializeInventoryRequest {
            item_id: item,
            warehouse_id: wh,
            quantity: 300,
        };
        execute(&state.pg_pool, tests::test_operator_context(), request)
            .await
            .unwrap();

        // 已存在库存行 → update 类型，before/after 数量正确
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *inv_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["quantity"], 100);
        assert_eq!(after["quantity"], 300);
    }
}
