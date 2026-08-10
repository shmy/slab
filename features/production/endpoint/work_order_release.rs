use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use production_contract::entity::WorkOrder;
use production_contract::error::ProductionError;
use production_contract::value_object::WorkOrderStatus;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct WOPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WOResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/work-orders/{id}/release",
    operation_id = "work_order_release", tag = "work-order",
    params(WOPath),
    responses((status = 200, body = JsonResponse<WOResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<WOPath>,
) -> JsonResponseType<WOResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: WOPath,
) -> rootcause::Result<WOResponse> {
    let mut conn = pg_pool.acquire().await?;

    // 变更历史：写前读全行作为 before（本端点无显式事务，未用 FOR UPDATE）
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
           WHERE id = $1"#,
        &*path.id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(ProductionError::NotFound)?;
    if before.status != WorkOrderStatus::Draft as i16 {
        return Err(ProductionError::InvalidStatus.into());
    }
    sqlx::query!(
        "UPDATE work_orders SET status = $1 WHERE id = $2",
        WorkOrderStatus::Released as i16,
        &*path.id
    )
    .execute(&mut *conn)
    .await?;

    // 变更历史：写后重读全行作为 after（同一连接，提交后即见自身写入）
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
    .fetch_one(&mut *conn)
    .await?;
    AuditService::record_updated(&mut conn, "work_order", &path.id, &ctx, &before, &after).await?;

    Ok(WOResponse { success: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use production_contract::value_object::WorkOrderStatus;

    async fn seed_work_order(pool: &sqlx::PgPool, code: &str, status: i16) -> ID {
        let item_id = tests::insert_test_item(pool, &format!("I-{code}")).await;
        let bom_id = tests::insert_test_bom(pool, &format!("BOM-{code}"), &item_id).await;
        let id = ID::new();
        sqlx::query!(
            r#"INSERT INTO work_orders (id, code, bom_id, item_id, planned_qty, status)
               VALUES ($1, $2, $3, $4, 10, $5)"#,
            &*id,
            code,
            &*bom_id,
            &*item_id,
            status,
        )
        .execute(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        id
    }

    #[sqlx::test]
    async fn test_release_draft_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let id = seed_work_order(&state.pg_pool, "MO-RLS-1", WorkOrderStatus::Draft as i16).await;

        let resp = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            WOPath { id },
        )
        .await
        .unwrap();
        assert!(resp.success);

        // 变更历史：update 类型，status 0 → 1
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT entity, action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.entity, "work_order");
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["status"], WorkOrderStatus::Draft as i16);
        assert_eq!(after["status"], WorkOrderStatus::Released as i16);
    }

    #[sqlx::test]
    async fn test_release_not_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let id =
            seed_work_order(&state.pg_pool, "MO-RLS-2", WorkOrderStatus::Released as i16).await;

        let err = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            WOPath { id },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));

        // 状态未变，不产生变更历史
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let count =
            sqlx::query_scalar!("SELECT COUNT(*) FROM audit_logs WHERE entity_id = $1", *id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(count, Some(0));
    }
}
