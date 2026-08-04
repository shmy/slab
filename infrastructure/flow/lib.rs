use std::fmt::Debug;
use std::sync::Arc;

use rootcause::Result;
use sayiir_core::codec::sealed;
use sayiir_core::codec::{Codec, EnvelopeCodec};
pub use sayiir_core::error::BoxError;
pub use sayiir_core::workflow::{ConflictPolicy, Workflow, WorkflowStatus};
#[cfg(feature = "test-utils")]
use sayiir_persistence::InMemoryBackend;
use sayiir_postgres::PostgresBackend;
use sayiir_runtime::CheckpointingRunner;
pub use sayiir_runtime::error::RuntimeError;
use sayiir_runtime::serialization::JsonCodec;
pub use sayiir_runtime::{task, workflow};

/// 流程引擎句柄。
///
/// 封装 [`CheckpointingRunner`]，生产环境基于 Postgres，测试环境通过
/// [`Flow::new_for_test`] 切换为 in-memory backend。
///
/// 后端是枚举（两个具体类型不同），无法 `Deref` 到单一类型，因此
/// [`Flow::run`] / [`Flow::resume`] / [`Flow::with_conflict_policy`] /
/// [`Flow::backend`] 全部委托到当前后端。
#[derive(Clone)]
pub struct Flow {
    runner: Arc<FlowRunner>,
}

/// 流程引擎后端：统一两种实现，避免把后端类型泛型泄漏到调用方。
pub enum FlowRunner {
    /// 生产后端（Postgres 持久化）。
    Postgres(CheckpointingRunner<PostgresBackend<JsonCodec>>),
    /// 测试后端（进程内存储，随 `test-utils` 特性启用）。
    #[cfg(feature = "test-utils")]
    InMemory(CheckpointingRunner<InMemoryBackend>),
}

/// 底层后端引用（[`Flow::backend`] 的返回类型），用于与
/// `sayiir_runtime::WorkflowClient` 等共享同一后端。
#[derive(Clone)]
pub enum FlowBackend {
    /// Postgres 持久化后端。
    Postgres(Arc<PostgresBackend<JsonCodec>>),
    /// 进程内测试后端。
    #[cfg(feature = "test-utils")]
    InMemory(Arc<InMemoryBackend>),
}

impl Flow {
    pub async fn try_new(dsn: &str) -> Result<Self> {
        let backend = PostgresBackend::<JsonCodec>::connect(dsn).await?;
        Ok(Self {
            runner: Arc::new(FlowRunner::Postgres(CheckpointingRunner::new(backend))),
        })
    }

    #[cfg(feature = "test-utils")]
    pub fn new_for_test() -> Self {
        let backend = InMemoryBackend::new();
        Self {
            runner: Arc::new(FlowRunner::InMemory(CheckpointingRunner::new(backend))),
        }
    }

    /// 设置重复 instance_id 的冲突策略，返回共享同一后端的新 `Flow`。
    ///
    /// 默认（未设置时）为 [`ConflictPolicy::Fail`]。
    pub fn with_conflict_policy(&self, policy: ConflictPolicy) -> Self {
        let runner = match self.runner.as_ref() {
            FlowRunner::Postgres(runner) => FlowRunner::Postgres(
                CheckpointingRunner::from_shared(runner.backend().clone())
                    .with_conflict_policy(policy),
            ),
            #[cfg(feature = "test-utils")]
            FlowRunner::InMemory(runner) => FlowRunner::InMemory(
                CheckpointingRunner::from_shared(runner.backend().clone())
                    .with_conflict_policy(policy),
            ),
        };
        Self {
            runner: Arc::new(runner),
        }
    }

    /// 返回底层后端引用（如与 `WorkflowClient` 共享以发送信号）。
    pub fn backend(&self) -> FlowBackend {
        match self.runner.as_ref() {
            FlowRunner::Postgres(runner) => FlowBackend::Postgres(runner.backend().clone()),
            #[cfg(feature = "test-utils")]
            FlowRunner::InMemory(runner) => FlowBackend::InMemory(runner.backend().clone()),
        }
    }

    /// 从头运行工作流，每个 task 完成后自动保存 checkpoint。
    ///
    /// `instance_id` 唯一标识本次执行实例；重复 ID 的行为由
    /// [`with_conflict_policy`](Self::with_conflict_policy) 设置的策略控制。
    pub async fn run<C, Input, M>(
        &self,
        workflow: &Workflow<C, Input, M>,
        instance_id: impl Into<String>,
        input: Input,
    ) -> Result<WorkflowStatus>
    where
        Input: Send + 'static,
        M: Send + Sync + 'static,
        C: Codec
            + EnvelopeCodec
            + sealed::EncodeValue<Input>
            + sealed::DecodeValue<Input>
            + 'static,
    {
        let instance_id = instance_id.into();
        Ok(match self.runner.as_ref() {
            FlowRunner::Postgres(runner) => runner.run(workflow, instance_id, input).await?,
            #[cfg(feature = "test-utils")]
            FlowRunner::InMemory(runner) => runner.run(workflow, instance_id, input).await?,
        })
    }

