---
name: rust-backend
description: "Slab Rust 垂直切片细节。仅当新建域、新加 endpoint 文件、或架构落位不确定时读取；改已有 endpoint 的局部实现不要加载——根目录 AGENTS.md 已有约定。"
---

# rust-backend

**Trigger**: 新建域、新加 endpoint 文件、架构落位不确定。改已有 endpoint 的 `execute` / handler / 测试且不改 contract：不要读本文件。

## 架构与落位

### 工程组织

- **`features/shared_contract/`**：通用领域逻辑（ID、分页等）
- **`features/{domain}_contract/`**：实体、值对象、领域错误；`port/` 仅跨域只读（如 `AccountPort::by_id`，不含写方法；不在内核定义「输入 port」）
- **`features/{domain}/endpoint/`**：HTTP 动作（单文件切片；由同级 `endpoint.rs` 汇总，不用 `endpoint/mod.rs`）
- **`features/{domain}/repository/`**：本域持久化变更（`{Aggregate}Repository`；由 `repository.rs` + `repository/` 组织）。仅本域 `execute` 可 `use crate::repository::…`
- **`features/{domain}/subscriber/`**：（按需）域外队列 `queue` 的 `QueueHandler` 实现（当前 `identity` 有 `AccountCreatedHandler`）
- **`features/{domain}/shared/`**：（按需）跨 HTTP 切片复用的编排辅助，不演化为万能层
- **`infrastructure/`**：技术适配层，**禁止**写业务逻辑
  - `db`：数据库连接池
  - `blob`：对象存储（S3/COS）
  - `jwt`：JWT 令牌
  - `http_client`：HTTP 客户端
  - `web`：Axum 自定义提取器、响应与通用中间件
  - `http_auth`：鉴权中间件
  - `locale`：本地化
  - `cache`：热点 KV 缓存
  - `queue`：域外队列
  - `appctx`：全局应用上下文（`AppCtx`）
  - `feature`：模块注册系统
- **`libs/`**：与业务无关的内部 crate（`trace_kit`、`auth_kit`、`image_kit`、`sched_kit`）

### 跨域只读 Port 与 Repository

- `{domain}_contract::port`：**仅**跨域读/校验（如 `AccountPort::by_id`）；**禁止**在 kernel 放 `create` / `update` / `delete`
- `features/{domain}/repository/`：本域持久化变更（`{Aggregate}Repository`）；由 `repository.rs` + `repository/*.rs` 组织（不用 `repository/mod.rs`）
- 跨域依赖：他域 feature **只**依赖 `{other}_contract`，**不得**依赖 `features/{other}` 切片 crate
- 依赖方向：单向链（如 `file → identity_contract`），**禁止** `A → B → A`
- Port / Repository 均为 **struct + `pub async fn` + `&mut PgConnection`**；**不强制** `trait` / `dyn`
- 事件异步一致：`queue` + `subscriber/`；同步跨域用只读 Port
- 抽象策略：默认最少抽象；`bin/server` 保持薄

详见 `docs/ARCHITECTURE.md` §7。

## 切片文件标准结构（单文件）

按顺序：

1. Request/Path/Query DTO（`Deserialize` + `Validify` + `IntoParams` + `ToSchema`）
2. Response DTO（`Serialize` + `ToSchema`）
3. `#[utoipa::path]` + `handler`（`#[tracing::instrument]`）
4. `execute`（`#[tracing::instrument]` + `#[inline]`；大/敏感入参可用 `skip_all`）
5. 同文件 `mod tests`（优先集成测试）

### 目录约定

- HTTP 动作：`endpoint/{resource}_{action}.rs` + `endpoint.rs` 汇总（不用 `endpoint/mod.rs`）
- 写库变更：`repository/{aggregate}_repository.rs` + `repository.rs` 汇总（不用 `repository/mod.rs`）
- 跨 action 编排（入队、拼装）放 `shared/`；共用 SQL 变更放 `repository/`，不混进 `shared/`

### execute 读路径 vs 写路径

- **查询**：`execute` 直连 DB → 映射 Response；不用 `SELECT *`；静态 SQL 用 `sqlx::query! / query_as! / query_scalar!`，动态 SQL 用 SeaQuery + `sea_query_binder::SqlxBinder`
- **写入**：DTO → `Validify` + `*_contract` 值对象 → `execute` → `crate::repository::*Repository`；错误用 kernel 领域错误

## 校验与类型

- 使用各 Kernel 中的**值对象**约束业务不变量（如 `Account`、`HashedPassword`）
- 使用 `ValidJson<T>`、`ValidQuery<T>` 或 `ValidPath<T>`，配合 `validify` 做自动校验

## 类型与映射规范

- **手机号**：统一 `shared_contract::value_object::phone_number::PhoneNumber`
- **数字枚举**（如 Gender）：`serde_repr::{Serialize_repr, Deserialize_repr}` + `#[repr(i16)]`；DB 用 `sqlx::{Type, Decode, Encode}`（底层 i16）
- **Row → DTO**：字段类型均支持 `sqlx::Type/Decode` 时优先 `derive(FromRow)` 直映射

## 错误与 i18n

