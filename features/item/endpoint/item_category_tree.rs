use axum::extract::State;
use db::PgPool;
use serde::Serialize;
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CategoryTreeNode {
    pub id: ID,
    pub name: String,
    pub sort_order: i32,
    #[schema(no_recursion)]
    pub children: Vec<CategoryTreeNode>,
}

#[utoipa::path(
    get,
    path = "/api/v1/item-categories/tree",
    operation_id = "item_category_tree",
    tag = "item-category",
    responses((status = 200, body = JsonResponse<Vec<CategoryTreeNode>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
) -> JsonResponseType<Vec<CategoryTreeNode>> {
    let response = execute(&pg_pool).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool) -> rootcause::Result<Vec<CategoryTreeNode>> {
    let mut conn = pg_pool.acquire().await?;
    let rows = sqlx::query_as!(
        CategoryRow,
        r#"SELECT id, name, parent_id, sort_order
           FROM item_categories ORDER BY parent_id NULLS FIRST, sort_order, id"#
    )
    .fetch_all(&mut *conn)
    .await?;

    let tree = build_tree(&rows, None);
    Ok(tree)
}

#[derive(Debug, sqlx::FromRow)]
struct CategoryRow {
    id: i64,
    name: String,
    parent_id: Option<i64>,
    sort_order: i32,
}

fn build_tree(nodes: &[CategoryRow], parent_id: Option<i64>) -> Vec<CategoryTreeNode> {
    nodes
        .iter()
        .filter(|n| n.parent_id == parent_id)
        .map(|n| CategoryTreeNode {
            id: ID::new_unchecked(n.id),
            name: n.name.clone(),
            sort_order: n.sort_order,
            children: build_tree(nodes, Some(n.id)),
        })
        .collect()
}