    /// 从最近 checkpoint 恢复工作流。
    ///
    /// 快照不存在或 workflow 定义 hash 不匹配时返回错误。
    pub async fn resume<C, Input, M>(
        &self,
        workflow: &Workflow<C, Input, M>,
        instance_id: &str,
    ) -> Result<WorkflowStatus>
    where
        Input: Send + 'static,
        M: Send + Sync + 'static,
        C: Codec
            + EnvelopeCodec
            + sealed::EncodeValue<Input>
            + sealed::DecodeValue<Input>
            + 'static,
    {
        Ok(match self.runner.as_ref() {
            FlowRunner::Postgres(runner) => runner.resume(workflow, instance_id).await?,
            #[cfg(feature = "test-utils")]
            FlowRunner::InMemory(runner) => runner.resume(workflow, instance_id).await?,
        })
    }
}

impl Debug for Flow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flow").finish()
    }
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use sayiir_core::context::WorkflowContext;
    use sayiir_core::workflow::{ConflictPolicy, WorkflowBuilder, WorkflowStatus};
    use sayiir_runtime::serialization::JsonCodec;

    use super::{BoxError, Flow, FlowBackend, RuntimeError};

    fn build_workflow() -> sayiir_core::workflow::Workflow<JsonCodec, u32, ()> {
        let ctx = WorkflowContext::new("smoke", Arc::new(JsonCodec), Arc::new(()));
        WorkflowBuilder::new(ctx)
            .then("step1", |i: u32| async move { Ok::<u32, BoxError>(i + 1) })
            .build()
            .expect("build workflow")
    }

    #[tokio::test]
    async fn run_and_resume_via_flow() {
        let flow = Flow::new_for_test();
        let workflow = build_workflow();

        let status = flow.run(&workflow, "instance-1", 1u32).await.expect("run");
        assert!(matches!(status, WorkflowStatus::Completed));

        // 终态实例 resume 应幂等返回已完成的 status。
        let resumed = flow.resume(&workflow, "instance-1").await.expect("resume");
        assert!(matches!(resumed, WorkflowStatus::Completed));
    }

    #[tokio::test]
    async fn resume_unknown_instance_fails() {
        let flow = Flow::new_for_test();
        let workflow = build_workflow();

        let err = flow.resume(&workflow, "no-such-id").await.unwrap_err();
        assert!(err.downcast_current_context::<RuntimeError>().is_some());
    }

    #[tokio::test]
    async fn default_conflict_policy_fails_on_duplicate() {
        let flow = Flow::new_for_test(); // 默认 ConflictPolicy::Fail
        let workflow = build_workflow();

        flow.run(&workflow, "dup-1", 1u32).await.expect("first run");
        let err = flow.run(&workflow, "dup-1", 1u32).await.unwrap_err();
        assert!(matches!(
            err.downcast_current_context::<RuntimeError>(),
            Some(RuntimeError::InstanceAlreadyExists(_))
        ));
    }

    #[tokio::test]
    async fn use_existing_policy_reuses_snapshot() {
        let flow = Flow::new_for_test().with_conflict_policy(ConflictPolicy::UseExisting);
        let workflow = build_workflow();

        flow.run(&workflow, "dup-2", 1u32).await.expect("first run");
        // 不重新执行，直接返回当前 status。
        let status = flow
            .run(&workflow, "dup-2", 1u32)
            .await
            .expect("second run");
        assert!(matches!(status, WorkflowStatus::Completed));
    }

    #[tokio::test]
    async fn terminate_existing_policy_restarts() {
        let flow = Flow::new_for_test().with_conflict_policy(ConflictPolicy::TerminateExisting);
        let ctx = WorkflowContext::new("smoke", Arc::new(JsonCodec), Arc::new(()));
        let counter = Arc::new(AtomicU32::new(0));

        let workflow = WorkflowBuilder::new(ctx)
            .then("step1", {
                let counter = Arc::clone(&counter);
                move |i: u32| {
                    let counter = Arc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok::<u32, BoxError>(i + 1)
                    }
                }
            })
            .build()
            .expect("build workflow");

        flow.run(&workflow, "dup-3", 1u32).await.expect("first run");
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // TerminateExisting：删除旧快照并从头重新执行。
        let status = flow
            .run(&workflow, "dup-3", 1u32)
            .await
            .expect("second run");
        assert!(matches!(status, WorkflowStatus::Completed));
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn backend_exposes_shared_reference() {
        let flow = Flow::new_for_test();
        assert!(matches!(flow.backend(), FlowBackend::InMemory(_)));
    }
}
