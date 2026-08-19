# Slab 制造企业管理系统 — 反向推导 PRD

> **文档性质**：由代码库反向推导的产品需求文档（Reverse PRD）。
> **依据**：代码（`features/`、`cross_domain/`、`infrastructure/`、`frontend/`）、数据库迁移（`infrastructure/migration/versions/*.sql`）、API 定义（`features/*/endpoint/*.rs`）、E2E 测试（`e2e/*.hurl`）、GitNexus 调用链分析。
> **标注约定**：
> - ✅ **已实现**：代码中存在完整实现 + 测试
> - 🔶 **部分实现**：存在骨架/单点，缺闭环
> - 🧩 **隐藏功能**：代码中已存在但当前无业务调用方
> - 💡 **推测**：由命名/结构推断，未经端到端验证
> - 🚀 **建议**：反向推导后建议的产品方向（第五阶段）
>
> 生成时间：2026-08-16。仓库 commit：`8486815`。

---

# 第一阶段：代码库理解

## 1.1 项目整体架构

**一句话**：Slab 是一个"模块化单体（Modular Monolith）"架构的制造业 ERP 后端 + React 管理后台 SPA，所有业务域以"垂直切片 + Contract 接缝"组织，理论上可拆分为微服务。

```
┌─────────────────────────────────────────────────────────────────┐
│                         frontend/（React 19 SPA）                │
│  登录/客户管理/用户管理/个人资料（已实现）+ 占位页面              │
│  多标签页 keep-alive、虚拟表格、RSQL 筛选栏、审计历史抽屉         │
└───────────────▲─────────────────────────────────────────────────┘
                │ Bearer JWT（access + refresh 双令牌，401 单飞刷新）
┌───────────────┴─────────────────────────────────────────────────┐
│                    bin/server（组装点：Axum 0.8）                │
│  中间件：JWT 鉴权（account/customer 双 realm）→ 本地化 → 路由     │
│  内务任务：KV/EventBus/Job GC + 指标采样（job_queue cron）        │
└───────┬──────────────────┬──────────────────┬───────────────────┘
        │                  │                  │
┌───────▼────────┐ ┌───────▼────────┐ ┌───────▼────────────────────┐
│  features/     │ │  cross_domain/ │ │  infrastructure/           │
│  业务切片×15   │ │  共享业务深模块 │ │  db / web / jwt / http_auth│
│  （每域 =      │ │  approval 审批  │ │  locale / event_bus / flow │
│  contract +    │ │  costing 成本   │ │  job_queue / kv / blob /   │
│  endpoint +    │ │  doc_numbering │ │  migration / module /      │
│  repository）  │ │  inventory_    │ │  appctx                    │
│                │ │  ledger 台账    │ │                            │
└───────┬────────┘ └────────────────┘ └────────────┬───────────────┘
        │                                          │
┌───────▼──────────────────────────────────────────▼───────────────┐
│              PostgreSQL（40+ 表、12 个编码序列、Outbox）          │
│  事件表/任务表：event_outbox、worker_jobs、kv_store、flow 表等    │
└──────────────────────────────────────────────────────────────────┘
```

## 1.2 技术栈

| 层 | 选型 | 依据 |
|---|---|---|
| 语言/运行时 | Rust（stable）、Tokio multi-thread | `Cargo.toml`、README |
| HTTP 框架 | Axum 0.8 + utoipa（OpenAPI） | 各 endpoint `#[utoipa::path]` |
| 数据库 | PostgreSQL + sqlx 0.9（SQL 内联编译期检查） | `infrastructure/migration/versions/*.sql` |
| 查询构建 | SeaQuery（列表/搜索） | `cursor_page::paginate` 说明文档 |
| 鉴权 | JWT 双 realm（account 员工 / customer 客户预留）+ KV 存 jti 校验撤销 | `infrastructure/http_auth/`、`infrastructure/jwt/token_realm.rs` |
| 本地化 | Fluent（en-US / zh-CN，错误 key 双语翻译） | `infrastructure/locale/`、`shared.ftl` |
| 事件 | Pg Outbox（默认）/ NATS JetStream，广播式 | `infrastructure/event_bus` |
| 流程编排 | sayiir 持久化工作流引擎 | `infrastructure/flow`（🧩 业务未接入） |
| 后台任务 | 自研 job_queue（入队/延迟/重试/终态） | `infrastructure/job_queue` |
| 缓存 | Pg UNLOGGED（默认）/ redb / Redis 可插拔 | `infrastructure/kv` |
| 对象存储 | 腾讯云 COS（默认）/ 本地文件系统 | `infrastructure/blob` |
| 可观测 | OpenTelemetry OTLP（10+ 指标埋点） | `libs/trace_kit` |
| 前端 | React 19 + Rsbuild + TanStack Router/Store/Table v9 + shadcn/ui + Tailwind 4 + xior | `frontend/` |

## 1.3 核心模块列表

注册在 `bin/server/modules.rs` 的 16 个模块：

| # | 模块 | 端点数 | 职责 |
|---|---|---|---|
| 1 | `identity` | 11 | 账号、登录/登出/刷新/改密/重置密码 |
| 2 | `item` | 14 | 物料主数据、分类树、单位换算、成本 |
| 3 | `customer` | 5 | 客户档案 CRUD |
| 4 | `supplier` | 5 | 供应商档案 CRUD |
| 5 | `purchase` | 14 | 采购订单/收货/退货/发票 + 审批流 |
| 6 | `sales` | 5 | 销售订单/发货/发票 + 审批流 |
| 7 | `warehouse` | 13 | 仓库、库存、盘点、调拨 |
| 8 | `production` | 8 | 生产工单全生命周期（含领料/报工/完工） |
| 9 | `product` | 5 | BOM（物料清单）、模具台账 |
| 10 | `planning` | 3 | MRP 净需求、采购建议、再订货预警 |
| 11 | `quality` | 7 | 检验模板、检验单（IQC/IPQC/OQC）、不合格处理 |
| 12 | `finance` | 6 | 应收/应付余额、账龄、利润表、收付款 |
| 13 | `file` | 1 | 图片上传（COS/本地） |
| 14 | `audit` | 1 | 变更历史查询（读时算 diff） |
| 15 | `health` | 3 | 健康检查（livez/readyz/healthz） |
| 16 | `shared_contract` | — | 共享内核：ID、游标分页、值对象（非注册模块） |

