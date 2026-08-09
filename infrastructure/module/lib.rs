use appctx::AppCtx;
use event_bus::EventRegistry;
use futures_util::future::BoxFuture;
use rootcause::Result;
use sched_kit::CronScheduler;
use utoipa_axum::router::OpenApiRouter;
use worker::JobRegistry;

/// 模块注册上下文：收集各域需要注册的后台任务。
///
/// `DomainModule::register` 中被域模块填充，随后由 server 消费。
pub struct ModuleRegistrar {
    /// 事件订阅者注册表（消费上下文为 `AppCtx`）。
    pub events: EventRegistry<AppCtx>,
    /// 定时任务（cron）。
    pub scheduler: CronScheduler<AppCtx>,
    /// 后台任务（Job Queue）消费 handler 注册表（消费上下文为 `AppCtx`）。
    pub jobs: JobRegistry<AppCtx>,
}

impl ModuleRegistrar {
    pub fn new(app_state: AppCtx) -> Self {
        Self {
            events: EventRegistry::default(),
            scheduler: CronScheduler::new(app_state),
            jobs: JobRegistry::default(),
        }
    }
}

/// 每个业务域实现本 trait，server 端通过模块列表统一编排，
/// 新增域只需实现 trait + 在列表加一行。
///
/// 所有方法均有默认空实现，域按需覆盖。
pub trait DomainModule: Send + Sync {
    /// 模块标识，用于日志和启动顺序。
    fn name(&self) -> &'static str;

    /// 受保护路由（需鉴权）。
    fn protected_routing(&self) -> OpenApiRouter<AppCtx> {
        OpenApiRouter::new()
    }

    /// 未受保护路由（登录、刷新等无需鉴权的端点）。
    fn unprotected_routing(&self) -> OpenApiRouter<AppCtx> {
        OpenApiRouter::new()
    }

    /// 注册后台任务（队列消费 handler、定时任务等）。
    /// server 启动时调用一次，各域按需填入。
    fn register(&self, _registrar: &mut ModuleRegistrar) {}

    /// 启动前钩子（如种子数据、缓存预热）。
    fn on_start<'a>(&'a self, state: &'a AppCtx) -> BoxFuture<'a, Result<()>> {
        let _ = state;
        Box::pin(async { Ok(()) })
    }
}
