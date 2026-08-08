use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use product_contract::entity::Bom;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct BomItemInput {
    pub item_id: ID,
    pub quantity: i64,
    pub unit: String,
    pub wastage_rate: Option<i64>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateBomRequest {
    pub name: String,
    pub item_id: ID,
    pub total_qty: Option<i64>,
    pub remark: Option<String>,
    pub items: Vec<BomItemInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateBomResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(post, path = "/api/v1/boms",
    operation_id = "bom_create", tag = "bom",
    request_body = CreateBomRequest,
    responses((status = 200, body = JsonResponse<CreateBomResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateBomRequest>,
) -> JsonResponseType<CreateBomResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateBomRequest,
) -> rootcause::Result<CreateBomResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let code = DocNumberer::next_number(&mut conn, "seq_bom", "BOM").await?;

    let mut txn = conn.begin().await?;

    sqlx::query!(
        r#"INSERT INTO boms (id, code, name, item_id, total_qty, remark, status)
           VALUES ($1, $2, $3, $4, $5, $6, 0)"#,
        &*id,
        code,
        request.name,
        &*request.item_id,
        request.total_qty.unwrap_or(1),
        request.remark,
    )
    .execute(&mut *txn)
    .await?;

    for (i, item) in request.items.iter().enumerate() {
        let item_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO bom_items (id, bom_id, item_id, quantity, unit, wastage_rate, sort_order)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            &*item_id,
            &*id,
            &*item.item_id,
            item.quantity,
            item.unit,
            item.wastage_rate.unwrap_or(0),
            i as i16,
        )
        .execute(&mut *txn)
        .await?;
    }
    let bom: Bom = sqlx::query_as!(
        Bom,
        r#"SELECT id, code, name, item_id, version, status, total_qty, remark FROM boms WHERE id = $1"#,
        &*id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_create(&mut txn, "bom", &id, &ctx, &bom).await?;
    txn.commit().await?;
    Ok(CreateBomResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_bom_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-BOM-1").await;

        let req = CreateBomRequest {
            name: "玩具车 BOM".into(),
            item_id,
            total_qty: None, // 默认 1
            remark: None,
            items: vec![
                BomItemInput {
                    item_id,
                    quantity: 2,
                    unit: "kg".into(),
                    wastage_rate: None,
                },
                BomItemInput {
                    item_id,
                    quantity: 1,
                    unit: "pcs".into(),
                    wastage_rate: Some(5),
                },
            ],
        };
        let resp = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("BOM-"));

        let row = sqlx::query!(
            "SELECT total_qty, status FROM boms WHERE id = $1",
            &*resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.total_qty, 1);
        assert_eq!(row.status, 0);

        let n = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM bom_items WHERE bom_id = $1",
            &*resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(n.unwrap_or(0), 2);

        // 变更历史：create 类型
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["name"], "玩具车 BOM");
        assert_eq!(after["status"], 0);
    }
}
