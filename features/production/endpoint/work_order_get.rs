use axum::extract::State;
use db::PgPool;
use production_contract::entity::{WorkOrder, WorkOrderMaterial, WorkOrderOperation};
use production_contract::error::ProductionError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetWOPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkOrderDetail {
    pub work_order: WorkOrder,
    pub materials: Vec<WorkOrderMaterial>,
    pub operations: Vec<WorkOrderOperation>,
}

#[utoipa::path(get, path = "/api/v1/work-orders/{id}", operation_id = "work_order_get", tag = "work-order",
    params(GetWOPath), responses((status = 200, body = JsonResponse<WorkOrderDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetWOPath>,
) -> JsonResponseType<WorkOrderDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetWOPath) -> rootcause::Result<WorkOrderDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!("SELECT id, code, bom_id, item_id, planned_qty, completed_qty, scrap_qty, status, due_date, remark FROM work_orders WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn).await?.ok_or(ProductionError::NotFound)?;
    let materials = sqlx::query!("SELECT id, work_order_id, item_id, required_qty, picked_qty, warehouse_id FROM work_order_materials WHERE work_order_id = $1", &*path.id).fetch_all(&mut *conn).await?;
    let operations = sqlx::query!("SELECT id, work_order_id, name, sequence, planned_qty, completed_qty, scrap_qty, status FROM work_order_operations WHERE work_order_id = $1 ORDER BY sequence", &*path.id).fetch_all(&mut *conn).await?;
    Ok(WorkOrderDetail {
        work_order: WorkOrder {
            id: ID::new_unchecked(row.id),
            code: row.code,
            bom_id: ID::new_unchecked(row.bom_id),
            item_id: ID::new_unchecked(row.item_id),
            planned_qty: row.planned_qty,
            completed_qty: row.completed_qty.unwrap_or(0),
            scrap_qty: row.scrap_qty.unwrap_or(0),
            status: row.status,
            due_date: row.due_date,
            remark: row.remark,
        },
        materials: materials
            .into_iter()
            .map(|r| WorkOrderMaterial {
                id: ID::new_unchecked(r.id),
                work_order_id: ID::new_unchecked(r.work_order_id),
                item_id: ID::new_unchecked(r.item_id),
                required_qty: r.required_qty,
                picked_qty: r.picked_qty.unwrap_or(0),
                warehouse_id: r.warehouse_id.map(ID::new_unchecked),
            })
            .collect(),
        operations: operations
            .into_iter()
            .map(|r| WorkOrderOperation {
                id: ID::new_unchecked(r.id),
                work_order_id: ID::new_unchecked(r.work_order_id),
                name: r.name,
                sequence: r.sequence,
                planned_qty: r.planned_qty,
                completed_qty: r.completed_qty.unwrap_or(0),
                scrap_qty: r.scrap_qty.unwrap_or(0),
                status: r.status,
            })
            .collect(),
    })
}
