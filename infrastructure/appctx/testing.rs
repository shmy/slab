use crate::{AppCtx, Blob, HttpClient, TokenBundle, TokenHelper, TokenRealm};
use db::PgPool;
use event_bus::EventBus;
use flow::Flow;
use job_queue::JobBus;
use kv::KvBackend;

/// 构建用于集成测试的 `AppCtx`。
///
/// 使用 in-memory Blob + 测试用 JWT helper。
/// 缓存/事件总线均固定复用测试 PG 池（`new_for_test`）：各后端正确性由 kv/event_bus crate 单测
/// 覆盖，集成测试只断言业务行为、不依赖具体后端，任意 kv-* 组合下均可运行。
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
    let kv = KvBackend::new_for_test(pg_pool.clone())
        .await
        .expect("create test cache backend");
    let bus = EventBus::new_for_test(pg_pool.clone())
        .await
        .expect("create test event bus backend");
    let jobs = JobBus::try_new_pg(pg_pool.clone())
        .await
        .expect("create test job queue backend");
    AppCtx {
        pg_pool,
        kv,
        bus,
        jobs,
        token_bundle,
        http_client: HttpClient::default(),
        blob,
        flow,
    }
}
