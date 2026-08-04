//! 审批状态机 — 统一管理单据 submit/approve/reject 的状态迁移规则。
//!
//! # 用法
//!
//! 在每个 domain 中定义对应单据类型的 `StateTransitions` 常量，然后在
//! submit/approve/reject 端点中调用 `transitions.submit_status(status)` 等
//! 方法获取目标状态，再执行 UPDATE SQL。
//!
//! ```ignore
//! use approval::StateTransitions;
//!
//! const PO_FLOW: StateTransitions = StateTransitions {
//!     submit: (0, 1),
//!     approvals: &[(1, 2), (2, 3)],
//!     reject: Some((&[1, 2], 4)),
//! };
//!
//! let new_status = PO_FLOW.submit_status(current)?;
//! ```

use rootcause::Result;

/// 状态迁移错误。
///
/// 消息固定为本地化 key；`current` / `action` 字段保留状态机细节，
/// 供日志与调试使用（客户端不需要参数化消息）。
#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("invalid_status_transition")]
    InvalidTransition { current: i16, action: &'static str },
}

/// 单据审批流定义。
///
/// 每个单据类型定义一个常量，描述其 submit → approval chain → reject 规则。
#[derive(Debug, Clone, Copy)]
pub struct StateTransitions {
    /// (草稿状态, 提交后状态) —— 例如 `(0, 1)`
    pub submit: (i16, i16),
    /// 审批链：连续的前置→后置对 —— 例如 `&[(1, 2), (2, 3)]`
    /// 第一个匹配当前状态的 pair 决定审批下一步的目标状态。
    pub approvals: &'static [(i16, i16)],
    /// (可驳回的源状态列表, 驳回后状态) —— 例如 `Some((&[1, 2], 4))`
    pub reject: Option<(&'static [i16], i16)>,
}

impl StateTransitions {
    /// 计算提交后的目标状态。
    pub fn submit_status(&self, current: i16) -> Result<i16> {
        if current == self.submit.0 {
            Ok(self.submit.1)
        } else {
            Err(TransitionError::InvalidTransition {
                current,
                action: "submit",
            }
            .into())
        }
    }

    /// 计算审批通过后的目标状态（走审批链）。
    pub fn approve_status(&self, current: i16) -> Result<i16> {
        self.approvals
            .iter()
            .find(|(from, _)| *from == current)
            .map(|(_, to)| *to)
            .ok_or_else(|| {
                TransitionError::InvalidTransition {
                    current,
                    action: "approve",
                }
                .into()
            })
    }

    /// 计算驳回后的目标状态。
    pub fn reject_status(&self, current: i16) -> Result<i16> {
        match self.reject {
            Some((from_states, to)) if from_states.contains(&current) => Ok(to),
            _ => Err(TransitionError::InvalidTransition {
                current,
                action: "reject",
            }
            .into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_STEP_FLOW: StateTransitions = StateTransitions {
        submit: (0, 1),
        approvals: &[(1, 2), (2, 3)],
        reject: Some((&[1, 2], 4)),
    };

    const ONE_STEP_FLOW: StateTransitions = StateTransitions {
        submit: (0, 1),
        approvals: &[(1, 3)],
        reject: None,
    };

    #[test]
    fn test_submit_draft() {
        assert_eq!(TWO_STEP_FLOW.submit_status(0).unwrap(), 1);
    }

    #[test]
    fn test_submit_non_draft_fails() {
        assert!(TWO_STEP_FLOW.submit_status(1).is_err());
        assert!(TWO_STEP_FLOW.submit_status(3).is_err());
    }

    #[test]
    fn test_approve_first_step() {
        assert_eq!(TWO_STEP_FLOW.approve_status(1).unwrap(), 2);
    }

    #[test]
    fn test_approve_second_step() {
        assert_eq!(TWO_STEP_FLOW.approve_status(2).unwrap(), 3);
    }

    #[test]
    fn test_approve_one_step() {
        assert_eq!(ONE_STEP_FLOW.approve_status(1).unwrap(), 3);
    }

    #[test]
    fn test_approve_already_approved_fails() {
        assert!(TWO_STEP_FLOW.approve_status(3).is_err());
    }

    #[test]
    fn test_approve_draft_fails() {
        assert!(TWO_STEP_FLOW.approve_status(0).is_err());
    }

    #[test]
    fn test_reject_pending() {
        assert_eq!(TWO_STEP_FLOW.reject_status(1).unwrap(), 4);
        assert_eq!(TWO_STEP_FLOW.reject_status(2).unwrap(), 4);
    }

    #[test]
    fn test_reject_not_rejectable() {
        assert!(TWO_STEP_FLOW.reject_status(0).is_err());
        assert!(TWO_STEP_FLOW.reject_status(3).is_err());
    }

    #[test]
    fn test_one_step_no_reject() {
        assert!(ONE_STEP_FLOW.reject_status(1).is_err());
    }

    #[test]
    fn test_submit_and_approve_one_step() {
        let status = ONE_STEP_FLOW.submit_status(0).unwrap();
        assert_eq!(status, 1);
        let status = ONE_STEP_FLOW.approve_status(status).unwrap();
        assert_eq!(status, 3);
    }
}
