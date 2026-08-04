use serde::{Deserialize, Serialize};
use sqlx::Type;

/// 审批状态：草稿 → 主管审批 → 经理审批 → 通过/拒绝
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum ApprovalStatus {
    Draft = 1,
    PendingSupervisor = 2,
    PendingManager = 3,
    Approved = 4,
    Rejected = 5,
}
