# Slab — 业务上下文

## 一、项目概述

通用 ERP 后端系统，基于 DDD 垂直切片架构（Rust / Axum / PostgreSQL）。

**当前阶段：Phase 4 完成。** 覆盖采购→质检→销售→生产→财务全线。

---

## 二、已实现领域

### Phase 1 — 基础

| 域 | 核心表 | 状态 |
|----|--------|------|
| `identity` | accounts | 账号 CRUD、登录/刷新/登出、密码管理（argon2） |
| `file` | — | 图片上传（S3） |
| `health` | — | /livez /readyz /healthz |

### Phase 2a — 采购 + 质检 + 库存升级

| 域 | 核心表 | 状态 |
|----|--------|------|
| `item` | items, item_categories, item_units, item_costs | 物料主数据 CRUD、分类树、单位换算、成本 |
| `customer` | customers | 客户 CRUD |
| `supplier` | suppliers | 供应商 CRUD |
| `warehouse` | warehouses, inventories, inventory_transactions, inventory_checks, stock_transfers | 仓库管理、库存流水、盘点、调拨 |
| `purchase` | purchase_orders, purchase_receipts, purchase_returns, purchase_invoices | 采购订单→收货→退货→发票，审批流 |
| `quality` | inspection_templates, inspection_orders, inspection_results, non_conformances | IQC 检验模板/单/结果、不合格处理 |
| `planning` | — (查询端点) | 再订货点预警、采购建议清单（warehouse 子模块） |

### Phase 2b — 销售

| 域 | 核心表 | 状态 |
|----|--------|------|
| `sales` | sales_orders, sales_deliveries, sales_returns, sales_invoices | 销售订单→发货→退货→发票 |

### Phase 3 — 生产

| 域 | 核心表 | 状态 |
|----|--------|------|
| `product` | boms, bom_items, molds, mold_maintenance | BOM 多级展开、模具台账 |
| `production` | work_orders, work_order_materials, work_order_operations, production_receipts, scrap_records | 工单→BOM 展开→领料→工序报工→完工入库→废品 |

### Phase 4 — 财务 + 计划

| 域 | 核心表 | 状态 |
|----|--------|------|
| `finance` | payments | 收/付款管理（关联采购/销售发票）、账龄分析（AR/AP 按 30/60/90 天分组） |

---

## 三、全域基础设施

| 组件 | 位置 | 职责 |
|------|------|------|
| `shared_contract` | features/ | 跨域共享类型（ID tsid、PhoneNumber、分页、值对象） |
| `infrastructure/appctx` | infrastructure/ | 全局 AppCtx（PgPool、TokenBundle、Blob、HttpClient） |
| `infrastructure/web` | infrastructure/ | Axum 提取器（ValidJson/Query/Path）、Problem Details 响应 |
| `infrastructure/http_auth` | infrastructure/ | Bearer JWT 鉴权中间件 + AuthedAccount |
| `infrastructure/locale` | infrastructure/ | Fluent 本地化中间件 |
| `infrastructure/event_bus` | infrastructure/ | 事件总线（广播；Pg Outbox 默认 / NATS JetStream） |
| `infrastructure/kv` | infrastructure/ | 可插拔 KV 缓存后端（Pg UNLOGGED 默认 / redb / redis） |
| `infrastructure/jwt` | infrastructure/ | JWT 签发/验证（双域：Account + Customer） |
| `infrastructure/blob` | infrastructure/ | 对象存储（S3/COS） |
| `infrastructure/migration` | infrastructure/ | sqlx migrate（8 个迁移文件） |
| `libs/authn_kit` | libs/ | argon2 密码哈希 |
| `libs/trace_kit` | libs/ | OpenTelemetry 观测 |
| `libs/sched_kit` | libs/ | 定时任务（cron） |

---

## 四、请求管道

```
Health 路由（公开）
  → identity unprotected（/login, /refresh）
  → 受保护路由（所有其他域）
  → account_auth_middleware（Bearer JWT）
  → locale middleware（Fluent）
  → GovernorLayer（50 req/s per IP）
  → OtelInResponseLayer + OtelAxumLayer
  → TimeoutLayer（外层超时）
```

---

## 五、数据库全景（43 张业务表 + 3 张基础设施表）

```
accounts               — 账号
customers              — 客户
suppliers              — 供应商
items                  — 物料（含 reorder_point / safety_stock）
item_categories        — 物料分类树
item_units             — 单位换算
item_costs             — 物料成本
warehouses             — 仓库
inventories            — 库存
inventory_transactions — 库存流水
inventory_checks       — 盘点单 + inventory_check_items
stock_transfers        — 调拨单 + stock_transfer_items
purchase_orders        — 采购订单 + purchase_order_lines
purchase_receipts      — 采购收货 + purchase_receipt_lines
purchase_returns       — 采购退货 + purchase_return_lines
purchase_invoices      — 采购发票（含 paid_amount）
sales_orders           — 销售订单 + sales_order_lines
sales_deliveries       — 销售发货 + sales_delivery_lines
sales_returns          — 销售退货 + sales_return_lines
sales_invoices         — 销售发票（含 paid_amount）
boms                   — BOM + bom_items
molds                  — 模具 + mold_maintenance
work_orders            — 工单 + work_order_materials + work_order_operations
production_receipts    — 完工入库
scrap_records          — 废品登记
inspection_templates   — 检验模板 + inspection_template_items
inspection_orders      — 检验单 + inspection_results
non_conformances       — 不合格处理
payments               — 收/付款记录
queues / queue_deliveries — 域外广播队列（消息本体 + 监听者投递状态）
caches                 — 热点 KV（UNLOGGED）
```

---

## 六、可扩展方向

- 成本核算（item_costs 已有表，缺少加权平均/先进先出算法）
- 财务报表（应收/应付明细、现金流）
- 外协加工（purchase orders 可扩展类型）
- 角色/权限（RBAC）
- 操作审计日志（变更历史已落地：资源维度字段级；请求级审计明确不在范围）
- 通知系统（邮件/短信/站内信）
- 移动端 API
