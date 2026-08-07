# 流程引擎（`flow`）

本文描述 `infrastructure/flow` 的架构、API 与使用边界，与当前代码一致。

## 1. 定位

- **用途**：基于 [sayiir](https://docs.sayiir.dev) 1.0 的**持久化工作流引擎**封装。`CheckpointingRunner` + Postgres 后端：每个 task 完成后自动存快照，进程崩溃后可 `resume` 续跑；测试环境通过 `test-utils` 特性切换为 in-memory backend。
- **不是**：不是消息队列（那是 `infrastructure/queue`）；不替代审批状态机（`cross_domain/approval`）；未启用分布式 worker（sayiir 的 `PooledWorker` 预留，单进程够用）。
- **职责边界**：只做**编排真相**（等待、超时、分流、联动），不做**业务真相**（单据状态列仍由同步端点维护）。

## 2. 核心概念

| 概念 | 说明 |
|------|------|
| 工作流实例 | `instance_id` 唯一标识一次执行；同名实例重复 `run` 的行为由 `ConflictPolicy` 决定 |
| Task（任务） | 工作流的执行单元，`#[task]` 宏把一个 async 函数变成可序列化注册的 `CoreTask` |
| Signal（信号） | 外部事件（人审批、质检结论、下游单据完成）；workflow 内 `signal "name"` 等待，外部 `WorkflowClient::send_event` 投递 |
| Delay（持久化 delay） | 跨小时/天的等待，写入快照，重启不丢 |
| Route / Loop / Fork | 条件分流（编译期穷尽检查）、循环（带 max_iterations）、并行分支 |
| 快照（Snapshot） | 每完成一个 task 保存的执行状态，存于 Postgres，是 resume 的依据 |

## 3. 公开 API（`infrastructure/flow/lib.rs`）

### `Flow`（Clone，AppCtx 持有）

| 方法 | 说明 |
|------|------|
| `try_new(dsn)` | 生产后端：`CheckpointingRunner<PostgresBackend<JsonCodec>>`（connect 自带迁移） |
| `new_for_test()` | 测试后端：`CheckpointingRunner<InMemoryBackend>`（`test-utils` 特性） |
| `with_conflict_policy(&self, p) -> Flow` | 设置重复 instance_id 策略，返回**共享同一后端**的新 Flow（默认 `Fail`） |
| `backend() -> FlowBackend` | 底层后端引用枚举，用于 `WorkflowClient::from_shared`（信号/控制） |
| `run(&workflow, instance_id, input)` | 从头执行，每 task 完成自动 checkpoint |
| `resume(&workflow, instance_id)` | 从最近 checkpoint 续跑；定义 hash 不匹配则报错 |

re-export：`Workflow` / `WorkflowStatus` / `ConflictPolicy` / `RuntimeError` / `BoxError` / `task!` / `workflow!` 宏 —— 调用方无需直接依赖 sayiir。

`FlowRunner` / `FlowBackend` 均为 Postgres / InMemory 两变体枚举（后端具体类型不同，故用委托方法而非 `Deref`）。

## 4. 定义工作流

### `workflow!` DSL（推荐，注册 `#[task]` 自动完成）

```rust
let wf = workflow! {
    name: "receipt_qc",
    deps: &deps,                              // 可选：任务依赖注入容器
    steps: [
        create_receipt,                       // #[task] 生成的 struct
        signal "qc_result",                   // 等待外部信号
        route extract_qc_decision -> QcDecision {   // 条件分流（`-> Enum` 编译期穷尽检查）
            Pass => [receive_into_warehouse],
            Hold => [hold_stock, signal "qc_release"],
            _    => [reject_receipt],
        }
    ]
}.unwrap();
```

语法速查：`,` 顺序；`a || b` 并行 fork；`delay "5s"` / `delay "wait_24h" "24h"` 持久化延迟；`signal "name"` 等待信号；`loop task N` / `loop poll 100 exit_with_last` 循环；`flow child` 内联子流程；`name(param: Type) { expr }` 内联任务；`route key_fn { "a" => [..], _ => [..] }` 字符串 key 分支。

### `#[task]` 任务（带依赖注入）

```rust
#[task(id = "charge_card", timeout = "30s", retries = 3, backoff = "100ms")]
async fn charge(order: Order, #[inject] db: Arc<AppCtx>) -> Result<Receipt, BoxError> {
    // 内部直接复用现有 repository
}
```

- 选项：`id` / `timeout` / `retries` / `backoff` / `backoff_multiplier` / `tags` / `priority` 等；返回 `T`（自动包 Ok）或 `Result<T, E>`（E: `Into<BoxError>`）。
- 生成的 struct（`ChargeTask`）提供 `new()` / `from_deps(&Deps)` / `register()`；`deps:` 缺失在**构建期**以 `BuildError::MissingDep` 暴露。
- `Deps::builder().insert(Arc::new(...)).build()`，按类型存取；共享单例必须包 `Arc`。

### `WorkflowBuilder`（编程式备选）

`.then()` / `.then_task::<T>()` / `.branches().join()` / `.route()` / `.loop_task()` / `.delay()` / `.wait_for_signal()` / `.then_flow(child)`，适合运行时才确定步骤的场景。

## 5. 运行、恢复与控制

### 冲突策略（`ConflictPolicy`）

| 策略 | 重复 instance_id 时 |
|------|---------------------|
| `Fail`（默认） | 报 `RuntimeError::InstanceAlreadyExists` |
| `UseExisting` | 幂等：返回已有实例当前 status，不重跑 |
| `TerminateExisting` | 删旧快照，从头重跑 |

### 信号投递（等待侧 + 发送侧）

```rust
// 等待侧（workflow 内）
steps: [submit, signal "manager_approval", process]

// 发送侧（审批端点）
let FlowBackend::Postgres(backend) = state.flow.backend() else { /* test-only */ };
let client = WorkflowClient::from_shared(backend);
client.send_event("po:PO-2026-0001", "manager_approval", bytes).await?;
```

### 生命周期控制（`WorkflowClient`）

`cancel(instance_id, reason, by)` / `pause(...)` / `unpause(instance_id)` / `status(instance_id)` —— 注意这些方法在 sayiir 1.0.0 位于 `WorkflowClient`（`client.rs`），**不在** `CheckpointingRunner` 上（官方文档页面写法略有超前）。

## 6. 与 AppCtx 集成

- `appctx::AppCtx` 持有 `pub flow: Flow`（`pub use flow::Flow`）；生产构建走 `Flow::try_new(dsn)`，迁移由 `PostgresBackend::connect` 自动执行。
- 测试：`appctx::testing::build(pool)` 已自动接 `Flow::new_for_test()`，工作流冒烟测试无需数据库。

## 7. 测试

- **flow crate 内**：`test-utils` 特性下冒烟测试覆盖 run/resume、三种冲突策略、`backend()`（全部通过）。
- **切片集成**：`#[sqlx::test]` + `testing::build`；workflow 定义（`workflow!` / `#[task]`）放 `features/{domain}/shared/`，端点测试里 `state.flow.run(...)` 验证。

## 8. 适用场景（先过尺子，再动手）

一个流程适合上 flow，当且仅当命中 ≥1 条：

1. 要等人/等系统（外部事件），等待可能跨小时、跨天
2. 步骤间有分支/循环/并行，不是简单线性跳转
3. 中断后要能续跑（崩溃不丢进度）
4. 需要时间驱动的自动行为（超时升级、逾期提醒、定时推进）
5. 需要跨单据/跨域自动联动

**推荐切入（当前有明确断层的场景）**：

| 场景 | 说明 |
|------|------|
| 工单全生命周期 | release → 多工序（可并行）→ 完工 → **自动**触发产成品入库 + 开质检单 |
| 质检挂起/放行 | 检验结论不合格 → 非合格品处置（返工/让步/退货）→ 处置结论回流 |
| 审批超时升级 | `delay "24h"` + route：未审自动提醒上一级 |
| 跨单据联动 | 采购单批准 → 推送/生成到货计划；销售单批准 → 交货计划 |

**不适用（保持同步状态机 + 单事务）**：纯 CRUD 与查询侧（列表/统计/报表——状态必须留在实体列）；单事务原子操作（扣库存、过账）；纯 submit/approve/reject 状态跳转（`cross_domain/approval` 已是最简形态）。

## 9. 陷阱与边界

1. **错误语义**：sayiir 的 `RuntimeError` 是句子风格，**不能直接进 `web::error` 的 key→400/l10n 机制**。task 内返回领域 thiserror（自动转 `BoxError`），端点侧把实例级错误映射回 snake_case key。
2. **状态投影**：流程进度存在快照 JSON 里，不在实体列。实体状态列继续由同步端点维护（业务真相），flow 实例只做编排真相——**三条时间线**（审批流状态 / 生命周期状态 / 流程实例状态）各管各的，不要互相反写。
3. **instance_id 命名**：建议 `"{doc_type}:{id}"`（如 `po:PO-2026-0001`）防跨单据冲突；`ConflictPolicy::UseExisting` 是天然幂等重放手段。
4. **定义不可变**：workflow 定义 hash 变了，旧实例 `resume` 报 `DefinitionMismatch`——改流程定义=新 instance_id（或 TerminateExisting），不能原地升级。
5. **快照体积**：每个 task 的输入/输出都序列化进快照，大 payload（图片等）不要直接过 task 参数，传引用（ID）由 task 内查库。
6. **测试后端**：InMemory 后端进程重启即丢，只用于测试；跨重启续跑必须验证于 Postgres 后端。
7. **信号时效**：`AwaitSignal` 可配 timeout；`send_event` 投递到不存在的实例会报错，投递前可用 `client.status(instance_id)` 探活。
