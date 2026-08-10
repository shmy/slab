//! 周期任务桥接：cron 到点触发 → 入队一个 Job。
//!
//! `sched_kit` 是纯触发器（时间驱动、进程内、master-only），`job_queue` 是执行引擎
//! （命令式、重试退避 / 超时 / 终态、多进程竞争消费）。[`ScheduledJob`] 把两者接起来：
//! 每个 cron tick 只做一次 `enqueue`，之后的重试 / 超时 / 终态留痕 / 幂等全部由
//! job_queue 承担——业务侧只需定义 `Job` + handler，再一行注册周期触发。

use appctx::AppCtx;
use job_queue::Job;
use sched_kit::{CronJob, CronJobFuture};

/// cron 表达式 → 周期入队的桥接任务。
///
/// # 语义
///
/// - **触发**：master 进程（`server_master`）内的 cron 调度器到点调用 [`CronJob::run`]；
/// - **执行**：`run` 只做 `context.jobs.enqueue(job.clone())`，入队后由 worker 进程竞争
///   消费（重试 / 超时 / 终态 / 幂等全归 job_queue，与一次性任务同一套语义）；
/// - **堆积**：每次 tick 都入队一个新实例，上一轮未消费完会排队堆积（时间驱动语义，
///   与 cron 一致）；需要"上一轮未完成则跳过本 tick"时，在 handler 内用幂等守卫 /
///   KV 标记自行控制。
#[derive(Clone, Debug)]
pub struct ScheduledJob<T: Job + Clone> {
    expr: &'static str,
    job: T,
}

impl<T: Job + Clone> ScheduledJob<T> {
    /// `expr` 为 tokio-cron-scheduler 表达式（**6 字段**：秒 分 时 日 月 周，如
    /// `"0 0 3 * * *"`；或英文缩写 `"every 5 minutes"`），在调度器 `start` 时校验，
    /// 非法表达式使 server 启动失败（fail fast）。
    pub fn new(expr: &'static str, job: T) -> Self {
        Self { expr, job }
    }
}

impl<T: Job + Clone> CronJob<AppCtx> for ScheduledJob<T> {
    fn name(&self) -> &'static str {
        T::NAME
    }

    fn expr(&self) -> &'static str {
        self.expr
    }

    fn run(&self, context: AppCtx) -> CronJobFuture {
        let job = self.job.clone();
        Box::pin(async move {
            context.jobs.enqueue(job).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use sqlx::{PgPool, Row};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct FakeJob {
        n: i64,
    }

    impl Job for FakeJob {
        const NAME: &'static str = "fake_job";
    }

    #[test]
    fn name_and_expr_are_passed_through() {
        let job = ScheduledJob::new("0 0 3 * * *", FakeJob { n: 1 });
        assert_eq!(job.name(), "fake_job");
        assert_eq!(job.expr(), "0 0 3 * * *");
    }

    /// 端到端：ScheduledJob::run（cron tick 的触发动作）→ JobBus::enqueue →
    /// worker_jobs 落库一行 Pending，payload 可反序列化为原 Job（类型擦除在入队侧）。
    #[sqlx::test]
    async fn run_enqueues_job_row(pool: PgPool) {
        let ctx = appctx::testing::build(pool.clone()).await;
        let job = ScheduledJob::new("0 0 3 * * *", FakeJob { n: 7 });

        job.run(ctx).await.unwrap();

        let row = sqlx::query("SELECT job_type, payload FROM worker_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("job_type"), "fake_job");
        assert_eq!(
            row.get::<serde_json::Value, _>("payload"),
            serde_json::json!({ "n": 7 })
        );
    }
}
