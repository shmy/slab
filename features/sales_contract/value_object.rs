use serde::{Deserialize, Serialize};
use sqlx::Type;

/// 销售订单审批流状态（CONTEXT.md「审批流状态」时间线）。
///
/// 草稿 -> 已提交 -> 主管审批中 -> 已批准。
/// 与生命周期状态（发货 / 退货 / 开票）是独立的两条时间线；无驳回。
///
/// 流转规则见 `features/sales/shared/flow.rs` 的 `SO_FLOW`。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum SalesOrderStatus {
    /// 草稿：可编辑 / 可提交。
    Draft = 0,
    /// 已提交：进入审批流，等待主管审批。
    Submitted = 1,
    /// 主管审批中：第一次审批通过后的中间态。
    PendingSupervisor = 2,
    /// 已批准：审批流终态，可发货。
    Approved = 3,
}
