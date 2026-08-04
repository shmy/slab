use std::future::Future;
use std::pin::Pin;

use rootcause::Result;
use sqlx::PgConnection;

pub trait QueueHandler: Send + Sync + 'static {
    fn topic(&self) -> &'static str;

    /// 监听者标识：同一 topic 下可注册多个 handler（广播），用 `name()` 区分。
    /// 是 `queue_deliveries.handler` 主键的一部分，也用于日志/告警。
    /// 默认取类型名；若类型名不可读（嵌套泛型等），可显式覆盖。
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// 同一消息在重试或崩溃恢复后可能再次进入 `handle`（at-least-once），实现须**幂等**。
    ///
    /// 保持逻辑短且可预期；勿在批处理事务路径内做慢外部 IO（HTTP/S3 等），以免长时间占用连接与同批其它 topic。
    fn handle<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}