跨域共享业务深模块（`cross_domain/`）：`approval`（审批状态机）、`costing`（加权平均成本）、`doc_numbering`（单据编码）、`inventory_ledger`（库存台账）。

## 1.4 模块依赖关系

```
features/{domain}（切片）
    ├──→ {domain}_contract          ← 自己的公共表面（实体/Port/事件/错误）
    ├──→ 其他 *_contract            ← 跨域只读 Port（如 planning 用 finance）
    └──→ cross_domain/              ← approval / costing / doc_numbering / inventory_ledger
         └──→ *_contract（值对象）
infrastructure/ ← 被所有切片依赖（db/web/http_auth/locale/...）
```

**跨域通道**（架构核心约束，`AGENTS.md` 依赖规则）：
- **只读 Port**：名词方法（如 `PlanningPort::mrp_calculate`、`InvoicePort::unpaid_aging`），跨域读数据。
- **同事务写 Port 例外**：`audit_contract::AuditService`（`record_create/record_updated/record_deleted`）供所有写端点同事务留痕；`inventory_ledger` 被销售/采购/生产/仓库 4 域直写。
- **事件**：`shared_contract::event::Event` trait + Pg Outbox。当前仅 `AccountCreatedEvent`、`AccountLoggedInEvent` 两个事件。

## 1.5 核心调用链（GitNexus 验证）

**跨域写样板**（以采购收货 `features/purchase/endpoint/purchase_receipt_create.rs` 为例）：
```
handler → execute
  ├─ begin txn
  ├─ SELECT purchase_orders FOR UPDATE（校验 Approved）
  ├─ DocNumberer::next_number("seq_purchase_receipt", "RCV")  → RCV-20260816-000001
  ├─ INSERT purchase_receipts / purchase_receipt_lines（校验不超收 received_qty）
  ├─ UPDATE purchase_order_lines.received_qty（FOR UPDATE）
  ├─ InventoryLedger::receive(...)           → UPSERT inventories + 写流水
  ├─ CostCalculator::recalc_weighted_average(...) → 翻转旧成本 + 插入新均价
  ├─ AuditService::record_create(...)        → audit_logs（同事务）
  └─ commit
```

**MRP 净需求计算**（`features/planning_contract/port.rs`）：
```
sales_order_lines(未关闭) JOIN sales_orders JOIN boms JOIN bom_items
  → 毛需求 gross_demand
  → 减当前库存 current_stock、减在途采购 in_transit_qty（已审批未收货）
  → 净需求 net_demand = GREATEST(毛需求 − 库存 − 在途, 0)
  → 建议采购量 suggested_order_qty = 净需求 + 安全库存
```

---

# 第二阶段：业务反推

## 2.1 产品定位（💡 推测，依据充分）

**Slab 是一款面向中小型制造企业的"小而全"ERP 后端系统**，覆盖**进销存（采购/销售/库存）+ 生产执行 + 质量管理 + 计划排产（MRP）+ 财务管控**的完整业务闭环。

代码依据：
- 物料类型枚举覆盖完整制造业词汇：原材料/自制/外购/半成品/成品/包材/消耗品（`item_contract/entity/item.rs:ItemType`）；
- 业务单据覆盖"采购→生产→销售"主链路，且存在 `inventory_ledger`（库存台账）、`costing`（加权平均成本）、`doc_numbering`（单据编码）、`approval`（审批流）四个跨域共享深模块——这是制造业 ERP 的核心骨架；
- E2E 测试分阶段命名 `erp_foundations → p2_purchase_sales → p3_production → p4_finance_planning → p5_cost_finance_mrp`，暗示开发按 ERP 业务线推进；
- 仓库类型含"原料仓/半成品仓/成品仓/包材仓/消耗品仓"（`0003_create_erp_foundations.sql`），典型制造企业仓库形态。

**一句话定位**：*面向制造业中小企业的模块化 ERP 后端平台，聚焦"以库存为核心、BOM 为纽带、审批为管控"的产供销一体化。*

## 2.2 目标用户（💡 推测）

| 画像 | 依据 |
|---|---|
| 制造业中小企业（塑料/五金/电子组装等离散制造） | 物料含原料/成品、模具台账、BOM、工单、MRP 的组合 |
| 多币种多仓库企业（当前仅 CNY 为主） | `currency VARCHAR(3) DEFAULT 'CNY'`，金额以"分"存储 |
| 需要审批管控的单体/集团企业 | 采购/销售/退货/盘点/调拨均走 submit→approve 审批流 |
| 有质检需求的工厂 | IQC/IPQC/OQC 检验模板 + 不合格处理（NC） |

## 2.3 用户角色（✅ 已实现的部分 + 💡 推测）

| 角色 | 权限模型 | 实现状态 |
|---|---|---|
| **系统管理员** | 内置 admin（e2e 默认手机号 13888888888） | ✅ 账号管理可创建/删除/改密 |
| **特权账户** | `accounts.privileged = true`，仅特权账户可管理特权账户 | ✅ `account_delete.rs:67` 校验 `target.privileged` 时要求操作者特权 |
| **普通员工** | 登录后可访问所有业务端点（当前**无功能级 RBAC**） | ✅ 基础，⚠️ 无细粒度授权 |
| **客户/供应商** | JWT `TokenRealm::Customer` 预留客户门户 realm | 🧩 中间件已存在（`http_auth/middleware/authorize.rs` 双 realm），无业务端点使用 |
| **审批人** | 无独立角色概念；审批动作人人可调（仅状态机约束） | 🔶 状态机有"主管审批中"状态，但无审批人指派 |

