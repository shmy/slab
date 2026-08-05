use audit_contract::AuditService;
use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use quality_contract::entity::InspectionTemplate;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct TemplateItemInput {
    pub name: String,
    pub specification: Option<String>,
    pub tolerance_upper: Option<String>,
    pub tolerance_lower: Option<String>,
    pub method: Option<String>,
    pub is_required: Option<bool>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateTemplateRequest {
    pub name: String,
    pub category: i16,
    pub items: Vec<TemplateItemInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateTemplateResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/inspection-templates",
    operation_id = "inspection_template_create",
    tag = "inspection-template",
    request_body = CreateTemplateRequest,
    responses((status = 200, body = JsonResponse<CreateTemplateResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateTemplateRequest>,
) -> JsonResponseType<CreateTemplateResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateTemplateRequest,
) -> rootcause::Result<CreateTemplateResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let seq = CodeGen::next_seq(&mut conn, "seq_inspection_order").await?;
    let prefix = match request.category {
        2 => "IPQC",
        3 => "OQC",
        _ => "IQC",
    };
    let code = format!("{}-{:06}", prefix, seq);

    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    sqlx::query!(
        r#"INSERT INTO inspection_templates (id, code, name, category)
           VALUES ($1, $2, $3, $4)"#,
        &*id,
        code,
        request.name,
        request.category,
    )
    .execute(&mut *txn)
    .await?;

    for (i, item) in request.items.iter().enumerate() {
        let item_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO inspection_template_items
                   (id, template_id, name, specification, tolerance_upper,
                    tolerance_lower, method, is_required, sort_order)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            &*item_id,
            &*id,
            item.name,
            item.specification,
            item.tolerance_upper,
            item.tolerance_lower,
            item.method,
            item.is_required.unwrap_or(true),
            i as i16,
        )
        .execute(&mut *txn)
        .await?;
    }

    // 变更历史：同事务回读实体后记录 create
    let template = sqlx::query_as!(
        InspectionTemplate,
        r#"SELECT id, code, name, category, is_active
           FROM inspection_templates WHERE id = $1"#,
        &*id,
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_create(&mut txn, "inspection_template", &id, &ctx, &template).await?;

    txn.commit().await?;
    Ok(CreateTemplateResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_template_create_iqc_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let req = CreateTemplateRequest {
            name: "IQC 来料检验".into(),
            category: 1,
            items: vec![
                TemplateItemInput {
                    name: "外观".into(),
                    specification: None,
                    tolerance_upper: None,
                    tolerance_lower: None,
                    method: None,
                    is_required: Some(true),
                },
                TemplateItemInput {
                    name: "尺寸".into(),
                    specification: Some("±0.1".into()),
                    tolerance_upper: Some("0.1".into()),
                    tolerance_lower: Some("0.1".into()),
                    method: Some("卡尺".into()),
                    is_required: None,
                },
            ],
        };
        let resp = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("IQC-"));

        let n = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM inspection_template_items WHERE template_id = $1",
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
        assert_eq!(after["code"], resp.code);
        assert_eq!(after["name"], "IQC 来料检验");
    }

    #[sqlx::test]
    async fn test_template_create_category_prefix(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        // category=2 → IPQC，category=3 → OQC
        let req = CreateTemplateRequest {
            name: "IPQC".into(),
            category: 2,
            items: vec![],
        };
        let resp = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("IPQC-"));

        let req = CreateTemplateRequest {
            name: "OQC".into(),
            category: 3,
            items: vec![],
        };
        let resp = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("OQC-"));
    }
}
