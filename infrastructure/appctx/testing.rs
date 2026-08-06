use crate::{AppCtx, Blob, HttpClient, TokenBundle, TokenHelper, TokenRealm};
use cache::Backend;
use db::PgPool;
use flow::Flow;

/// 构建用于集成测试的 `AppCtx`。
///
/// 使用 in-memory Blob + 测试用 JWT helper。
/// 缓存后端：`redb` feature 下用临时文件 `RedbCache`；否则（含 `redis` feature，
/// 测试环境无 Redis 实例）退回 `PgCache` 复用测试 PG 池。
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
            Backend::try_new(pg_pool.clone())
                .await
                .expect("create PG backend")
        }
        #[cfg(feature = "kv-redb")]
        {
            let dir = Box::leak(Box::new(
                tempfile::tempdir().expect("create temp cache dir"),
            ));
            Backend::try_new(dir).expect("create redb backend")
        }
        #[cfg(feature = "kv-redis")]
        {
            Backend::try_new("redis://127.0.0.1:6379/0")
                .await
                .expect("create redis backend")
        }
    };
    AppCtx {
        pg_pool,
        kv,
        token_bundle,
        http_client: HttpClient::default(),
        blob,
        flow,
    }
}
