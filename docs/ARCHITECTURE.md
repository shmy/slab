# Slab 后端架构总览

## 1. 总体形态

后端采用 **Rust Workspace + features 垂直切片**：

- 业务切片：`features/<domain>`（HTTP 端点集中在 **`endpoint/`** 子目录；队列消费若有，集中在 **`subscriber/`**）
- 领域内核：`features/<domain>_contract`
- 跨域共享：`features/shared_contract`
- 技术适配：`infrastructure/*`
- 程序入口：`bin/server`

这种组织的目标是：按业务域隔离演进，同时在单仓保持编译与联调效率。

### 1.1 路径命名（集合与成员）

- **复数**：仅用于「容纳多个 member crate 的顶层目录」，与常见 Workspace 习惯一致，例如 **`features/`**、**`libs/`**、**`infrastructure/`**。
- **单数**：其下的 **crate 名**（如 `identity`、`queue`、`trace_kit`）以及 **域内模块目录**（如 **`endpoint/`**、**`repository/`**、**`subscriber/`**、**`shared/`**），表示一类能力或命名空间，不因目录内文件多而改为复数。

## 2. 目录职责

### 2.1 业务域层（features）

- `identity`：账号管理与认证（登录、刷新、登出、密码管理）
- `file`：文件上传
- `health`：健康检查
- `item`：物料主数据（分类树、单位换算、成本）
- `customer` / `supplier`：客户 / 供应商主数据
- `warehouse`：仓库、库存、盘点、调拨
- `purchase`：采购订单 → 收货 → 退货 → 发票（含审批流）
- `quality`：IQC 检验模板/单/结果、不合格处理
- `sales`：销售订单 → 发货 → 退货 → 发票
- `product`：BOM、模具台账
- `production`：工单 → 领料 → 工序报工 → 完工入库 → 废品
- `planning`：再订货点预警、采购建议、MRP 计算（只读计算域）
- `finance`：收/付款（关联采购/销售发票）、账龄分析

每个域的 `lib.rs` 负责路由装配；HTTP 动作放在 **`endpoint/<resource>_<action>.rs`**（单文件承载 DTO、handler、`execute`、测试），由同级 **`endpoint.rs`** 汇总子模块。对本聚合的**持久化变更**（含 CRUD 与后续领域命令）放在 **`repository/`**（如 `AccountRepository`），由 **`repository.rs`** 汇总；跨动作编排辅助留在 **`shared/`**（如 `purchase` 审批流定义）。

**已知取舍（非缺陷，勿按统一模板强改）**：

- `health` 无 `health_contract`：纯基础设施探针，不需要业务契约。
- `file_contract` 无 entity/port：文件域只提供上传，`lib.rs` 直接承载路径策略与图片格式校验这些领域函数。
- `planning_contract` 只有 port 无 entity/error：planning 是只读计算域（MRP/建议），不产生自身实体。
- `product_contract` / `production_contract` 无 port：目前没有他域需要跨域读 `boms` / `molds` / `work_orders`，输出 port 按需添加。
- 大部分域不设 `repository/`：按 §6.1 约定，「同一域内 ≥2 个写端点共用同类 SQL / 变更逻辑」才抽；写端点各写各表时保持 execute 内联 SQL。`purchase` 是例外——4 个状态端点（submit/approve/reject/delete）共用锁定读 + 状态迁移，故有 `PurchaseOrderRepository`。

### 2.2 内核层（*_contract / shared_contract）

- `*_contract`：领域错误、值对象、实体、事件定义，以及 **「输出 port」**（见 **§7**）；不承载 Axum HTTP 适配逻辑。
- `shared_contract`：跨域复用的通用类型（如 ID、分页、部分值对象）及共享能力。

### 2.3 技术适配层（infrastructure）

- `db`：数据库连接与连接池（`PgPool`）
- `queue`：可插拔队列后端——Pg Outbox（默认）/ NATS JetStream（`docs/QUEUE.md`）
- `cache`：可插拔缓存后端——Pg UNLOGGED 表（默认）/ redb 嵌入式 / Redis（`docs/CACHE.md`）
- `blob`：对象存储（S3/COS）
- `jwt`：JWT 令牌生成/验证
- `http_client`：HTTP 客户端（reqwest）
- `web`：通用请求提取、响应封装、HTTP 错误结构
- `http_auth`：鉴权中间件与鉴权上下文提取
- `locale`：本地化中间件
- `appctx`：应用上下文（DI 容器）
- `migration`：数据库迁移
- `feature`：模块注册系统
- `approval`：审批状态机（跨域深度模块，见 §7.6）
- `inventory_ledger`：库存台账（跨域深度模块，见 §7.6）

