---
name: rust-tests
description: Slab Rust 后端测试编写规范（集成测试 + Hurl E2E）
---

# rust-tests

**Trigger**: 当用户要求为 Slab（slab）Rust 后端的端点、队列消费、pg_queue/pg_cache 新增或补充测试时加载本 skill。也适用于编写 Hurl E2E、增加覆盖率、回归测试或验证 execute/handler 行为。

## 前置条件

- **数据库**：`#[sqlx::test]` 需要可连接的 PostgreSQL（通过 `DATABASE_URL` 环境变量）
- **crate 依赖**：被测 crate 的 `[dev-dependencies]` 应包含 `migration`、`shared_contract`（features = `["test-utils"]`）、`tokio`（`macros` + `rt-multi-thread`）
- **蓝本**：参考 `features/identity/Cargo.toml`
- **Hurl E2E**：依赖本机 Hurl 8.x、已启动的 server、同迁移后的 PostgreSQL

## 标准做法：同文件 `mod tests`

1. 在 `features/{domain}/endpoint/{action}.rs` 末尾使用 `#[cfg(test)] mod tests { ... }`
2. 使用 `#[sqlx::test]` 注入 `PgPool`，测试开头调用 `migration::run_migrations(&pool)`
3. 用 `shared_contract::testing::app_state::build(pool).await` 构建 `AppState`
4. 优先直接测 `execute`（与 handler 逻辑一致时），必要时再测 handler
5. **准备数据**：小函数 `seed_*` 或用域内共享 `#[cfg(test)] pub(crate) mod tests`（如 `identity` 的 `insert_test_account`）
6. **断言**：
   - 返回体：`execute(...).unwrap()` 或对 `Report` 匹配领域错误
   - 数据库：`query_one` / `query` + `row.get(...)` 或 `derive(FromRow)` 映射
   - 队列：查 `queues` 的 `topic`、`payload`、`status`
7. **错误分支**：对 `SqlState::UNIQUE_VIOLATION` 等映射的 domain error 写独立用例
8. **副作用边界**：NATS、外部 HTTP 等使用 mock；队列消费测幂等与短事务约束

## HTTP E2E：Hurl（`e2e/`）

- **入口**：`just e2e`（按固定顺序执行 `.hurl`，文件间 `sleep 2` 防 GovernorLayer 429）
- **变量**：`e2e/env`（Java properties 格式）
- **调试单文件**：`hurl --test --variables-file e2e/env e2e/identity.hurl`
- **详细调试**：`hurl --verbose --variables-file e2e/env e2e/identity.hurl`

### 编写规范

- 每个场景一个或多个 `.hurl` 文件；断言用 `[Asserts]` + `jsonpath`，链式流程用 `[Captures]`
- 覆盖 HTTP 契约、中间件、端到端旅程（与 `cargo test` 的 execute 级测试互补）
- 边界用例固定 `Accept-Language: en-US` 以便断言英文 `detail`
- 变更种子管理员、路由、错误码或 Fluent 文案时同步更新 `e2e/*.hurl` 和 `docs/E2E_HURL.md`

详见 `docs/E2E_HURL.md`。

## 运行

| 命令 | 用途 |
|------|------|
| `cargo test -p <crate_name>` | 单个 crate 测试 |
| `cargo test -p <crate> -- <test_name>` | 单个测试 |
| `just e2e` | HTTP E2E（需先启动 server） |
| `hurl --test --variables-file e2e/env e2e/identity.hurl` | 单文件 Hurl |

## 检查清单

- [ ] 新测是否经过真实迁移后的 schema（通过 `build()` 已满足）？
- [ ] 是否覆盖至少一条非成功路径（重复键、未找到、校验失败等）？
- [ ] 写队列的事件是否断言了 topic + payload（若该动作负责入队）？
- [ ] 测试结束是否依赖容器 drop 清理（无需手工删表）？
- [ ] 若改了 Hurl 用例或 `e2e/env`：是否已跑通 `just e2e`，并更新 `docs/E2E_HURL.md`？
- [ ] 若改了 l10n（尤其 en-US）且影响 Identity E2E 中断言的 `detail`：是否同步 `e2e/identity.hurl`？

## 与 sqlx 的关系

静态 SQL 优先用 `sqlx::query! / query_as! / query_scalar!`，动态查询按需使用 SeaQuery + `SqlxBinder`。
