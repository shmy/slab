# Slab 改进计划

> 基于 2026-08 架构与代码审查。核心判断：**架构纪律顶级，代码实现中上，测试偏薄且不均**。短板都是"再磨一磨"的活，不是方向性错误。

## 审查摘要

| 维度 | 评分 | 说明 |
|---|---|---|
| 架构纪律 | 9/10 | arch_test 强制依赖规则、ADR 记录决策回溯、clippy 全局 deny、cross_domain 例外有命名 |
| 代码实现 | 7/10 | 垂直切片一致、跨域写规范、错误枚举规范；但状态魔法数字、领域语言未贯彻到代码层 |
| 测试 | 6/10 | 203 测试 / 100 端点；分布不均（file/health 零测试），复杂端点只覆盖 happy path |

### 三个核心短板（按反差大小排序）

1. **状态魔法数字**：CONTEXT.md 精细区分"审批流状态 / 生命周期状态"两条时间线，代码层却坍缩成裸数字 `status != 3`。24 处遍布 7 域。现成范式 `ApprovalStatus` 枚举已有，但全仓仅 1 域使用。
2. **跨域写漏接风险**：`inventory_ledger` 被 8 端点同事务直写，漏接不报编译错（audit 已有 arch_test 守护，inventory_ledger 没有）。
3. **复杂端点测试覆盖偏薄**：`purchase_receipt_create.rs` 295 行 / 1 测试，核心分支（超收、前置状态、副作用）未覆盖。

---

## P0：状态枚举化（最高收益，可分域渐进）

### 现状证据

- 24 处业务状态魔法数字，遍布 7 域（purchase / production / quality / sales / product）
- migration 注释语义混乱：`-- ApprovalStatus`（审批流）与 `-- 0=draft 1=released...`（生命周期）混用，DB 列都叫 `status`
- CONTEXT.md 的两条状态时间线在代码层未区分
- 测试层同样渗透：`assert_eq!(status, 3)` 靠数字断言
- 现成范式：`features/shared_contract/value_object/approval_status.rs` 的 `ApprovalStatus` 枚举（`#[derive(Type)]` + `#[repr(i16)]`）

### 目标

CONTEXT.md 的领域状态概念在代码层有对应 `enum`；两条时间线（审批流 / 生命周期）显式区分；migration 注释语义对齐。

### 分批执行

| 批次 | 域 | 枚举 | 时间线 |
|---|---|---|---|
| P0-1 | purchase | `PurchaseOrderStatus`（审批流）、`PurchaseReceiptStatus`（生命周期） | 拆两条 |
| P0-2 | production | `WorkOrderStatus`（生命周期） | 生命周期 |
| P0-3 | quality | `InspectionOrderStatus`（待检/已检）、`Verdict`（Pass/Fail/Conditional） | 拆两条，对应 CONTEXT.md「检验结论 vs 状态」 |
| P0-4 | sales / product | `SalesOrderStatus`、`BomStatus` | 各自 |

### 每批步骤

1. 在 `{domain}_contract::value_object` 建枚举（复制 `ApprovalStatus` 范式，doc 注释引用 CONTEXT.md 术语）
2. 端点 `order.status != 3` → 枚举判断（`query_as!` 反序列化成枚举，或 `as i16` 比较）
3. 测试 `assert_eq!(status, 3)` → 枚举变体
4. 新建 migration 时注释标明时间线归属（**旧 migration 不可编辑**，仅新版本动到相关表时对齐）

### 守护

新增 arch_test 规则 `endpoints_should_not_compare_status_with_magic_number`：扫描 `endpoint/*.rs` 的 `status != <数字>` / `status == <数字>` 模式，强制用枚举常量。与 `write_endpoints_must_wire_audit_service` 同构。

### 预期收益

CONTEXT.md 从"文档词汇表"变成"代码注释来源"；两条状态时间线代码层可区分；grep 状态语义不再靠数字猜。

---

## P1：跨域写守护 + 复杂端点测试补强

### P1-A：inventory_ledger 接入守护

**现状**：8 端点接入 `InventoryLedger::receive/issue/transfer`，漏接不报编译错。costing 仅 1 处，暂不守护。

**做法**：arch_test 加规则 `inventory_mutating_endpoints_must_wire_ledger`。识别动词集 `{create, pick, return_approve, transfer_approve, check_approve, initial}` × 物料移动语义，强制 `content.contains("InventoryLedger::")`。复用 audit 的豁免清单 + 欠账白名单机制。

**预期收益**：避免库存账实不符--ERP 最严重的一类 bug。

### P1-B：复杂端点测试补强

**现状**：`purchase_receipt_create.rs` 295 行 / 1 测试，仅 happy path 日期断言。

**目标**：每个复杂端点至少覆盖 4 类分支：成功路径、前置状态拒绝、业务规则拒绝、副作用验证（inventory_ledger + costing + audit 全断言）。

**优先补**（按风险）：
1. `purchase_receipt_create` - 跨 3 个 cross_domain 写
2. `sales_delivery_create` - 出库负库存边界
3. `work_order_material_pick` - 领料超领
4. `stock_transfer_approve` - 调拨两端原子性

---

## P2：横切关注点（依赖 P0）

- **P2-A Permission 系统**（S 级）：前置依赖 P0--权限规则常为"状态驱动授权"（草稿本人可改、已提交需主管审批）。`libs/authz_kit`（Cedar）待接入。
- **P2-B file / health 零测试补齐**：file 测 happy path + 大小限制；health 加冒烟测试。

---

## P3：长期治理

- 补 ADR：0002（垂直切片 + contract 公共表面）、0003（cross_domain 例外栖息地）、0004（状态双时间线模型）
- TODO.md 优先级刷新：Audit 已完成，Permission 依赖 P0，Notification 独立可并行

---

## 执行顺序

```
第 1-2 周：P0-1（purchase 状态枚举化）+ arch_test 守护规则  ← 杠杆点
第 3 周：  P1-A（inventory_ledger 守护）-- 独立、收益高、风险低
第 4 周：  P0-2 / P0-3（production / quality，复用范式）
第 5 周：  P1-B（复杂端点测试补强）
后续：     P0-4 → P2-A（Permission）→ P3（ADR 补全）
```

**关键原则**：P0-1 是杠杆点--同时建立"枚举范式 + arch_test 守护规则"，后续批次是机械复制。先做扎实，别急着铺开。
