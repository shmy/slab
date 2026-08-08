use appctx::AppCtx;
use sched_kit::{CronJob, CronJobFuture};

/// 清理缓存后端中的过期条目（pg 表 / redb 文件；redis 后端由服务端 TTL 处理，返回 0）。
pub struct KvGc;

impl CronJob<AppCtx> for KvGc {
    fn name(&self) -> &'static str {
        "kv_gc"
    }
    fn expr(&self) -> &'static str {
        "every 5 minutes"
    }
    fn run(&self, state: AppCtx) -> CronJobFuture {
        Box::pin(async move {
            let deleted = state.kv.delete_expired().await?;
            if deleted > 0 {
                tracing::info!(deleted, "kv_gc completed");
            } else {
                tracing::debug!("kv_gc: no expired cache rows");
            }
            Ok(())
        })
    }
}

/// 清理事件总线（pg 后端）中已投递的过期条目和孤儿 inbox。
pub struct BusGc;

impl CronJob<AppCtx> for BusGc {
    fn name(&self) -> &'static str {
        "bus_gc"
    }
    fn expr(&self) -> &'static str {
        "every 10 minutes"
    }
    fn run(&self, state: AppCtx) -> CronJobFuture {
        Box::pin(async move {
            let deleted = state
                .bus
                .delete_delivered_older_than(event_bus::DEFAULT_DELIVERED_RETENTION_DAYS)
                .await?;
            if deleted > 0 {
                tracing::info!(deleted, "bus_gc completed");
            } else {
                tracing::debug!("bus_gc: no old delivered event rows");
            }
            Ok(())
        })
    }
}