> ⚠️ **关键发现**：`libs/authz_kit`（Cedar 授权策略）已存在但**零调用方**（grep 无业务引用）；`privileged` 是当前唯一的权限维度。权限体系属"部分实现"。

## 2.4 核心业务场景

1. **采购闭环**：物料缺料 → 创建采购订单 → 审批 → 收货入库（自动加权平均成本 + 库存台账）→ 采购退货 → 采购发票 → 付款。
2. **销售闭环**：客户下单 → 销售订单 → 审批 → 发货出库（扣减库存）→ 销售发票 → 收款。
3. **生产执行**：BOM 定义 → 工单下达 → 领料 → 工序报工 → 完工入库（+ 废品登记，💡 表存在）。
4. **计划排产**：销售需求 → MRP 展开 BOM → 净需求计算 → 采购建议 + 再订货预警。
5. **库存治理**：初始建账 → 盘点（账面 vs 实盘 → 差异调整）→ 调拨（跨仓移动）。
6. **质量控制**：检验模板 → 来料/过程/出货检验 → 不合格处理（退货/返工/特采）。
7. **财务管控**：应收/应付余额、账龄分析、月度利润表、收付款登记。

## 2.5 主要业务流程（状态机汇总）

**统一审批流**（`cross_domain/approval/lib.rs` `StateTransitions`）：
```
Draft(0) ──submit──► Submitted(1) ──approve──► PendingSupervisor(2) ──approve──► Approved(3)
   ▲                        │  ▲                     │
   └──────── reject(4) ◄────┴──┴─────────────────────┘
```
- **两步审批**（采购订单 PO_FLOW）：Draft→Submitted→PendingSupervisor→Approved/Rejected（`purchase_contract/value_object.rs:PurchaseOrderStatus`）
- **单步审批**（销售订单、采购退货 RET_FLOW、盘点、调拨）：Draft→Submitted→Approved/Rejected（无中间态）

**工单生命周期**（`production_contract`）：Draft → Released → InProgress → Completed → Closed（`work_order_release.rs` 校验 Draft→Released）。

**检验单**：待检 → 已检（结论 Verdict：pass/fail/conditional 为独立维度，`quality_contract/value_object.rs` 注释明确"已检≠通过"）。

---

# 第三阶段：功能模块分析

## 3.1 身份与账号（identity）

- **目标**：系统登录认证与账号生命周期管理。
- **用户角色**：系统管理员、所有员工。
- **核心功能**：
  - ✅ 手机号+密码登录（`account_login.rs`），JWT access+refresh 双令牌
  - ✅ 登出（撤销 token：`account_logout.rs`，KV 删除 jti）
  - ✅ 令牌刷新（`account_refresh_token.rs`）
  - ✅ 修改密码（`account_update_password.rs`）/ 管理员重置密码（`account_reset_password.rs`）
  - ✅ 账号 CRUD + 搜索（`account_create/update/delete/get/search.rs`），特权账户保护（`account_delete.rs:67`）
  - ✅ 当前用户自省（`profile_current.rs`，前端登录后拉取）
  - 🧩 双 realm：`TokenRealm::Customer` 客户令牌域已实现中间件，无业务端点
- **业务流程**：登录 → 签发 access/refresh → 业务请求带 Bearer → KV 校验 jti 撤销态 → 401 时前端单飞刷新。
- **涉及数据模型**：`accounts`（含 privileged、version 乐观锁）。
- **相关 API**：`/api/v1/identity/login|logout|refresh|password`、`/api/v1/accounts*`、`/api/v1/profile/current`。
- **前端页面**：`routes/login.tsx`、`routes/_app/profile.tsx`、`routes/_app/settings/users.tsx` ✅。
- **实现状态**：✅ 已完成（含事件 `AccountCreatedEvent`/`AccountLoggedInEvent`，前端已接入）。

## 3.2 物料主数据（item）

- **目标**：统一物料档案，为进销存/生产/BOM 提供基础数据。
- **用户角色**：物料管理员/工程部。
- **核心功能**：
  - ✅ 物料分类树（`item_category_create/update/delete/tree.rs`）
  - ✅ 物料 CRUD + 搜索（code/name 模糊 + RSQL 筛选，`item_search.rs`）
  - ✅ 单位换算（`item_unit_create/list.rs`，rate 换算比）
  - ✅ 成本管理：手动/参考/最新采购/加权平均 4 类成本（`item_cost_create/list.rs`、`item_weighted_cost_get.rs`），加权平均由采购收货自动重算（`cross_domain/costing`）
  - ✅ 7 类物料类型（原料/自制/外购/半成品/成品/包材/消耗品）
  - ✅ 安全库存、再订货点（`0008_create_p4_foundations.sql` 扩展列）
- **涉及数据模型**：`item_categories`、`items`（version 乐观锁）、`item_units`、`item_costs`（is_current 快照链）。
- **相关 API**：`/api/v1/items*`、`/api/v1/item-categories*`、`/api/v1/items/{id}/units|costs`。
- **实现状态**：✅ 已完成（后端），⚠️ 前端无物料页面。

## 3.3 客户 / 供应商档案（customer / supplier）

