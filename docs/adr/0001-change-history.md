# 变更历史（Change History）采用业务层同步快照 + 读时 diff + contract 跨域写 Port

Status: accepted（推翻 2026-07 的 Outbox 版本）

> **2026-08-05 修订**：写入入口由 `audit_contract::record` 改为 `audit_contract::AuditService`（动词方法 `record_create` / `record_updated` / `record_deleted`，before 由调用方锁读显式传入）；`action` 由业务动作字符串改为 `AuditAction` 枚举（`audit_logs.action` SMALLINT，当前 CRUD 三态 Created=1/Updated=2/Deleted=3，业务动作如 approve 需要时扩展变体）；`Operator` 值对象下沉 `shared_contract::value_object::operator`，`OperatorContext` 改为 newtype 提取器（`OperatorContext(pub Operator)` + Deref，消费方只依赖 shared_contract）。migration 0012 变更 action 列类型时清空了上线初期存量行（当时仅有测试数据）。

系统需要"资源维度的字段级变更历史"：记录所有写操作产生的变更（创建 / 更新 / 删除），字段级 before → after，按资源可查。

第一版设计（Outbox 异步落库 + `#[derive(Audited)]` 派生宏 + 业务侧 diff，见 git 历史 5329e43 的 ADR 与未合并代码）在落地过程中被推翻：为一条 INSERT 付出纯类型 / 派生宏 / 运行时框架 / 路由切片四个 crate 的机制，且秒级延迟让前端"保存后立刻看历史"体验打折。最终改为**业务事务内同步快照写入 + 查询时算 diff**。

## 决策要点

1. **记录内容**：所有产生变更的写操作；创建（before 空）与删除（after 空）都记；操作人取 `OperatorContext` 必记；同时记录操作动作（`action`，2026-08 起为 `AuditAction` 枚举落库 SMALLINT，见顶部修订——原设计为业务动作字符串如 `account.create` / `purchase_order.approve`，业务动作扩展为枚举变体）、客户端 IP 与 User-Agent。不记录请求级审计（监控已覆盖），不记录失败请求（无变更即无历史）。
2. **存储**：全局单表 `audit_logs(entity, entity_id, operator_id, action, before jsonb, after jsonb, ip inet, user_agent, created_at)`，归 `features/audit` 域。**存 before/after 快照而非 diff**：快照是无损源数据，diff 是派生数据——字段级展示、撤销、报表都能从快照重算，格式随意演进；create/delete 本来就是整快照，空间差异可忽略（ERP 写频率下年增 GB 级，TOAST 压缩后更小，有清理脚本托底）。
3. **写入**：业务切片在自己的写事务内同步调用 `audit_contract::AuditService`（`record_create` / `record_updated` / `record_deleted`，传 `&mut txn` + `&Operator` + before/after 实体），与业务写同事务原子——回滚即消失、提交即可见。无 Outbox、无消费者、无异步延迟、无需幂等。**禁止"提交后再写"**（事务成功但审计丢失的黑洞窗口）。
4. **diff**：查询端读时计算（`features/audit::diff::json_diff` 纯函数：数组整体对比、对象递归展开 `.` 路径、键排序确定性输出）；`change_type`（create/update/delete）由 before/after 快照推断，不落库。
5. **敏感字段**：实体序列化层一处声明排除（`#[serde(skip)]`），如 `Account.password` / `Account.version`——密码哈希不进审计表，乐观锁版本递增不构成历史噪音。
6. **跨域写 Port**：`audit_contract::AuditService` 是 contract 内带 SQL 实现的**跨域同事务写**入口。本仓 Port 先例（`{Domain}Port` 默认方法自带读 SQL）只覆盖读，audit 是第一个跨域写——以动词方法命名（`record_create` 等 vs 只读 Port 的名词方法 `by_id`）并写文档注释区分，不违反"contract 间互不依赖 / 不 import features/{other} runtime crate"规则。写 SQL 放 `lib.rs` 而非 `port.rs`（arch_test 规则要求 port 文件不得出现写 SQL）。
7. **操作人上下文**：`Operator` 值对象（操作人 + IP + UA）在 `shared_contract::value_object::operator`——纯值对象，零基础设施依赖，登录历史 / 安全日志同样需要（按内容而非消费方命名）。HTTP 提取器 `http_auth::extract::operator::OperatorContext`（newtype `OperatorContext(pub Operator)` + Deref，消费方 `&ctx` 自动 deref coercion）。提取器放 `http_auth` 而非 `audit_contract`（contract 不得依赖 infrastructure，否则把鉴权栈拖进所有消费方），也非 `web`（`http_auth → web` 已成，反向成环）。
8. **查询**：`GET /api/v1/audit-logs?entity=&entity_id=`，id（应用生成 tsid）倒序游标分页；`operator_id` **不设外键**——历史是不可变记录，操作人不因账户删除被抹除（LEFT JOIN accounts 取姓名，账户删除后显示 null）。当前登录即可读，权限控制留待 Permission 系统（TODO S 级）。
9. **保留**：永久保留，不设 TTL；手动清理 / 归档脚本按需重建（沿用第一版的 cleanup 思路）。例外：上线初期（2026-08）migration 0012 变更 `action` 列类型时清空了当时的存量行（仅测试数据，见顶部修订）。

## Considered Options

- **SQL 触发器（被否）**：零接入但只能记"表行"（OLD/NEW），不是资源级业务动作；order + 明细分表记多条；拿不到操作人（需 GUC 仪式，把"记得做某事"请回业务代码）；敏感列盲拍进审计表（每表排除清单 = 白名单问题回归）；无法表达 `action`（approve / complete 这类业务动作）。第一版已否，复查后结论不变。
- **Outbox 异步落库（被否，第一版）**：落库压力后移 + 幂等重试，但 ERP 写频率下一条 INSERT 的负载不值得 dispatcher / 消费者 / 幂等主键 / 秒级延迟整套机器；历史不可立即见。
- **写时算 diff 落库（被否）**：diff 是派生数据，锁死展示格式、丢原始快照；且需要字段编排（派生宏或白名单）才能得到"审计视图"。快照 + 读时 diff 无需编排，`#[serde(skip)]` 一处声明解决敏感字段。
- **跨域写走事件 / 只读 Port（被否）**：事件通道已被同步直写取代；只读 Port 语义不承载写。contract 内带写实现是 audit 的特例，动词命名 + 文档注释明确边界。
- **每域独立历史表（被否）**：表结构、写入、端点每域复制，违反 DRY。

## Consequences

- 写端点需接入变更历史（同事务调 `audit_contract::AuditService` 的 `record_*` 方法）；**漏接不产生编译错误**，靠代码评审 + 端点测试断言 `audit_logs` 兜底（identity 的账户创建 / 更新 / 删除已接入作示范）。
- 历史**立即可见**（无秒级延迟）；事务回滚时审计同步消失。
- `audit_logs` 持续增长（年增 GB 级），`(entity, entity_id, id DESC)` 索引支撑按资源查询；归档 / 清理脚本按需重建。
- 领域语言用"变更历史"，代码命名沿用 `audit`（`features/audit`、`audit_logs`）——展示层与代码命名不同步是有意的（见 CONTEXT.md）。
- 新写端点接入变更历史成为编码约定（见 AGENTS.md 新增功能清单）。
