use serde::{Deserialize, Serialize};
use sqlx::Type;

/// 工单生命周期状态（CONTEXT.md「生命周期状态」时间线）。
///
/// 草稿 -> 已下达 -> 进行中 -> 已完成 -> 已关闭。
/// 与审批流状态是独立的两条时间线；工单无审批流，由「下达」启动生命周期。
///
/// 状态转换在端点内直接校验（`work_order_release` / `work_order_complete`），
/// 未走 `approval::StateTransitions`（该抽象面向审批流，工单是生命周期模型）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum WorkOrderStatus {
    /// 草稿：可下达。
    Draft = 0,
    /// 已下达：可领料 / 可报工 / 可完成。
    Released = 1,
    /// 进行中：已开始执行。
    InProgress = 2,
    /// 已完成：生产完工。
    Completed = 3,
    /// 已关闭：生命周期终态。
    Closed = 4,
}
