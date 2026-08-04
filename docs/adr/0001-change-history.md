# 变更历史（Change History）采用 Outbox 异步落库 + 全局单表 + 派生宏

Status: accepted

系统需要"资源维度的字段级变更历史"（TODO 原 Audit Log，讨论后重新定义）：记录所有写操作产生的变更（创建 / 更新 / 删除），字段级 before → after，按资源可查。我们不记录请求级审计（监控已覆盖），不记录失败请求（无变更即无历史）。数据流为：业务事务内读 before → 业务侧计算 diff → 同事务写入 Outbox → 消费者异步落库到全局 `changesets` 表，接受秒级可见延迟。

## 决策要点

1. **记录内容**：所有产生变更的写操作；创建（before 空）与删除（after 空）都记；操作人取 `AuthedAccount` 必记。
2. **存储**：全局单表 `changesets(resource_type, resource_id, actor_id, change_type, diff jsonb, created_at)`，放 `infrastructure/audit`，而非每域独立历史表——changeset 结构跨域同构，单表让宏 / diff / 查询端点只写一次。
3. **事务**：走项目 Outbox（`infrastructure/queue`），业务与"审计待办"同事务原子，提交后消费。**禁止"提交后再发送"**（存在事务成功但消息丢失的黑洞窗口）。
4. **diff**：业务侧计算，入队传 diff 结果（`[{field, label, before, after}]`），消费者无脑 INSERT；diff 失败在源头暴露（fail-fast）。
5. **审计视图**：`#[derive(Audited)]` 派生宏（`libs/audit_kit`），支持 `#[audit(skip)]` / `#[audit(label)]`；元字段（`version` / `created_at` / `updated_at`）一律 skip，不记录 version 号。
6. **查询**：统一端点 `GET /api/v1/changesets?resource_type=&resource_id=`，按时间倒序分页；当前登录即可读，权限控制留待 Permission 系统（TODO S 级）接入。
7. **保留**：永久保留，不设 TTL；提供手动清理 / 归档脚本。

## Considered Options

- **SQL 触发器（被否）**：绝对完备但只能记"行变更"（表名 + OLD/NEW），不是业务动作；业务事务回滚时触发器写的审计行随之回滚，**无法记录"失败请求"**——虽然本设计也不记失败，但触发器方案彻底绑死行级语言，且每加一张表都要记得挂触发器（与白名单问题同构）。
- **同事务直接写 changesets（被否，改用 Outbox）**：无延迟、更强一致，但业务事务更长；Outbox 方案把落库压力后移且不丢（重试 + 幂等），秒级延迟在 ERP 写频率下可接受。
- **提交后发送队列（禁止）**：事务成功后、发送前崩溃 = 历史永久缺失，黑洞比"尽力而为"更不可接受。
- **每域独立历史表（被否）**：表结构、仓储、端点每域复制一遍，违反 DRY 且易漂移。

## Consequences

- 变更生效到历史可见之间存在秒级延迟；前端"保存后立刻看历史"需接受短暂为空。
- `changesets` 表健康是运维红线：Outbox 消费者若积压或失败，历史会滞后或缺失，需监控消费者延迟。
- 审计是横切能力而非业务域：`infrastructure/audit` 提供 trait / diff / 消费者 / 查询，`features/audit` 只挂路由（仿 health 无 contract），业务切片依赖 `infrastructure/audit` + `libs/audit_kit`。
- 新写端点需要接入变更历史（业务侧 capture）；漏接不产生编译错误，靠代码评审与端点测试兜底（请求级兜底层不存在，因已砍掉请求审计）。
