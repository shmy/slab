use serde::{Deserialize, Serialize};
use sqlx::Type;

/// 检验单状态（CONTEXT.md「检验单状态」）。
///
/// 待检 -> 已检。「已检」**不代表通过**--检验结论（`Verdict`）是独立维度；
/// 不合格的检验单同样走完流程（状态=已检），只是结论=不通过。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum InspectionOrderStatus {
    /// 待检：检验单已创建，尚未完成检验。
    Pending = 0,
    /// 已检：检验已完成，检验结论见 `result` 字段（`Verdict`）。
    Inspected = 10,
}

/// 检验结论（CONTEXT.md「检验结论」，代码字段名 `result`）。
///
/// 检验人对检验单下达的判定，与检验单状态（待检/已检）是独立维度。
/// 单个检验项结果（`InspectionResult.result`）仅用 `Pass`/`Fail` 两个变体。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum Verdict {
    /// 通过。
    Pass = 1,
    /// 不通过。
    Fail = 2,
    /// 有条件通过。
    Conditional = 3,
}
