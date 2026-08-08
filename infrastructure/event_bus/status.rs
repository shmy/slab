#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i16)]
pub(crate) enum DeliveryStatus {
    Pending = 1,
    Delivered = 2,
    Failed = 3,
}

impl DeliveryStatus {
    pub(crate) const fn as_i16(self) -> i16 {
        self as i16
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RetryNextAttempt {
    DelaySecs(i64),
    Terminal,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RetryPlan {
    pub(crate) attempts: i32,
    pub(crate) status: DeliveryStatus,
    pub(crate) last_error: String,
    pub(crate) next_attempt_at: RetryNextAttempt,
}

impl RetryPlan {
    pub(crate) fn from_failure(
        attempts: i32,
        max_attempts: i32,
        backoff_max_secs: i64,
        error: &str,
    ) -> Self {
        let next_attempt = attempts + 1;
        if next_attempt >= max_attempts {
            return Self {
                attempts: max_attempts,
                status: DeliveryStatus::Failed,
                last_error: format!(
                    "permanently_failed_after_{}_attempts: {}",
                    max_attempts, error
                ),
                next_attempt_at: RetryNextAttempt::Terminal,
            };
        }

        let shift = next_attempt.clamp(0, 10);
        let delay_secs = (1_i64 << shift).min(backoff_max_secs);
        Self {
            attempts: next_attempt,
            status: DeliveryStatus::Pending,
            last_error: error.to_string(),
            next_attempt_at: RetryNextAttempt::DelaySecs(delay_secs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_plan_transitions_to_terminal_on_last_attempt() {
        let plan = RetryPlan::from_failure(4, 5, 300, "boom");

        assert_eq!(plan.attempts, 5);
        assert_eq!(plan.status, DeliveryStatus::Failed);
        assert_eq!(plan.next_attempt_at, RetryNextAttempt::Terminal);
        assert_eq!(plan.last_error, "permanently_failed_after_5_attempts: boom");
    }

    #[test]
    fn retry_plan_schedules_exponential_backoff_before_last_attempt() {
        let plan = RetryPlan::from_failure(1, 5, 300, "boom");

        assert_eq!(plan.attempts, 2);
        assert_eq!(plan.status, DeliveryStatus::Pending);
        assert_eq!(plan.next_attempt_at, RetryNextAttempt::DelaySecs(4));
        assert_eq!(plan.last_error, "boom");
    }

    #[test]
    fn event_status_uses_stable_smallint_values() {
        assert_eq!(DeliveryStatus::Pending.as_i16(), 1);
        assert_eq!(DeliveryStatus::Delivered.as_i16(), 2);
        assert_eq!(DeliveryStatus::Failed.as_i16(), 3);
    }

    #[test]
    fn retry_plan_backoff_caps_at_max_secs() {
        // attempts=9 → 下一跳 shift=10 → 2^10=1024，封顶到 backoff_max_secs=300
        let plan = RetryPlan::from_failure(9, 12, 300, "boom");
        assert_eq!(plan.next_attempt_at, RetryNextAttempt::DelaySecs(300));
        // 未达封顶时按指数退避：attempts=4 → shift=5 → 2^5=32
        let plan = RetryPlan::from_failure(4, 12, 300, "boom");
        assert_eq!(plan.next_attempt_at, RetryNextAttempt::DelaySecs(32));
        // 封顶小于指数值才生效：backoff_max_secs=10 时 2^5=32 被压到 10
        let plan = RetryPlan::from_failure(4, 12, 10, "boom");
        assert_eq!(plan.next_attempt_at, RetryNextAttempt::DelaySecs(10));
    }
}
