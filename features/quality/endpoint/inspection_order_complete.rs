use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use quality_contract::entity::InspectionOrder;
use quality_contract::error::QualityError;
use quality_contract::value_object::{InspectionOrderStatus, Verdict};
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CompleteInspectionPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct RecordResult {
    pub template_item_id: ID,
    pub result: i16,
    pub actual_value: Option<String>,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CompleteInspectionRequest {
    pub results: Vec<RecordResult>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CompleteInspectionResponse {
    pub result: i16,
}

#[utoipa::path(
    post,
    path = "/api/v1/inspection-orders/{id}/complete",
    operation_id = "inspection_order_complete",
    tag = "inspection-order",
    params(CompleteInspectionPath),
    request_body = CompleteInspectionRequest,
    responses((status = 200, body = JsonResponse<CompleteInspectionResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<CompleteInspectionPath>,
    ValidJson(request): ValidJson<CompleteInspectionRequest>,
) -> JsonResponseType<CompleteInspectionResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: CompleteInspectionPath,
    request: CompleteInspectionRequest,
) -> rootcause::Result<CompleteInspectionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 锁定读 + 完成态守卫：已完成的检验单不可重复完成（避免重复写入结果行）
    // 锁读整行作为变更历史 before
    let before = sqlx::query_as!(
        InspectionOrder,
        r#"SELECT id AS "id: ID", code, template_id AS "template_id: ID", source_type, source_id AS "source_id: ID", item_id AS "item_id: ID",
                  lot_qty, sample_qty, inspector, result, status, inspected_at
           FROM inspection_orders WHERE id = $1 FOR UPDATE"#,
        &*path.id,
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(QualityError::InspectionNotFound)?;
    if before.status == InspectionOrderStatus::Inspected as i16 {
        return Err(QualityError::InvalidStatus.into());
    }

    // 判定整体检验结论：任一项 fail → 不通过
    let any_fail = request
        .results
        .iter()
        .any(|r| r.result == Verdict::Fail as i16);
    let overall = if any_fail {
        Verdict::Fail as i16
    } else {
        Verdict::Pass as i16
    };

    // 逐项写入检验结论明细
    for r in &request.results {
        let rid = ID::new();
        sqlx::query!(
            r#"INSERT INTO inspection_results (id, inspection_id, template_item_id, result, actual_value, remark)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
            &*rid,
            &*path.id,
            &*r.template_item_id,
            r.result,
            r.actual_value,
            r.remark,
        )
        .execute(&mut *txn)
        .await?;
    }

    // 更新检验单状态
    sqlx::query!(
        r#"UPDATE inspection_orders SET result = $1, status = $2, inspected_at = NOW() WHERE id = $3"#,
        overall,
        InspectionOrderStatus::Inspected as i16,
        &*path.id,
    )
    .execute(&mut *txn)
    .await?;

    // 变更历史：同事务回读写后实体，记录 updated（before 为上述锁读整行）
    let after = sqlx::query_as!(
        InspectionOrder,
        r#"SELECT id AS "id: ID", code, template_id AS "template_id: ID", source_type, source_id AS "source_id: ID", item_id AS "item_id: ID",
                  lot_qty, sample_qty, inspector, result, status, inspected_at
           FROM inspection_orders WHERE id = $1"#,
        &*path.id,
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_updated(
        &mut txn,
        "inspection_order",
        &path.id,
        &ctx,
        &before,
        &after,
    )
    .await?;

    txn.commit().await?;
    Ok(CompleteInspectionResponse { result: overall })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use quality_contract::value_object::{InspectionOrderStatus, Verdict};

    async fn seed_order(state: &appctx::AppCtx, code: &str) -> (ID, ID) {
        let template_id = tests::insert_test_template(&state.pg_pool, "TPL-CMP-1").await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-CMP-1").await;
        let template_item_id = tests::insert_test_template_item(&state.pg_pool, &template_id).await;
        let order_id =
            tests::insert_test_inspection_order(&state.pg_pool, code, &template_id, &item_id).await;
        (order_id, template_item_id)
    }

    #[sqlx::test]
    async fn test_complete_all_pass(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let (order_id, template_item_id) = seed_order(&state, "IQ-CMP-1").await;

        let req = CompleteInspectionRequest {
            results: vec![
                RecordResult {
                    template_item_id,
                    result: 1,
                    actual_value: Some("OK".into()),
                    remark: None,
                },
                RecordResult {
                    template_item_id,
                    result: 1,
                    actual_value: None,
                    remark: None,
                },
            ],
        };
        let resp = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CompleteInspectionPath { id: order_id },
            req,
        )
        .await
        .unwrap();
        assert_eq!(resp.result, Verdict::Pass as i16);

        let row = sqlx::query!(
            "SELECT result, status FROM inspection_orders WHERE id = $1",
            &*order_id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.result.unwrap_or(0), Verdict::Pass as i16);
        assert_eq!(row.status, InspectionOrderStatus::Inspected as i16);

        // 变更历史：updated 类型，before=待检(0)，after=已完成(10)
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *order_id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["status"], InspectionOrderStatus::Pending as i16);
        assert_eq!(after["status"], InspectionOrderStatus::Inspected as i16);
        assert_eq!(after["result"], Verdict::Pass as i16);
    }

    #[sqlx::test]
    async fn test_complete_any_fail_overall_fail(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let (order_id, template_item_id) = seed_order(&state, "IQ-CMP-2").await;

        let req = CompleteInspectionRequest {
            results: vec![RecordResult {
                template_item_id,
                result: 2, // fail
                actual_value: Some("尺寸超差".into()),
                remark: None,
            }],
        };
        let resp = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CompleteInspectionPath { id: order_id },
            req,
        )
        .await
        .unwrap();
        assert_eq!(resp.result, Verdict::Fail as i16);

        let row = sqlx::query!(
            "SELECT result FROM inspection_orders WHERE id = $1",
            &*order_id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.result.unwrap_or(0), Verdict::Fail as i16);
    }

    #[sqlx::test]
    async fn test_complete_already_completed_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let (order_id, template_item_id) = seed_order(&state, "IQ-CMP-3").await;

        // 模拟已完成的检验单
        sqlx::query!(
            "UPDATE inspection_orders SET status = $1 WHERE id = $2",
            InspectionOrderStatus::Inspected as i16,
            &*order_id
        )
        .execute(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();

        let req = CompleteInspectionRequest {
            results: vec![RecordResult {
                template_item_id,
                result: 1,
                actual_value: None,
                remark: None,
            }],
        };
        let err = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CompleteInspectionPath { id: order_id },
            req,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));

        // 被拒绝的状态流转不产生变更记录
        let count = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!" FROM audit_logs WHERE entity_id = $1"#,
            *order_id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(count.count, 0);
    }

    #[sqlx::test]
    async fn test_complete_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CompleteInspectionPath { id: ID::new() },
            CompleteInspectionRequest { results: vec![] },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("inspection_order_not_found"));
    }
}
