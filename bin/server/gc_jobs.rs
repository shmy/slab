//! 系统内务任务（全部落库为 Job，`worker_jobs` 表即执行统计台账）。
//!
//! - `KvGc`：清理缓存后端过期条目（pg 表 / redb 文件；redis 后端由服务端 TTL 处理，返回 0）；
//! - `BusGc`：清理事件总线已投递的过期条目和孤儿 inbox；
//! - `JobGc`：清理超过保留期的终态（Done / Failed）Job 行——统计台账不无限膨胀。
//!
//! 内务语义通过 `RETRIES = 0` 保留：失败不重试（下个周期再跑，符合时间驱动语义），
//! 但终态 `Failed` + `last_error` 留痕，可统计失败率——**落库 ≠ 必须重试**。
//!
//! 三个任务都经 `register_gc_tasks` 注册（handler + cron 周期），由 server 组装：
//! cron 触发是 master-only，入队后由 worker 多进程竞争消费。

use appctx::AppCtx;
use job_queue::{DEFAULT_JOB_RETENTION_DAYS, Job};
use module::ModuleRegistrar;
use rootcause::Result;
use serde::{Deserialize, Serialize};

/// 清理缓存后端中的过期条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvGc {}

impl Job for KvGc {
    const NAME: &'static str = "kv_gc";
    const RETRIES: usize = 0; // 内务任务：失败下期再跑（原因见文件头 doc）
}

async fn handle_kv_gc(_job: KvGc, ctx: &AppCtx) -> Result<()> {
    log_gc_result(KvGc::NAME, ctx.kv.delete_expired().await?);
    Ok(())
}

/// 清理事件总线（pg 后端）中已投递的过期条目和孤儿 inbox。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusGc {}

impl Job for BusGc {
    const NAME: &'static str = "bus_gc";
    const RETRIES: usize = 0; // 内务任务：失败下期再跑（原因见文件头 doc）
}

async fn handle_bus_gc(_job: BusGc, ctx: &AppCtx) -> Result<()> {
    log_gc_result(
        BusGc::NAME,
        ctx.bus
            .delete_delivered_older_than(event_bus::DEFAULT_DELIVERED_RETENTION_DAYS)
            .await?,
    );
    Ok(())
}

/// 清理超过保留期的终态 Job 行（统计台账保留期管理）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobGc {}

impl Job for JobGc {
    const NAME: &'static str = "job_gc";
    const RETRIES: usize = 0; // 内务任务：失败下期再跑（原因见文件头 doc）
}

async fn handle_job_gc(_job: JobGc, ctx: &AppCtx) -> Result<()> {
    log_gc_result(
        JobGc::NAME,
        ctx.jobs
            .delete_finished_older_than(DEFAULT_JOB_RETENTION_DAYS)
            .await?,
    );
    Ok(())
}

/// 内务任务统一完成日志：删除 > 0 行记 info（含行数），0 行记 debug；job 名作字段记录。
fn log_gc_result(job: &'static str, deleted: u64) {
    if deleted > 0 {
        tracing::info!(job, deleted, "gc completed");
    } else {
        tracing::debug!(job, "gc: nothing to do");
    }
}

/// server 组装：注册内务任务（消费 handler + cron 周期触发，全部落库可统计）。
pub fn register_gc_tasks(r: &mut ModuleRegistrar) {
    r.jobs
        .register::<KvGc, _>(|job, ctx| Box::pin(handle_kv_gc(job, ctx)));
    r.jobs
        .register::<BusGc, _>(|job, ctx| Box::pin(handle_bus_gc(job, ctx)));
    r.jobs
        .register::<JobGc, _>(|job, ctx| Box::pin(handle_job_gc(job, ctx)));
    r.scheduled("every 5 minutes", KvGc {});
    r.scheduled("every 10 minutes", BusGc {});
    r.scheduled("0 0 4 * * *", JobGc {}); // 每天 04:00 清理（6 字段：秒 分 时 日 月 周）
}