- **目标**：客商主数据管理。
- **核心功能**：✅ CRUD + 搜索（编码/名称/电话/联系人模糊，软删除 `is_active`），带 RSQL 筛选与审计历史。
- **涉及数据模型**：`customers`、`suppliers`。
- **前端页面**：`routes/_app/customers/index.tsx` ✅（客户管理页是前端标杆页：无限滚动 + 筛选 + 详情抽屉 + 审计历史 + 编辑表单）；供应商页面 ❌ 未实现。
- **实现状态**：✅ 后端已完成；客户前端 ✅，供应商前端 ❌。

## 3.4 采购管理（purchase）

- **目标**：采购订单、收货、退货、发票全流程 + 两级审批。
- **核心功能**：
  - ✅ PO 创建/查询/删除（草稿可删）/提交/审批/驳回（`purchase_order_*`，状态机 `PO_FLOW`）
  - ✅ 收货（`purchase_receipt_create.rs`）：校验订单已批准、行不超收 → 写库存台账 + 重算加权平均成本 + 变更历史，单事务
  - ✅ 采购退货（`purchase_return_*`）：关联收货行，审批后 `InventoryLedger::force_issue`（允许负库存出库）
  - ✅ 采购发票（`purchase_invoice_create/get.rs`）：金额/税额/总金额，关联 PO
- **业务流程**：见 2.4 采购闭环。
- **涉及数据模型**：`purchase_orders(+lines)`、`purchase_receipts(+lines)`、`purchase_returns(+lines)`、`purchase_invoices`。
- **相关 API**：`/api/v1/purchase-orders*`、`/api/v1/purchase-receipts*`、`/api/v1/purchase-returns*`、`/api/v1/purchase-invoices*`。
- **实现状态**：✅ 后端已完成；⚠️ 前端无页面；⚠️ 无采购订单行级"未收货完成自动关闭"、无收货单审核环节。

## 3.5 销售管理（sales）

- **目标**：销售订单、发货、发票流程 + 审批。
- **核心功能**：
  - ✅ SO 创建/查询/提交/审批（单步：`sales_order_submit/approve.rs`）
  - ✅ 发货（`sales_delivery_create.rs`）：校验订单已批准 → `InventoryLedger::issue`（库存不足报 `insufficient_inventory`）
  - ✅ 销售发票（`sales_invoice_create.rs`）
  - 🔶 销售退货：**数据表存在**（`sales_returns(+lines)`），**无端点**（e2e/端点清单无 sales_return）
  - 🔶 发货/发票无明细查询端点（仅有创建）
- **涉及数据模型**：`sales_orders(+lines)`、`sales_deliveries(+lines)`、`sales_returns(+lines)`、`sales_invoices`。
- **实现状态**：🔶 部分完成（缺退货、缺查询列表、缺收款与发票核销联动端点）。

## 3.6 仓库与库存（warehouse）

- **目标**：库存台账统一管理 + 盘点/调拨治理。
- **核心功能**：
  - ✅ 仓库 CRUD（5 类仓库）
  - ✅ 库存查询（`inventory_search.rs`，含 CAST 数量排序）、初始建账（`inventory_initial.rs`）
  - ✅ 库存流水（`inventory_transaction_search.rs`，7 类交易类型）
  - ✅ 盘点：创建→提交→审批→差异调整（`inventory_check_*`，`InventoryLedger::adjust`）
  - ✅ 调拨：创建→提交→审批→双仓移动（`stock_transfer_*`，`InventoryLedger::transfer` = issue+receive）
  - ✅ 跨域深模块 `inventory_ledger`：FOR UPDATE + UPSERT + 流水，4 域共用（含负库存出库 force_issue）
- **涉及数据模型**：`warehouses`、`inventories`（quantity/locked_qty/version）、`inventory_transactions`、`inventory_checks(+items)`、`stock_transfers(+items)`。
- **实现状态**：✅ 后端已完成；⚠️ 前端无页面；⚠️ locked_qty（锁定库存）字段存在但未见业务占用（推测为预留）。

## 3.7 产品结构（product：BOM + 模具）

- **目标**：产品配方（BOM）与模具资产台账。
- **核心功能**：
  - ✅ BOM 创建/查询/发布（`bom_create/get/release.rs`，draft→released→obsolete，支持多级 `parent_item_id`、损耗率万分比）
  - ✅ 模具台账：创建/查询（`mold_create/get.rs`，腔数、寿命/已用模次、保养周期）
  - 🔶 模具保养记录：表存在（`mold_maintenance`），无端点
- **涉及数据模型**：`boms(+items)`、`molds`、`mold_maintenance`。
- **实现状态**：🔶 部分完成（缺 BOM 编辑/版本管理、缺模具保养 CRUD、缺 BOM 详情行查询）。

## 3.8 生产执行（production）

- **目标**：工单驱动的车间执行与完工入库。
- **核心功能**：
  - ✅ 工单创建/查询/下达（`work_order_create/get/release.rs`：BOM 展开生成物料需求 `work_order_materials`）
  - ✅ 领料（`work_order_material_pick.rs`：`InventoryLedger::issue`，MaterialPick 类型）
  - ✅ 工序报工（`work_order_operation_report.rs`：工序完成数量上报）
  - ✅ 完工入库（`production_receipt_create.rs`：`InventoryLedger::receive`）
  - ✅ 工单物料成本（`work_order_material_cost_get.rs`）
  - 🔶 废品登记：表存在（`scrap_records`），无端点
  - 💡 推测：工单状态机完整（Draft→Released→InProgress→Completed→Closed），但"报工自动推进工单状态"未端到端验证
- **涉及数据模型**：`work_orders`、`work_order_materials`、`work_order_operations`、`production_receipts`、`scrap_records`。
- **实现状态**：🔶 后端主要闭环已实现；⚠️ 前端无页面；⚠️ 无工单排程/产能维度。

## 3.9 计划排产（planning）

