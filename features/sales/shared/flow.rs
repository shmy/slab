//! 销售域审批流定义。

use approval::StateTransitions;

/// 销售订单审批流：Draft(0) → Submit(1) → Supervisor(2) → Approved(3)。
pub(crate) const SO_FLOW: StateTransitions = StateTransitions {
    submit: (0, 1),
    approvals: &[(1, 2), (2, 3)],
    reject: None,
};