该层通常只做技术接入，不承载业务决策。但少数组件（`inventory_ledger`、`approval`、`costing`）例外——它们是跨域深度模块，封装了多个业务域共享的业务不变量（库存不足校验、审批状态迁移），以「浅接口 + 深实现」模式减少重复。这属于有意识的设计取舍，见 §7.6。

## 3. 请求链路

全局入口在 `bin/server/app.rs`，主链路如下：

1. 聚合业务路由：`identity` → `file`
2. 叠加鉴权中间件（业务受保护路由）
3. 合并 `identity::public_routing()`（登录/刷新）
4. 叠加 locale、限流、OTel 观测
5. 合并 `health::public_routing()`
6. 可选挂载 Scalar 文档页
7. 外层统一超时保护

健康检查与业务链路分开，避免探针被业务中间件干扰。

## 4. 当前能力快照

| 域 | 能力 | 状态 |
|----|------|------|
| `identity` | 账号 CRUD/搜索 + 登录/刷新/登出/密码管理 | ✅ |
| `file` | 图片上传 | ✅ |
| `health` | 存活/就绪/健康探针 | ✅ |
| `item` | 物料 CRUD、分类树、单位换算、成本 | ✅ |
| `customer` / `supplier` | 客户 / 供应商 CRUD | ✅ |
| `warehouse` | 仓库、库存流水、盘点、调拨 | ✅ |
| `purchase` | 采购订单→收货→退货→发票 + 审批流 | ✅ |
| `quality` | 检验模板/单/结果、不合格处理 | ✅ |
| `sales` | 销售订单→发货→退货→发票 | ✅ |
| `product` | BOM 多级展开、模具台账 | ✅ |
| `production` | 工单→领料→报工→完工入库→废品 | ✅ |
| `planning` | 再订货点预警、采购建议、MRP 计算 | ✅ |
| `finance` | 收/付款、账龄分析 | ✅ |

## 5. 数据与事件

### 5.1 已有迁移覆盖（`infrastructure/migration/versions/`）

| 版本 | 内容 |
|------|------|
| `0001_create_foundations` | 基础表（customers/suppliers/items/warehouses 等） |
| `0002_create_account_table` | `accounts` |
| `0003_create_erp_foundations` | ERP 主数据扩展 |
| `0004_create_p2a_foundations` | 采购/质检/库存（purchase_orders、inspection_*、inventories 等） |
| `0005_alter_inventories_quantity_bigint` | inventories 数量改 bigint |
| `0006_create_sales_tables` | 销售（sales_orders/deliveries/returns/invoices） |
| `0007_create_production_tables` | 生产（work_orders、production_receipts 等） |
| `0008_create_p4_foundations` | 财务/计划（payments、item_costs 等） |

另含基础设施表：`queues` + `queue_deliveries`（域外广播队列，消息本体 + 监听者投递状态，默认队列后端 `PgBackend` 使用，见 `docs/QUEUE.md`）、`caches`（`UNLOGGED` 热点 KV + TTL，默认缓存后端 `PgCache` 使用，见 `docs/CACHE.md`）。

### 5.2 事件消费现状

- 事件定义：`identity_contract::events` 定义 `AccountCreatedEvent` / `AccountLoggedInEvent`（实现 `shared_contract::event::Event`）。
- 入队：`identity` 域在 `account_create`（立即）与 `account_login`（延迟 10s）内与业务同事务入队。
- 消费：`identity/subscriber/` 下 `AccountCreatedHandler` 与 `AccountLoggedInHandler` 各一个（当前仅观测打日志，无副作用），在 `identity::Module::register` 注册。
- 队列系统（dispatcher + Inbox 幂等）就绪，dispatcher 在 server 进程内运行；后续业务域的消费端按需添加。

## 6. 开发约定（面向后续迭代）

1. 新增端点优先沿用「单动作单文件」模板，文件落在 **`features/<domain>/endpoint/<action>.rs`**（与现有域对齐）。
2. 业务规则进入 `*_contract` 或域切片，不进入 `infrastructure`。
3. 列表/搜索查询优先使用 SeaQuery 组合条件，避免可选参数哨兵式 SQL。
4. 日常增量校验按 crate 执行 `cargo check -p <crate>`，降低全量编译成本。
5. HTTP 方法语义：**部分字段更新默认 `PATCH`**；`PUT` 仅用于「整体替换」或语义明确的「子资源整体替换」。

### 6.1 Endpoint：`execute` 读路径与写路径

**查询类**（单条 GET、列表、搜索等）