- **目标**：需求驱动的物料计划（MRP 雏形）。
- **核心功能**：
  - ✅ MRP 净需求计算（`planning/mrp`，SQL CTE 口径单一事实源）
  - ✅ 采购建议（`purchase_suggestion_list.rs`）
  - ✅ 再订货预警（`reorder_alert_list.rs`）
- **涉及数据模型**：只读聚合（items + sales_order_lines + boms + inventories + purchase_order_lines）。
- **实现状态**：✅ 后端核心算法完成（含完备测试：净需求下限、等号边界）；⚠️ 前端无页面；⚠️ 无"建议→一键转 PO"闭环、无多级 BOM 递归展开（当前仅一层）。

## 3.10 质量管理（quality）

- **目标**：来料/过程/出货检验 + 不合格处理。
- **核心功能**：
  - ✅ 检验模板（`inspection_template_create/get.rs`，IQC=1/IPQC=2/OQC=3）
  - ✅ 检验单创建/查询/完成（`inspection_order_*`：逐项记录结果 → 汇总判定 pass/fail/conditional）
  - ✅ 不合格处理（`non_conformance_create/get.rs`：严重度 critical/major/minor、处置 return/rework/accept）
  - 🔶 NC 审批（`status` 字段为 ApprovalStatus 但无 approve 端点）
  - 💡 推测：检验单与采购收货/生产/仓库的联动（source_type 字段）未端到端验证
- **涉及数据模型**：`inspection_templates(+items)`、`inspection_orders`、`inspection_results`、`non_conformances`。
- **实现状态**：🔶 部分完成（缺检验单查询列表、缺 NC 审批闭环、缺检验结论自动驱动库存放行）。

## 3.11 财务管理（finance）

- **目标**：应收应付与经营报表。
- **核心功能**：
  - ✅ 应收/应付余额（`finance/balances`：未收/未付汇总）
  - ✅ 账龄分析（`finance/aging`：0-30/31-60/61-90/90+ 分桶）
  - ✅ 利润表（`finance/income-statement`：收入-支出=毛利，按月）
  - ✅ 收付款登记（`payment_create/get/search.rs`：AR/AP 两类，联动发票 paid_amount）
- **涉及数据模型**：`payments`、`sales_invoices`、`purchase_invoices`（paid_amount 累计）。
- **实现状态**：🔶 部分完成（后端报表已实现；⚠️ 无总账/凭证/科目体系，无应收应付明细账，⚠️ 前端无页面）。

## 3.12 文件服务（file）

- **目标**：图片上传。
- **核心功能**：✅ 图片上传（multipart，2MiB 限制，压缩 `image_kit`，COS/本地可插拔），💡 推测后续用于物料图片/检验附件。
- **实现状态**：✅ 基础完成（无查询/删除端点）。

## 3.13 变更历史（audit）

- **目标**：所有写操作留痕，字段级 diff。
- **核心功能**：
  - ✅ 跨域同事务写 Port `AuditService`（identity/purchase/sales/warehouse/production/quality 等写端点均已接入）
  - ✅ 查询（`audit_search.rs`：按实体过滤 + LEFT JOIN 操作人 + 读时算 diff，快照 JSONB）
- **涉及数据模型**：`audit_logs`（before/after JSONB、action、ip、user_agent）。
- **实现状态**：✅ 完成；前端已实现通用 `AuditHistory` 抽屉组件（`components/AuditHistory.tsx`）。

## 3.14 系统健康与可观测（health + 基础设施）

- ✅ `/livez` `/readyz` `/healthz`（健康检查）
- ✅ 内务 Job：KV/EventBus/Job GC + 积压指标采样（`bin/server/internal_jobs.rs`）
- ✅ OpenTelemetry OTLP 三信号
- 🧩 `flow` 工作流引擎（sayiir）已建好但**无业务流程使用**（`grep flow:: features/` 无命中）
- 🧩 事件总线仅 2 个身份事件，无业务域事件
- 🧩 `authz_kit`（Cedar）零调用方

---

# 第四阶段：PRD（标准格式）

# 产品概述

## 产品定位

**Slab 制造企业管理系统**：面向中小型制造企业的产供销一体化 ERP 平台。以**物料主数据**为基础、**库存台账**为核心、**BOM+工单**为生产纽带、**审批流**为管控手段、**MRP**为计划大脑，覆盖采购、销售、生产、仓储、质量、财务六大业务域，提供统一的单据编码、变更历史审计与多语言支持。

产品形态：REST API（OpenAPI 契约）+ React 管理后台 SPA（Web）。

## 用户角色

| 角色 | 说明 | 权限 |
|---|---|---|
| 系统管理员 | 内置 admin 账号 | 全部功能 + 账号管理 + 特权账户管理 |
| 特权账户 | 被授权的管理账户 | 可管理特权账户（删除/重置密码） |
| 普通员工 | 业务操作员 | 全部业务功能（当前无功能级授权） |
| 客户/供应商 | （预留） | 客户 realm 门户（未启用） |
| 审批人 | （概念） | 审批动作，无独立指派 |

## 核心价值

1. **闭环**：采购/销售/生产/仓储/质量/财务六域单据全链路贯通，库存与成本自动联动。
2. **管控**：统一两级审批流 + 全量变更历史（字段级 diff），操作可追溯。
3. **计划**：MRP 净需求计算自动给出采购建议与缺料预警。
4. **质量**：检验模板化 + 不合格处置流程，覆盖 IQC/IPQC/OQC。
5. **工程化**：模块化单体架构、OpenAPI 契约、双语本地化、可插拔基础设施，可平滑演进微服务。

## 功能模块

1. 身份与账号
2. 物料主数据
3. 客商档案
4. 采购管理
5. 销售管理
6. 仓库与库存
7. 产品结构（BOM/模具）
8. 生产执行
9. 计划排产（MRP）
10. 质量管理
11. 财务管理
12. 文件服务
13. 变更历史（系统级）

