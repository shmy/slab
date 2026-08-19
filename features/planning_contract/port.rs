//! 规划只读 Port — 供 MRP 等跨域读操作使用。

use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

/// 物料库存聚合：活跃物料的当前库存总量与在途采购量。
///
/// 「当前库存总量」与「在途采购量」的口径单一事实来源，
/// 供采购建议、再订货预警、MRP 等规划查询共用（MRP 因需在同一 SQL 内
/// join demand，内部保留等价 CTE，口径以本方法为准）。
#[derive(Debug, Clone)]
pub struct ItemStockAggregate {
    pub item_id: ID,
    pub item_code: String,
    pub item_name: String,
    pub item_type: i16,
    pub safety_stock: i64,
    pub reorder_point: i64,
    /// 当前库存总量（所有仓库求和）
    pub current_stock: i64,
    /// 在途采购量（已审批未收货的采购订单行 quantity - received_qty）
    pub in_transit_qty: i64,
}

/// MRP 净需求项。
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct MrpItem {
    pub item_id: ID,
    pub item_code: String,
    pub item_name: String,
    pub gross_demand: i64,
    pub current_stock: i64,
    pub in_transit_qty: i64,
    pub net_demand: i64,
    pub safety_stock: i64,
    pub suggested_order_qty: i64,
}

/// 规划只读 Port。
pub struct PlanningPort;

impl PlanningPort {
    /// `purchase_contract::PurchaseOrderStatus::Approved`。
    /// contract 不得互依，故内联判别值；PO 审批终态变更时同步。
    const PURCHASE_ORDER_APPROVED: i16 = 3;
    /// `product_contract::BomStatus::Released`。
    const BOM_RELEASED: i16 = 1;
    /// 全部活跃物料的库存与在途聚合。
    ///
    /// 返回全部活跃物料（不做业务过滤），由调用方按 item_type / safety_stock /
    /// reorder_point 等业务条件过滤与计算，避免每个规划端点各自内联聚合 SQL。
    pub async fn stock_and_transit(conn: &mut PgConnection) -> Result<Vec<ItemStockAggregate>> {
        let rows = sqlx::query(
            r#"SELECT i.id, i.code, i.name, i.item_type, i.safety_stock, i.reorder_point,
                      COALESCE(SUM(iv.quantity), 0)::BIGINT AS current_stock,
                      COALESCE(
                          (SELECT SUM(pol.quantity - pol.received_qty)
                           FROM purchase_order_lines pol
                           JOIN purchase_orders po ON po.id = pol.order_id
                           WHERE po.status = $1 AND pol.item_id = i.id
                             AND pol.quantity > pol.received_qty),
                      0)::BIGINT AS in_transit_qty
               FROM items i
               LEFT JOIN inventories iv ON iv.item_id = i.id
               WHERE i.is_active = true
               GROUP BY i.id, i.code, i.name, i.item_type, i.safety_stock, i.reorder_point"#,
        )
        .bind(Self::PURCHASE_ORDER_APPROVED)
        .fetch_all(conn)
        .await?;

        use sqlx::Row;
        let items = rows
            .iter()
            .map(|row| ItemStockAggregate {
                item_id: ID::new_unchecked(row.get::<i64, _>("id")),
                item_code: row.get("code"),
                item_name: row.get("name"),
                item_type: row.get("item_type"),
                safety_stock: row.get("safety_stock"),
                reorder_point: row.get("reorder_point"),
                current_stock: row.get("current_stock"),
                in_transit_qty: row.get("in_transit_qty"),
            })
            .collect();
        Ok(items)
    }

    /// 执行 MRP 净需求计算。
    ///
    /// 销售订单行(成品) → BOM 展开 → 毛需求(原料) → 减库存 → 减在途 PO → 净需求。
    /// stock / transit CTE 与 `Self::stock_and_transit` 口径一致
    /// （MRP 需在同一 SQL 内 join demand，无法复用方法，改口径时两处同步）。
    pub async fn mrp_calculate(conn: &mut PgConnection) -> Result<Vec<MrpItem>> {
        let rows = sqlx::query(
            r#"WITH demand AS (
                   SELECT bi.item_id AS raw_item_id,
                          SUM(sol.quantity * bi.quantity)::BIGINT AS gross_demand
                   FROM sales_order_lines sol
                   JOIN sales_orders so ON so.id = sol.order_id
                   JOIN boms b ON b.item_id = sol.item_id
                   JOIN bom_items bi ON bi.bom_id = b.id
                   WHERE sol.closed = FALSE
                     AND b.status = $1
                   GROUP BY bi.item_id
               ),
               stock AS (
                   SELECT item_id,
                          COALESCE(SUM(quantity), 0)::BIGINT AS current_stock
                   FROM inventories
                   GROUP BY item_id
               ),
               transit AS (
                   SELECT pol.item_id,
                          COALESCE(SUM(pol.quantity - pol.received_qty), 0)::BIGINT AS in_transit_qty
                   FROM purchase_order_lines pol
                   JOIN purchase_orders po ON po.id = pol.order_id
                   WHERE po.status = $2
                     AND pol.quantity > pol.received_qty
                   GROUP BY pol.item_id
               )
               SELECT i.id, i.code, i.name, i.safety_stock,
                      COALESCE(d.gross_demand, 0)::BIGINT AS gross_demand,
                      COALESCE(s.current_stock, 0)::BIGINT AS current_stock,
                      COALESCE(t.in_transit_qty, 0)::BIGINT AS in_transit_qty,
                      GREATEST(
                          COALESCE(d.gross_demand, 0) -
                          COALESCE(s.current_stock, 0) -
                          COALESCE(t.in_transit_qty, 0),
                      0)::BIGINT AS net_demand
               FROM items i
               LEFT JOIN demand d ON d.raw_item_id = i.id
               LEFT JOIN stock s ON s.item_id = i.id
               LEFT JOIN transit t ON t.item_id = i.id
               WHERE d.gross_demand IS NOT NULL
                  OR i.safety_stock > COALESCE(s.current_stock, 0) + COALESCE(t.in_transit_qty, 0)
               ORDER BY net_demand DESC"#,
        )
        .bind(Self::BOM_RELEASED)
        .bind(Self::PURCHASE_ORDER_APPROVED)
        .fetch_all(conn)
        .await?;

        use sqlx::Row;
        let items: Vec<MrpItem> = rows
            .iter()
            .map(|row| {
                let gross_demand: i64 = row.get("gross_demand");
                let current_stock: i64 = row.get("current_stock");
                let in_transit_qty: i64 = row.get("in_transit_qty");
                let safety_stock: i64 = row.get("safety_stock");
                let net_demand: i64 = row.get("net_demand");
                MrpItem {
                    item_id: ID::new_unchecked(row.get::<i64, _>("id")),
                    item_code: row.get("code"),
                    item_name: row.get("name"),
                    gross_demand,
                    current_stock,
                    in_transit_qty,
                    net_demand,
                    safety_stock,
                    suggested_order_qty: net_demand + safety_stock,
                }
            })
            .collect();

        Ok(items)
    }
}