- `execute` 内 **直连数据库**（SeaQuery、`query` / `query_opt` 等），把结果映射成 **Response DTO** 或列表项；**不必**为对外展示先构造完整「富领域对象」再转出。
- **数据库访问约定**：统一使用 `sqlx`。除非确实需要**动态 SQL**（例如条件拼接、可选过滤等，推荐 SeaQuery + `sea_query_binder::SqlxBinder`），否则默认一律使用带宏的 `sqlx::query! / sqlx::query_as! / sqlx::query_scalar!`（静态 SQL + 编译期检查）。
- **避免 `SELECT *`**：优先 **显式列出列** 或使用与 schema 对齐的 **`FromRow`** 等映射，便于迁移与避免无意暴露列。
- 分页、筛选、排序留在 **Query DTO + `Validify`**；需要他域只读数据时可用 **`*_contract::port`** 或本域 SQL（按 §7）。

**写入类**（创建、更新、删除及后续非 CRUD 持久化命令）

- 典型路径：**请求 DTO** → **`Validify` + `*_contract` 值对象** → **`execute` 编排** → **`crate::repository::*Repository`** 写库（或仅一处写库时 SQL 可暂留 `execute` 内，见下）。
- **何时抽 `repository/`**：同一域内 **2 个及以上** 写端点共用同类 SQL / 变更逻辑时，抽到 **`features/{domain}/repository/{aggregate}_repository.rs`**；结构为 **`repository.rs`**（`pub(crate) mod …`）+ **`repository/`** 子目录（**不用** `repository/mod.rs`）。
- **命名**：类型 **`{Aggregate}Repository`**（如 `AccountRepository`），方法仍用动词（`create` / `update` / `delete` / `update_status` 等）；**不含** HTTP、鉴权、发队列——这些留在 `execute`。
- 失败映射到 **`*_contract` 领域错误**，再统一为 Problem Details。
- 「走 domain」指 **用内核类型与不变量约束写入与业务错误**，不等于每笔都要上完整 DDD 聚合；**简单 CRUD** 保持 **校验 + 一次写库** 即可，复杂不变量再抽到 `*_contract` 的函数或实体逻辑。

**`handler` / `execute` 属性（可观测与内联）**

- **`handler`**：`#[tracing::instrument]`。
- **`execute`**：`#[tracing::instrument]` + **`#[inline]`**（与 `handler` 同文件、同 crate，薄编排便于被内联）。入参过大或不宜进 span 时，`execute` 可用 `#[tracing::instrument(skip_all)]`（如 `file_upload_image`）。

### 6.2 HTTP 方法约定（更新语义）

- **默认规则**：凡是「更新部分字段/局部状态」，使用 **`PATCH`**。
- **`PUT` 使用边界**：
  - 明确替换整个资源表示；
  - 或替换语义明确的完整子资源（例如资源下的固定子对象整体替换）。
- **当前对齐**：更新类端点（`PATCH /api/v1/accounts/{id}` 等）统一走 PATCH；「提交/审批/驳回/作废」等状态动作端点用 `POST .../{id}/submit`、`/approve`、`/reject` 子路径，不用 PUT。

## 7. 跨域依赖、只读 Port 与切片内 Repository

单体里「甲域编排里要调乙域能力」时，用 **依赖方向一致** 的链式 crate 依赖；**禁止** crate 依赖成环。

### 7.1 三层落位（垂直切片内）

| 层次 | 落位 | 职责 | 谁可依赖 |
|------|------|------|----------|
| **只读 Port** | `{domain}_contract::port`（如 `AccountPort::by_id`） | 跨域 **SELECT / 存在性校验** | 他域 feature crate |
| **Repository** | `features/{domain}/repository/`（如 `AccountRepository`） | 本域 **持久化变更**（CRUD、改状态、归档等） | **仅** 本域 `features/{domain}` 内 `endpoint` / `shared` |
| **编排** | `features/{domain}/endpoint/*` 的 `execute` | DTO 校验、调 Port/Repository、事务、入队 | — |

### 7.2 核心原则

1. **`{domain}_contract` 只定义「输出 port」**（本域**提供给谁用**的跨域 API）。`features/{domain}_contract/port/` **仅保留读/校验**（如 `by_id`）；**不写** `create` / `update` / `delete`。
2. **`{domain}_contract` 不定义「输入 port」**：本域依赖谁由 **feature 的 `execute`** 或 `bin/server` 组合（直接 `use` 他域 kernel 的**只读** API）。
3. **本域写库只在切片 crate 内**：`features/{domain}/repository/{aggregate}_repository.rs` + 根目录 **`repository.rs`** 声明子模块；**不**单独建 `{domain}_repository` workspace member。
4. **跨域不得依赖他域切片 crate**：feature 的 `Cargo.toml` 只依赖他域 `*_contract`，**不得**依赖他域 feature crate。
5. **单向依赖**：**禁止** `A → B → A`。