## 详细需求

### 功能 1：账号与认证

- **功能名称**：账号管理 / 登录认证
- **使用角色**：系统管理员、全体员工
- **使用场景**：员工登录系统；管理员维护账号与密码。
- **功能描述**：手机号+密码登录签发 JWT 双令牌；账号 CRUD；修改/重置密码；登出撤销令牌。
- **操作流程**：登录（POST /identity/login）→ 携带 Bearer 访问业务 API → 401 自动刷新（POST /identity/refresh）→ 登出（POST /identity/logout）。
- **输入**：手机号（11 位）、密码；账号：姓名/手机号/备注。
- **输出**：access_token + refresh_token；账号信息（含 privileged 标志）。
- **业务规则**：
  - 特权账户（`privileged=true`）只能由特权账户创建/删除/重置密码（`account_delete.rs:67`、`account_reset_password.rs:70` 保护校验）。
  - 令牌以 KV 存 jti，登出/刷新后旧令牌即失效（`authorize.rs` 校验 jti 一致性）。
  - 账号删除采用软删除（搜索接口过滤）。
- **异常情况**：手机号已存在 → `phone_already_exists`；密码错误 → 401 `invalid_credentials`；令牌撤销 → 401 `access_token_revoked`。
- **实现状态**：✅ 已实现；⚠️ 无验证码/2FA/登录失败锁定。

### 功能 2：物料主数据

- **功能名称**：物料档案 / 分类 / 单位 / 成本
- **使用角色**：物料管理员、采购、生产、仓库、财务
- **使用场景**：建立物料档案并分类；维护单位换算与成本价格。
- **功能描述**：物料 CRUD（7 类物料类型）、分类树、单位换算、四类成本维护（手动/参考/最新采购/加权平均）。
- **操作流程**：建分类 → 建物料（选类型/单位/安全库存）→ 维护单位换算 → 维护成本。
- **输入**：编码（自动序列规则 `seq_item_*`）、名称、类型、基础单位、规格、安全库存、再订货点。
- **输出**：物料卡片数据；分类树。
- **业务规则**：
  - 编码唯一；软删除（`is_active`）。
  - 加权平均成本由采购收货自动重算（`CostCalculator::recalc_weighted_average`），历史成本留痕（is_current 快照链）。
  - 更新采用版本号乐观锁（`version` 冲突 → 409 `*_version_conflict`）。
- **异常情况**：编码重复 → 唯一约束错误；分类不存在 → 400。
- **实现状态**：✅ 后端已实现；⚠️ 前端无页面。

### 功能 3：客商档案

- **功能名称**：客户/供应商管理
- **使用角色**：销售、采购、财务
- **使用场景**：维护客户与供应商基础信息，供订单引用。
- **功能描述**：CRUD + 模糊搜索（编码/名称/电话/联系人）+ RSQL 筛选 + 变更历史 + 软删除。
- **操作流程**：列表 → 新建/编辑 → 保存（自动留痕）。
- **业务规则**：`is_active` 软删除；编码唯一。
- **实现状态**：✅ 后端 + 客户前端完成；❌ 供应商前端未实现。

### 功能 4：采购管理

- **功能名称**：采购订单 / 收货 / 退货 / 发票
- **使用角色**：采购员、仓库、财务、审批人
- **使用场景**：向供应商采购物料，收货入库，处理退货与发票结算。
- **功能描述**：PO 全生命周期（草稿→提交→主管审批→批准/驳回）；收货自动入账（库存+成本+流水）；退货出库；发票登记。
- **操作流程**：
  1. 创建 PO（引用供应商+物料行）→ 提交 → 审批（两步）→ 批准
  2. 收货（校验不超收，写库存台账 + 加权平均成本重算，同事务）
  3. 退货（关联收货行，审批后负库存出库）
  4. 开票（登记金额/税额）
- **输入**：供应商、行（物料/数量/单价/单位）、收货行（仓库/批次/实际成本）、退货原因。
- **输出**：PO/收货单/退货单/发票，统一编码 `PO-20260816-000001` / `RCV-...`。
- **业务规则**：
  - 收货仅允许已批准 PO（`OrderNotApproved` 错误）。
  - 单行累计收货不可超订单数量（`purchase_receipt_create.rs` 校验 received_qty）。
  - 审批状态机非法迁移 → `invalid_status_transition`。
  - 退货允许库存不足出库（`force_issue`）。
- **异常情况**：PO 未批准收货、超收、审批状态非法、库存不足。
- **实现状态**：✅ 后端已实现；⚠️ 前端无页面；⚠️ 无收货审核、无自动关单。

### 功能 5：销售管理

- **功能名称**：销售订单 / 发货 / 发票
- **使用角色**：销售员、仓库、财务、审批人
- **使用场景**：客户下单、发货出库、开票结算。
- **功能描述**：SO 全生命周期（草稿→提交→批准）；发货扣库存；发票登记。
- **操作流程**：创建 SO → 提交 → 批准 → 发货（校验库存）→ 开票。
- **业务规则**：发货仅允许已批准订单；库存不足 → `insufficient_inventory`；发货行不可超未发数量。
- **异常情况**：库存不足、订单未批准、超发。
- **实现状态**：🔶 部分实现（✅ 主链路；❌ 销售退货无端点、发票无列表/详情查询、无收款核销端点）。

### 功能 6：仓库与库存

