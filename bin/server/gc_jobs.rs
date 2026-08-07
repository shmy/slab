use appctx::AppCtx;
use sched_kit::{CronJob, CronJobFuture};

/// 清理缓存后端中的过期条目（pg 表 / redb 文件；redis 后端由服务端 TTL 处理，返回 0）。
pub struct CacheGc;

impl CronJob<AppCtx> for CacheGc {
    fn name(&self) -> &'static str {
        "cache_gc"
    }
    fn expr(&self) -> &'static str {
        "every 5 minutes"
    }
    fn run(&self, state: AppCtx) -> CronJobFuture {
        Box::pin(async move {
            let deleted = state.kv.delete_expired().await?;
            if deleted > 0 {
                tracing::info!(deleted, "cache_gc completed");
            } else {
                tracing::debug!("cache_gc: no expired cache rows");
            }
            Ok(())
        })
    }
}

/// 清理 pg_queue 中已投递的过期条目和孤儿 inbox。
pub struct QueueGc;

impl CronJob<AppCtx> for QueueGc {
    fn name(&self) -> &'static str {
        "queue_gc"
    }
    fn expr(&self) -> &'static str {
        "every 10 minutes"
    }
    fn run(&self, state: AppCtx) -> CronJobFuture {
        Box::pin(async move {
            let deleted = state
                .queue
                .delete_delivered_older_than(queue::DEFAULT_DELIVERED_RETENTION_DAYS)
                .await?;
            if deleted > 0 {
                tracing::info!(deleted, "queue_gc completed");
            } else {
                tracing::debug!("queue_gc: no old delivered queue rows");
            }
            Ok(())
        })
    }
}
