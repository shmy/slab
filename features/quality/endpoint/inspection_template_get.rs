use axum::extract::State;
use db::PgPool;
use quality_contract::entity::{InspectionTemplate, InspectionTemplateItem};
use quality_contract::error::QualityError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetTemplatePath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TemplateDetail {
    pub template: InspectionTemplate,
    pub items: Vec<InspectionTemplateItem>,
}

#[utoipa::path(get, path = "/api/v1/inspection-templates/{id}", operation_id = "inspection_template_get", tag = "inspection-template",
    params(GetTemplatePath), responses((status = 200, body = JsonResponse<TemplateDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetTemplatePath>,
) -> JsonResponseType<TemplateDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetTemplatePath) -> rootcause::Result<TemplateDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!(
        "SELECT id, code, name, category, is_active FROM inspection_templates WHERE id = $1",
        &*path.id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(QualityError::TemplateNotFound)?;
    let items = sqlx::query!("SELECT id, template_id, name, specification, tolerance_upper, tolerance_lower, method, is_required, sort_order FROM inspection_template_items WHERE template_id = $1 ORDER BY sort_order", &*path.id)
        .fetch_all(&mut *conn).await?;
    Ok(TemplateDetail {
        template: InspectionTemplate {
            id: ID::new_unchecked(row.id),
            code: row.code,
            name: row.name,
            category: row.category,
            is_active: row.is_active,
        },
        items: items
            .into_iter()
            .map(|r| InspectionTemplateItem {
                id: ID::new_unchecked(r.id),
                template_id: ID::new_unchecked(r.template_id),
                name: r.name,
                specification: r.specification,
                tolerance_upper: r.tolerance_upper,
                tolerance_lower: r.tolerance_lower,
                method: r.method,
                is_required: r.is_required,
                sort_order: r.sort_order,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_template_get_with_items(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let template_id = tests::insert_test_template(&state.pg_pool, "TPL-GET-1").await;
        tests::insert_test_template_item(&state.pg_pool, &template_id).await;

        let detail = execute(&state.pg_pool, GetTemplatePath { id: template_id })
            .await
            .unwrap();
        assert_eq!(detail.template.code, "TPL-GET-1");
        assert_eq!(detail.template.category, 1);
        assert_eq!(detail.items.len(), 1);
        assert_eq!(detail.items[0].name, "CheckItem");
    }

    #[sqlx::test]
    async fn test_template_get_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(&state.pg_pool, GetTemplatePath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("inspection_template_not_found"));
    }
}
