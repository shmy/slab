use crate::{AppCtx, Blob, HttpClient, TokenBundle, TokenHelper, TokenRealm};
use cache::KvBackend;
use db::PgPool;
use flow::Flow;
use queue::QueueBackend;

/// 构建用于集成测试的 `AppCtx`。
///
/// 使用 in-memory Blob + 测试用 JWT helper。
/// 缓存后端：kv-redb → 临时文件 `RedbCache`；kv-redis → 直连本地 Redis
/// （需本机 Redis，否则构造失败）；默认 → `PgCache` 复用测试 PG 池。
#[allow(clippy::expect_used)]
pub async fn build(pg_pool: PgPool) -> AppCtx {
    let token_bundle = TokenBundle::new(
        TokenHelper::new_for_test_with_realm(TokenRealm::Customer),
        TokenHelper::new_for_test_with_realm(TokenRealm::Account),
    );
    let blob = Blob::new_for_test()
        .await
        .expect("create in-memory test Blob");
    let flow = Flow::new_for_test();
    let kv = {
        #[cfg(all(feature = "kv-pg", not(any(feature = "kv-redb", feature = "kv-redis"))))]
        {
            KvBackend::try_new(pg_pool.clone())
                .await
                .expect("create PG backend")
        }
        #[cfg(feature = "kv-redb")]
        {
            let dir = Box::leak(Box::new(
                tempfile::tempdir().expect("create temp cache dir"),
            ));
            KvBackend::try_new(dir.path().join("cache.redb")).expect("create redb backend")
        }
        #[cfg(feature = "kv-redis")]
        {
            KvBackend::try_new("redis://127.0.0.1:6379/0")
                .await
                .expect("create redis backend")
        }
    };
    // 队列后端：测试环境直接构造 Pg 变体（`try_new` 幂等建表，不依赖 NATS 实例）。
    let queue = QueueBackend::Pg(
        queue::PgBackend::try_new(pg_pool.clone())
            .await
            .expect("create PG queue backend"),
    );
    AppCtx {
        pg_pool,
        kv,
        queue,
        token_bundle,
        http_client: HttpClient::default(),
        blob,
        flow,
    }
}
