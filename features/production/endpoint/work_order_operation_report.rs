use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use production_contract::entity::WorkOrder;
use production_contract::error::ProductionError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct ReportPath {
    pub work_order_id: ID,
    pub operation_id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct ReportRequest {
    pub completed_qty: i64,
    pub scrap_qty: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReportResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/work-orders/{work_order_id}/operations/{operation_id}/report",
    operation_id = "work_order_operation_report", tag = "work-order",
    params(ReportPath), request_body = ReportRequest,
    responses((status = 200, body = JsonResponse<ReportResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<ReportPath>,
    ValidJson(request): ValidJson<ReportRequest>,
) -> JsonResponseType<ReportResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: ReportPath,
    request: ReportRequest,
) -> rootcause::Result<ReportResponse> {
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
        &*path.work_order_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(ProductionError::NotFound)?;

    sqlx::query!(
        r#"UPDATE work_order_operations
           SET completed_qty = completed_qty + $1, scrap_qty = scrap_qty + $2, status = 2
           WHERE id = $3 AND work_order_id = $4"#,
        request.completed_qty,
        request.scrap_qty.unwrap_or(0),
        &*path.operation_id,
        &*path.work_order_id,
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query!(
        r#"UPDATE work_orders
           SET completed_qty = (SELECT COALESCE(SUM(completed_qty),0) FROM work_order_operations WHERE work_order_id = $1),
               scrap_qty = (SELECT COALESCE(SUM(scrap_qty),0) FROM work_order_operations WHERE work_order_id = $1)
           WHERE id = $1"#, &*path.work_order_id,
    ).execute(&mut *conn).await?;

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
        &*path.work_order_id
    )
    .fetch_one(&mut *conn)
    .await?;
    AuditService::record_updated(
        &mut conn,
        "work_order",
        &path.work_order_id,
        &ctx,
        &before,
        &after,
    )
    .await?;

    Ok(ReportResponse { success: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_report_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        let item_id = tests::insert_test_item(&state.pg_pool, "I-WO-RP-1").await;
        let bom_id = tests::insert_test_bom(&state.pg_pool, "BOM-WO-RP-1", &item_id).await;
        let wo_id = ID::new();
        let op_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();

        sqlx::query!(
            r#"INSERT INTO work_orders (id, code, bom_id, item_id, planned_qty, status)
               VALUES ($1, $2, $3, $4, 10, 1)"#,
            &*wo_id,
            "MO-RP-1",
            &*bom_id,
            &*item_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            r#"INSERT INTO work_order_operations (id, work_order_id, name, sequence, planned_qty, status)
               VALUES ($1, $2, 'OP10', 10, 10, 0)"#,
            &*op_id,
            &*wo_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let resp = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            ReportPath {
                work_order_id: wo_id,
                operation_id: op_id,
            },
            ReportRequest {
                completed_qty: 5,
                scrap_qty: None,
            },
        )
        .await
        .unwrap();
        assert!(resp.success);

        // 变更历史：update 类型，completed_qty 0 → 5
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
        assert_eq!(before["completed_qty"], 0);
        assert_eq!(after["completed_qty"], 5);
    }

    #[sqlx::test]
    async fn test_report_work_order_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        let err = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            ReportPath {
                work_order_id: ID::new(),
                operation_id: ID::new(),
            },
            ReportRequest {
                completed_qty: 1,
                scrap_qty: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("work_order_not_found"));
    }
}