### 7.3 实现形态

- **Port / Repository** 均采用 **空 struct + `pub async fn` + `&mut sqlx::PgConnection`**（或 `tx.as_mut()`），由 `execute` 传入连接以共事务。
- **Port 组织**：统一为单文件 **`{domain}_contract/port.rs`**（port 及其专属值对象同文件，如 `finance_contract::port::{InvoicePort, InvoiceType}`）；调用方直接 `use {domain}_contract::port::{Domain}Port`。**不建** `port/` 子目录。
- **audit 例外（跨域同事务写）**：`audit_contract::AuditService`（动词方法 `record_create` / `record_updated` / `record_deleted`）是唯一的跨域写入口，写 SQL 在 `lib.rs` 而非 `port.rs`——arch_test 规则 5 禁止 port 文件出现写 SQL，`port.rs` 只承载只读 Port。调用方传 `&mut txn` + `&Operator`（`shared_contract::value_object::operator`）+ before/after 实体。
- **Repository 按需创建**：仅当「同一域内 ≥2 个写端点共用同类 SQL / 变更逻辑」时建 `features/{domain}/repository/{aggregate}_repository.rs` + 根目录 `repository.rs`；**不要**留空占位或为单写端点预建。
- **勿用 Cargo feature 门控同一 kernel 的读写**：workspace 内多 crate 共用 `{domain}_contract` 时 Cargo 会对 feature **取并集**，**无效**。
- **CI 兜底**：`bin/server/arch_test.rs`（`cargo test -p server arch_test`）自动验证依赖方向：contract 不得依赖其他 contract / 不得依赖 infrastructure / feature runtime 不得依赖 runtime / infra 不得依赖 feature runtime / port 文件不得出现写 SQL。
- **不强制** `trait` / `Arc<dyn …>`；多实现或单测替身再在局部引入。

### 7.4 代价与边界（避免误用）

- Port / Repository 直接依赖 `sqlx` 的 `PgConnection`，**绑定 PostgreSQL**；业务以 PG 为主存，接受这一取舍。
- **异步事件**（最终一致）仍优先 **`queue` + 他域 `subscriber/`**（见 **§5.2**）；**同步跨域** 用 **只读 Port + 同连接**，勿跨域调 Repository。

### 7.5 抽象策略（少即是多）

- **默认最少抽象**：能用 **关联函数 / 小 struct + 明确参数** 表达跨域能力，就不要先起一层 `trait`、`Arc<dyn …>`、通用 DI 容器或「全接口化」目录结构。
- **禁止默认对标 Java 式堆叠**：不为「理论上可替换」提前做工厂、多层 Port/Adapter 命名仪式；**真有**多实现、单测替身、插件边界时，再在**该点**引入 `trait` 等。
- **`bin/server` 组合根保持薄**：路由与中间件拼装为主，不把业务编排压进「启动器里的百万行装配」。

### 7.6 跨域深度模块

少数场景下，一个模块的业务不变量被 **多个域共享**（如库存增减、单据审批），且它们本质上是写操作，不适合放入只读 Port。这些模块放在 `infrastructure/` 并标记为**深度模块**，遵守以下约定：

| 模块 | 位置 | 封装的不变量 | 使用者 |
|------|------|------------|--------|
| `inventory_ledger` | `infrastructure/inventory_ledger/` | 库存 UPSERT + FOR UPDATE + 流水记录 + 不足校验 | sales, purchase, production, warehouse |
| `approval` | `infrastructure/approval/` | 单据 submit/approve/reject 状态迁移规则 | purchase, sales, warehouse |
| `costing` | `infrastructure/costing/` | 加权平均成本重算 | purchase |

**原则：**

1. **浅接口，深实现**：接口（`InventoryLedger::issue`、`StateTransitions::approve_status`）简单稳定，内部封装复杂的不变量组合。接口即测试面。
2. **无业务域偏袒**：深度模块不应隐含某一业务域的特定语义；差异由调用方通过参数（`reference_type`、自定义 `TransactionType`）表达。
3. **可删除性**（Deletion Test）：如果删除该模块，相关域必须各自重新实现相同逻辑——这是「深」的标志。
4. **豁免规则**：feature crate 可以依赖 `infrastructure/` 下的深度模块，但必须通过 contract 的端口间接使用（即仍禁止依赖其他 feature 的 runtime crate）。
5. **有限数量**：整个代码库中深度模块的数量应保持在个位数。每新增一个，应检查是否真的「跨域 + 写 + 有不变量」，还是可以拆回各域。