- **功能名称**：库存台账 / 盘点 / 调拨
- **使用角色**：仓管员、审批人
- **使用场景**：查询实时库存与流水；定期盘点调差；仓库间调拨。
- **功能描述**：库存查询（物料×仓库）；初始建账；盘点（创建→提交→审批→按差异调整库存）；调拨（创建→提交→审批→源仓出库+目标仓入库）。
- **操作流程**：见 2.4 场景 5。
- **业务规则**：
  - 库存变更统一走 `InventoryLedger`（FOR UPDATE 行锁 + UPSERT + 流水记录），杜绝散落 SQL。
  - 调拨审批通过时源仓出库（`TransferOut`）+ 目标仓入库（`TransferIn`）同事务。
  - 盘点差异为 0 时跳过写入。
- **异常情况**：调拨/发货库存不足；盘点单状态非法。
- **实现状态**：✅ 后端已实现；⚠️ 前端无页面；🧩 `locked_qty` 预留未用。

### 功能 7：产品结构（BOM/模具）

- **功能名称**：BOM 管理 / 模具台账
- **使用角色**：工程部
- **使用场景**：定义成品配方（含多级子件与损耗）；管理模具资产与寿命。
- **功能描述**：BOM 创建→发布→作废（支持多级、损耗率）；模具台账（腔数、寿命/已用模次、保养周期）。
- **业务规则**：MRP 仅展开已发布 BOM（`b.status = 1`）；BOM 发布后状态不可回退。
- **实现状态**：🔶 部分实现（✅ BOM 建/查/发布、模具建/查；❌ BOM 编辑/版本管理、模具保养记录端点）。

### 功能 8：生产执行

- **功能名称**：工单管理
- **使用角色**：车间主管、操作工、仓管员
- **使用场景**：按 BOM 下达生产任务、领料、工序报工、完工入库。
- **功能描述**：工单创建（BOM 展开物料需求）→ 下达 → 领料（扣库存）→ 工序报工 → 完工入库（+物料成本统计）。
- **操作流程**：建工单 → 下达（Draft→Released）→ 领料（MaterialPick 流水）→ 报工（按工序）→ 完工入库（Inbound 流水）。
- **业务规则**：领料/完工走统一库存台账；工单状态机严格校验。
- **实现状态**：🔶 主要闭环已实现；❌ 废品登记端点缺失、无前端页面、无排程。

### 功能 9：计划排产（MRP）

- **功能名称**：MRP 净需求 / 采购建议 / 缺料预警
- **使用角色**：计划员、采购员
- **使用场景**：根据未关闭销售订单 + BOM 展开计算原料净需求，输出采购建议与再订货预警。
- **功能描述**：毛需求 = Σ(SO 行量 × BOM 用量)；净需求 = max(毛需求 − 当前库存 − 在途采购, 0)；建议采购量 = 净需求 + 安全库存。
- **业务规则**：
  - 仅统计未关闭 SO 行与已发布 BOM。
  - 在途口径 = 已审批未收货 PO 行（`po.status >= 3 AND quantity > received_qty`）。
  - 净需求下限 0（GREATEST）；安全库存不足也触发预警。
- **异常情况**：无（只读计算）。
- **实现状态**：✅ 算法已实现（测试完备）；❌ 无"建议转 PO"闭环、无多级 BOM 递归展开、无前端。

### 功能 10：质量管理

- **功能名称**：检验模板 / 检验单 / 不合格处理
- **使用角色**：质检员
- **使用场景**：按模板执行来料/过程/出货检验，处置不合格品。
- **功能描述**：检验模板（IQC/IPQC/OQC + 检验项/公差/方法）；检验单逐项记录→判定（pass/fail/conditional）；不合格单（严重度 + 处置 return/rework/accept）。
- **业务规则**：任何一项 fail → 整体 fail；已检单不可重复完成；检验结论与"已检"为两个独立维度。
- **实现状态**：🔶 部分实现（✅ 模板/检验单/NC 创建与查询；❌ NC 审批端点、检验单列表、检验联动库存放行）。

### 功能 11：财务管理

- **功能名称**：应收应付 / 收付款 / 经营报表
- **使用角色**：财务
- **使用场景**：跟踪发票结算、分析账龄与月度利润。
- **功能描述**：收付款登记（AR/AP，联动发票 paid_amount）；应收/应付余额；账龄分桶（0-30/31-60/61-90/90+）；月度利润表（收入-支出=毛利）。
- **业务规则**：发票 paid_amount 累计不可超总金额（💡 推测，未见强校验）；金额以"分"整数存储。
- **实现状态**：🔶 部分实现（✅ 登记+报表；❌ 无凭证/总账、无明细账、无前端）。

### 功能 12：变更历史（系统级）

- **功能名称**：操作审计
- **使用角色**：管理员、审计员
- **使用场景**：查询任意业务单据的创建/更新/删除留痕与字段级差异。
- **功能描述**：写端点同事务记录 before/after JSONB 快照；查询时计算字段级 diff；支持按实体/操作人/时间筛选。
- **操作流程**：业务写操作 → 同事务写 audit_logs → 前端审计抽屉读 diff。
- **业务规则**：与业务同事务（回滚即消失）；操作人字段不设外键（防账号删除抹除历史）；敏感字段在快照序列化层排除。
- **实现状态**：✅ 已实现（identity/purchase/sales/warehouse/production/quality 已接入；前端 AuditHistory 组件可用）。

---

# 第五阶段：产品优化建议

## 5.1 当前产品优势

1. **架构质量高**：模块化单体 + Contract 接缝 + 架构边界测试（`cargo test -p server arch_test`），跨域通道清晰（只读 Port / 同事务写例外 / Outbox 事件），可平滑拆微服务。
2. **数据一致性设计扎实**：库存台账、加权平均成本、单据编码、审批状态机全部下沉为跨域深模块，杜绝重复实现；库存变更行级锁 + 流水留痕。
3. **可观测与运维完整**：OTLP 三信号、健康检查、GC 内务任务、积压指标、双语本地化、OpenAPI 契约自动生成前端类型。
4. **测试覆盖专业**：每个端点同文件集成测试 + 分阶段 Hurl E2E + MRP 边界测试（净需求下限/等号边界）。
5. **审计能力完整**：全量写操作留痕 + 字段级 diff，是"可追溯"的产品卖点。