- domain/kernel 错误使用纯 l10n key（不加 `l10n:` 前缀），如 `#[error("customer_not_found")]`
- 通用值对象错误放 `shared.ftl`，领域错误放对应领域 ftl
- 新增错误 key 时 `zh-CN` 与 `en-US` 同步补齐
- 错误响应统一 RFC 9457 Problem Details（见 `bin/server/app.rs`）

## OpenAPI 与路由

- 每个 handler 补齐 `#[utoipa::path]` 元数据
- Response DTO 关键字段用 `///` 注释提升文档
- 受保护接口走 `routing()`；公开接口（如登录/刷新）走 `public_routing()`

## 执行与校验

- 开发时优先：`cargo check -p <member-crate>`
- 完成后至少检查受影响链路 crate（如 `shared_contract → pricing/order → server`）
- 文件含 `Validify`/`TryFromMultipart` 派生时**避免** `use rootcause::Result`，改显式 `rootcause::Result<T>`

### 编码规范

- **SQL 列表查询**：使用 **SeaQuery** 拼装；可选条件用 `and_where_option` 按需拼接 WHERE。禁止「把可选参数绑成 NULL 再在谓词里写 `($n::type IS NULL OR …)`」的可选参数哨兵模式（不利于计划缓存、可读差、易与真实 NULL 混淆）
- **Kernel Entity 边界**：`features/*_contract/entity/*` 不承载审计字段（`created_at`/`updated_at`）。审计时间若需对外返回，放 endpoint DTO 或查询投影层，不进入 kernel entity
- **错误处理**：业务与 `execute` 使用 `rootcause::Result<T>`。将 DB 错误映射为 kernel 领域错误。同文件有 `#[derive(Validify)]` 时不要 `use rootcause::Result`（遮蔽标准库 Result 导致宏展开失败）
- **可观测性**：每个 `handler` 与 `execute` 须加 `#[tracing::instrument]`；`execute` 另加 `#[inline]`。入参过大或不宜进 span 时用 `skip_all`（见 `file_upload_image`）

## 工程目标：AI 友好与快速编译

新代码默认对齐以下两个目标：

### AI 友好

- **单文件切片**：一个 `{action}.rs` 内尽量包含 DTO、utoipa、handler、execute、测试，减少跨文件跳转
- **蓝本优先**：新端点复制 `docs/templates/endpoint_template.rs` 再改
- **路径可预期**：领域不变量在 `*_contract`，技术细节在 `infrastructure/`；避免巨型 `pub use` 重导出链
- **查询与数据形状**：列表/搜索优先在同文件内用 SeaQuery 一段写完，避免逻辑碎在多处

### 快速编译

- **按 crate 检查**：日常用 `cargo check -p <crate>`（如 `-p identity`），避免全量编 `bin/server`
- **依赖收窄**：重依赖只出现在真正用到的 crate，避免经 `appctx` 等枢纽把无关 crate 拉进每个增量图
- **少动公共枢纽**：`infrastructure/web`、`appctx` 等被大量 member 依赖的 crate，非必要不频繁改其公开 API
- **过程宏**：utoipa、validify、serde 等是合理成本；通过依赖收窄控制谁被连带重编

## AI 协作说明

- **上下文**：实现新功能时，先查 `features/shared_contract` 是否已有通用类型
- **一致性**：对齐 `features/identity/endpoint/account_create.rs` 的写法与结构
- **命名**：域内子模块用 **`endpoint.rs` / `repository.rs` + 同名目录**；目录内**不要** `mod.rs`，实现文件用资源名（如 `account_repository.rs`）
- **重构**：若发现 `infrastructure/` 含业务逻辑，或 `features/*` 渗入过多技术细节，应建议迁到合适层次

## 参考蓝本

- 路由聚合：`bin/server/router.rs`
- 端点蓝本：`features/identity/endpoint/account_create.rs`
- 队列消费注册：`features/identity/lib.rs` 的 `register` 方法
- 共享手机号 VO：`features/shared_contract/value_object/phone_number.rs`
- 端点模板：`docs/templates/endpoint_template.rs`（含 4 种 Pattern）

## 域外队列（queue）

速查，详见 `docs/QUEUE.md`：

- Crate：`infrastructure/queue`；表：`queues`
- **入队**：在业务 Transaction 内调用 `enqueue_event`，与主写同事务提交，直接构造事件对象入队，无需中间桥接函数
- **消费**：`QueueHandler` 放 `subscriber/`；在该域 `register(&mut Registry)` 中挂到 `queue::Registry`
- **幂等**：at-least-once，`handle` 可能多次执行
- **短事务**：禁止在 handle 内长时间 IO
- **failed**：人工介入，不自动 GC
- **SQL 坑**：PostgreSQL `make_interval(secs => …)` 的 `secs` 是 `double precision`；退避用 `bigint * interval '1 second'`

## 热点 KV（cache）

速查，详见 `docs/CACHE.md`：

- Crate：`infrastructure/cache`；表：`caches`（`UNLOGGED`）
- 带 TTL 的共享 KV，可走 Transaction
- 禁止存放不可丢的业务主数据
- `set_ex`：`ON CONFLICT DO UPDATE`；`take`：`DELETE … RETURNING`（原子消费）
- GC 用独立 `pg_advisory_xact_lock` 锁（与 queue GC 不同 key）
- 禁止使用`async-trait`这个crate
- 必须使用`sqlx`的宏版本
