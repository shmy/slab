//! 库存台账 — 统一管理库存变更与流水记录。
//!
//! 所有涉及库存变动的领域（销售、采购、生产、仓库）均通过本模块操作，
//! 消除重复的 FOR UPDATE / UPSERT / 流水写入逻辑。

use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

/// 库存台账错误。
#[derive(Debug, thiserror::Error)]
pub enum InventoryError {
    #[error("insufficient_inventory")]
    InsufficientInventory,
}

/// 库存交易类型（对应 inventory_transactions.transaction_type）。
#[derive(Debug, Clone, Copy)]
pub enum TransactionType {
    /// 入库（采购、生产等）
    Inbound = 1,
    /// 出库（销售等）
    Outbound = 2,
    /// 调拨入库
    TransferIn = 3,
    /// 调拨出库
    TransferOut = 4,
    /// 盘点调整
    InventoryCheck = 5,
    /// 采购退货出库
    PurchaseReturn = 6,
    /// 工单领料出库
    MaterialPick = 7,
}

/// 一次库存台账操作的完整描述。
///
/// 把「库存定位（物料/仓库）+ 变动数量 + 记账类型 + 业务溯源（来源单据/批次）」
/// 聚合成一个命名字段的结构；动作方法统一接收它，调用方无需按位置记忆一长串参数。
#[derive(Debug, Clone, Copy)]
pub struct LedgerCommand<'a> {
    /// 物料
    pub item_id: &'a ID,
    /// 仓库
    pub warehouse_id: &'a ID,
    /// 变动数量（receive/issue 为正数；adjust 内部可能为负）
    pub quantity: i64,
    /// 记账类型（对应 inventory_transactions.transaction_type）
    pub tx_type: TransactionType,
    /// 来源单据类型（如 "sales_delivery"、"purchase_receipt"）
    pub reference_type: &'a str,
    /// 来源单据 ID
    pub reference_id: &'a ID,
    /// 批次号（可选）
    pub batch_number: Option<&'a str>,
}

/// 库存台账。
pub struct InventoryLedger;

impl InventoryLedger {
    /// 入库：增加指定仓库的库存数量。
    ///
    /// 用于采购收货、生产完工入库、调拨入库等场景。
    pub async fn receive(conn: &mut PgConnection, cmd: &LedgerCommand<'_>) -> Result<()> {
        let before_qty = Self::read_qty(conn, cmd.item_id, cmd.warehouse_id).await?;
        let after_qty = before_qty + cmd.quantity;
        Self::upsert_and_log(conn, cmd, after_qty, before_qty).await
    }

    /// 负库存出库：允许库存变为负数的出库（即原「强制出库」）。
    ///
    /// 用于采购退货等场景，即使库存不足也继续执行。
    pub async fn force_issue(conn: &mut PgConnection, cmd: &LedgerCommand<'_>) -> Result<()> {
        let before_qty = Self::read_qty(conn, cmd.item_id, cmd.warehouse_id).await?;
        let after_qty = before_qty - cmd.quantity;
        Self::upsert_and_log(conn, cmd, after_qty, before_qty).await
    }

    /// 出库：减少指定仓库的库存数量。
    ///
    /// 用于销售发货、调拨出库等场景。
    /// 库存不足时返回 `InventoryError::InsufficientInventory`。
    pub async fn issue(conn: &mut PgConnection, cmd: &LedgerCommand<'_>) -> Result<()> {
        let before_qty = Self::read_qty(conn, cmd.item_id, cmd.warehouse_id).await?;
        if before_qty < cmd.quantity {
            return Err(InventoryError::InsufficientInventory.into());
        }
        let after_qty = before_qty - cmd.quantity;
        Self::upsert_and_log(conn, cmd, after_qty, before_qty).await
    }

    /// 盘点调整：将库存调整为指定数量。
    pub async fn adjust(
        conn: &mut PgConnection,
        item_id: &ID,
        warehouse_id: &ID,
        new_quantity: i64,
        reference_type: &str,
        reference_id: &ID,
    ) -> Result<()> {
        let before_qty = Self::read_qty(conn, item_id, warehouse_id).await?;
        let adjustment = new_quantity - before_qty;
        if adjustment == 0 {
            return Ok(());
        }
        let cmd = LedgerCommand {
            item_id,
            warehouse_id,
            quantity: adjustment,
            tx_type: TransactionType::InventoryCheck,
            reference_type,
            reference_id,
            batch_number: None,
        };
        Self::upsert_and_log(conn, &cmd, new_quantity, before_qty).await
    }

    /// 调拨：从源仓库出库 + 目标仓库入库（调用方负责事务）。
    pub async fn transfer(
        conn: &mut PgConnection,
        item_id: &ID,
        from_warehouse_id: &ID,
        to_warehouse_id: &ID,
        quantity: i64,
        reference_id: &ID,
        batch_number: Option<&str>,
    ) -> Result<()> {
        Self::issue(
            conn,
            &LedgerCommand {
                item_id,
                warehouse_id: from_warehouse_id,
                quantity,
                tx_type: TransactionType::TransferOut,
                reference_type: "stock_transfer_out",
                reference_id,
                batch_number,
            },
        )
        .await?;

        Self::receive(
            conn,
            &LedgerCommand {
                item_id,
                warehouse_id: to_warehouse_id,
                quantity,
                tx_type: TransactionType::TransferIn,
                reference_type: "stock_transfer_in",
                reference_id,
                batch_number,
            },
        )
        .await
    }

    // ─── 内部辅助 ───

    /// 读取当前库存（FOR UPDATE），不存在时返回 0。
    async fn read_qty(conn: &mut PgConnection, item_id: &ID, warehouse_id: &ID) -> Result<i64> {
        let row = sqlx::query!(
            r#"SELECT quantity FROM inventories
               WHERE item_id = $1 AND warehouse_id = $2
               FOR UPDATE"#,
            item_id as _,
            warehouse_id as _,
        )
        .fetch_optional(conn)
        .await?;
        Ok(row.map(|r| r.quantity).unwrap_or(0))
    }

    /// UPSERT 库存 + 写入流水。
    async fn upsert_and_log(
        conn: &mut PgConnection,
        cmd: &LedgerCommand<'_>,
        new_quantity: i64,
        before_qty: i64,
    ) -> Result<()> {
        let inv_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version)
               VALUES ($1, $2, $3, $4, 0, 1)
               ON CONFLICT (item_id, warehouse_id) DO UPDATE
               SET quantity = $4, version = inventories.version + 1"#,
            &*inv_id,
            cmd.item_id as _,
            cmd.warehouse_id as _,
            new_quantity,
        )
        .execute(&mut *conn)
        .await?;

        Self::write_transaction(conn, cmd, before_qty, new_quantity).await
    }

    /// 写入库存流水。
    async fn write_transaction(
        conn: &mut PgConnection,
        cmd: &LedgerCommand<'_>,
        before_qty: i64,
        after_qty: i64,
    ) -> Result<()> {
        let tx_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO inventory_transactions
                   (id, item_id, warehouse_id, transaction_type, quantity, batch_number,
                    reference_type, reference_id, before_quantity, after_quantity)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
            &*tx_id,
            cmd.item_id as _,
            cmd.warehouse_id as _,
            cmd.tx_type as i16,
            cmd.quantity,
            cmd.batch_number,
            cmd.reference_type,
            cmd.reference_id as _,
            before_qty,
            after_qty,
        )
        .execute(conn)
        .await?;
        Ok(())
    }
}