## 5.2 潜在问题（按代码依据）

| # | 问题 | 代码依据 | 影响 |
|---|---|---|---|
| 1 | **无功能级 RBAC**：`privileged` 是唯一权限维度，`authz_kit`（Cedar）零调用 | `libs/authz_kit` 无引用；`account_delete.rs:67` 仅特权校验 | 任何普通员工可调所有业务端点，安全风险 |
| 2 | **前端完成度严重滞后后端**：仅登录/客户/用户/个人资料 4 个真实页面 | `frontend/src/routes/` 仅 10 个路由文件，其余为占位 | 产品不可用，后端 102 端点无消费端 |
| 3 | **审批人无指派**：状态机有"主管审批中"中间态，但无审批人字段/流程 | `purchase_orders.approved_by` 仅记录动作人 | 无法体现真实审批权责 |
| 4 | **部分表无端点（僵尸表）**：销售退货、模具保养、废品登记 | `sales_returns`、`mold_maintenance`、`scrap_records` 建表无端点 | 业务流程断裂（销售退货无法处理） |
| 5 | **事件驱动未激活**：仅 2 个身份事件；flow 工作流引擎、job_queue 业务任务零使用 | `features/*/lib.rs` 无业务 subscriber/scheduled | 基础设施投入未变现 |
| 6 | **MRP 仅一层 BOM 展开**：CTE 只 join 一层 bom_items | `planning_contract/port.rs` 的 `mrp_calculate` 无递归 | 多级 BOM 企业净需求算错 |
| 7 | **locked_qty 锁定库存未使用**：字段存在但无占库存/释放逻辑 | `inventories.locked_qty` 全库无业务读写 | 缺"订单占料"能力 |
| 8 | **收款/付款与发票核销无强校验**（推测） | `payments` 无超付校验测试 | 财务准确性风险 |
| 9 | **发票无作废/红冲、无应收应付明细账** | 无对应端点 | 财务功能不完整 |

## 5.3 缺失功能（反向推导）

- 销售退货全流程（表已有）
- 模具保养记录 CRUD、模具寿命预警
- 废品登记（scrap_records）
- 采购收货审核环节、PO 自动关单
- 单据打印/导出（无任何导出端点）
- 仪表盘/看板（无统计聚合页）
- 库存锁定/预留（locked_qty）
- 批次全链路追溯（batch_number 有字段但无追溯查询）
- 多级 BOM 递归 MRP
- 供应商/客户关联业务查询（如"某供应商全部 PO"）

## 5.4 可商业化方向

1. **行业垂直化**：以"离散制造（注塑/五金/组装）"为垂直行业包，沉淀模具管理、BOM、质检等 Know-how，做行业版。
2. **平台化/可扩展**：模块化单体天然支持"按模块订阅"——基础进销存免费/低价，生产+MRP 高级模块收费。
3. **开发者生态**：OpenAPI 契约 + 前端类型生成 + 架构测试已具备 SDK 化基础，可输出"ERP 快速二开框架"（现有 README 已自我定位为框架）。
4. **私有化 + 合规**：完整审计留痕是制造业审核（ISO/客户验厂）刚需，可作为合规卖点。
5. **连接器**：当前含 COS/Redis/NATS 等可插拔基建，可延伸对接电商/海关/金税（进项票直连）。

## 5.5 下一阶段 Roadmap（建议）

**P0 可用性（产品从"后端完备"到"可用"）**
- 补齐前端业务页面：物料、采购、销售、仓库、生产、质量、计划、财务（按客户/采购/仓库优先）。
- 补销售退货、模具保养、废品登记端点（消灭僵尸表）。

**P1 权限与安全**
- 接入 `authz_kit`（Cedar）实现功能级 RBAC + 数据范围。
- 审批人指派与待办中心。

**P2 计划深度**
- 多级 BOM 递归展开；建议→一键转 PO；工单排程。
- 库存锁定（locked_qty）订单占料。

**P3 质量与追溯**
- NC 审批闭环；检验结论驱动库存放行/冻结；批次追溯。
- 单据导出/打印、操作日志查看器完善。

**P4 平台化**
- 激活 flow 工作流引擎承接审批/长流程；扩展业务事件（订单已批准/已发货等）驱动通知与集成。
- 多租户与按模块订阅的计费模型探索。

---

## 附录：代码依据索引

| 主题 | 位置 |
|---|---|
| 架构与依赖规则 | `README.md`、`AGENTS.md`、`docs/ARCHITECTURE.md` |
| 数据模型 | `infrastructure/migration/versions/0001~0011_*.sql` |
| 审批状态机 | `cross_domain/approval/lib.rs`、`features/purchase_contract/value_object.rs` |
| 库存台账 | `cross_domain/inventory_ledger/lib.rs` |
| 加权平均成本 | `cross_domain/costing/lib.rs` |
| 单据编码 | `cross_domain/doc_numbering/lib.rs` |
| MRP 计算 | `features/planning_contract/port.rs` |
| 审计服务 | `features/audit_contract/lib.rs`、`features/audit/endpoint/audit_search.rs` |
| 认证鉴权 | `infrastructure/http_auth/middleware/authorize.rs`、`infrastructure/jwt/token_realm.rs` |
| API 清单 | `features/*/endpoint/*.rs` 的 `#[utoipa::path]` |
| 前端页面 | `frontend/src/routes/`、`frontend/src/components/` |
| E2E 业务流程 | `e2e/erp_foundations.hurl` ~ `e2e/p5_cost_finance_mrp.hurl` |
| 内务任务 | `bin/server/internal_jobs.rs` |
