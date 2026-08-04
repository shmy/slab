//! 仓库域审批流定义。

use approval::StateTransitions;

/// 调拨单审批流：Draft(0) → Submit(1) → Approved(3)。
pub(crate) const TR_FLOW: StateTransitions = StateTransitions {
    submit: (0, 1),
    approvals: &[(1, 3)],
    reject: None,
};

/// 盘点单审批流：Draft(0) → Submit(1) → Approved(3)。
pub(crate) const CH_FLOW: StateTransitions = StateTransitions {
    submit: (0, 1),
    approvals: &[(1, 3)],
    reject: None,
};
