use crate::{AppCtx, Blob, HttpClient, TokenBundle, TokenHelper, TokenRealm};
use cache::Backend;
use db::PgPool;
use flow::Flow;

/// 构建用于集成测试的 `AppCtx`。
///
/// 使用 in-memory Blob + 测试用 JWT helper。
/// 缓存后端：redb 变体（临时文件）——测试不依赖外部服务；
/// 若编译进 `redis` 变体但未启用，仍以 Redb 变体为准。
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
    // 测试进程生命周期内保持临时目录（drop 过早会删除数据库文件）。
    let dir = Box::leak(Box::new(
        tempfile::tempdir().expect("create temp cache dir"),
    ));
    let kv = Backend::Redb(
        cache::RedbCache::open(dir.path().join("cache.redb")).expect("open redb test cache"),
    );
    AppCtx {
        pg_pool,
        kv,
        token_bundle,
        http_client: HttpClient::default(),
        blob,
        flow,
    }
}
