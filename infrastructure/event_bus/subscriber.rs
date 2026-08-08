use std::future::Future;
use std::pin::Pin;

use rootcause::Result;

/// 队列消费监听者。`C` 为消费上下文（如 `AppCtx`），`handle` 直接持有 `&C`——
/// handler 需要连接/其它能力时从上下文获取（如 `ctx.pg_pool.acquire()`）。
pub trait Subscriber<C: Send + Sync + 'static>: Send + Sync + 'static {
    fn topic(&self) -> &'static str;

    /// 监听者标识：同一 topic 下可注册多个 handler（广播），用 `name()` 区分。
    /// 是 `queue_deliveries.handler` 主键的一部分，也用于日志/告警。
    /// 默认取类型名；若类型名不可读（嵌套泛型等），可显式覆盖。
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// 处理一条消息。`ctx` 借用只存活于本次调用（future 生命周期内）。
    ///
    /// 同一消息在重试或崩溃恢复后可能再次进入 `handle`（at-least-once），实现须**幂等**。
    /// 保持逻辑短且可预期；勿做慢外部 IO（HTTP/S3 等），以免长时间占用消费路径。
    fn handle<'a>(
        &'a self,
        ctx: &'a C,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}
