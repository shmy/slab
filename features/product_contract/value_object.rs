use serde::{Deserialize, Serialize};
use sqlx::Type;

/// BOM 生命周期状态（CONTEXT.md「生命周期状态」时间线）。
///
/// 草稿 -> 已发布 -> 已废弃。
/// 只有已发布的 BOM 会被 MRP 纳入需求计算；发布是不可逆的状态前置。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum BomStatus {
    /// 草稿：可编辑 / 可发布。
    Draft = 0,
    /// 已发布：MRP 净需求计算纳入。
    Released = 1,
    /// 已废弃：不再纳入计算。
    Obsolete = 2,
}
