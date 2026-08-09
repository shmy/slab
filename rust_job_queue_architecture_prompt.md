# Rust + Axum 企业级 Job Queue 架构设计需求

## 角色

你是一名资深 Rust 后端架构师。

我正在设计一个 Rust + Axum 的企业级后端系统，采用 DDD + 模块化单体架构，需要设计一个后台工作队列（Job Queue）抽象层。

目标：

- 保留 Rust 强类型优势
- 支持多个业务 Job
- 不让业务代码直接依赖具体队列实现
- 未来可以替换不同 Queue Backend

---

# 技术背景

技术栈：

- Rust
- Tokio async runtime
- Axum Web Framework
- Modular Monolith
- PostgreSQL
- Apalis

---

# 核心目标

设计一个强类型 Job Queue 封装。

业务代码：

```rust
jobs.enqueue(
    GenerateReport {
        report_id: 10001
    }
).await?;
```

业务代码不应该知道：

- Apalis
- Redis
- Storage
- Worker
- Queue Backend

---

# 重点问题

不要设计：

```rust
struct AppState {
    report_queue: Data<GenerateReport>,
    email_queue: Data<SendEmail>,
}
```

因为：

- 泛型类型爆炸
- AppState 膨胀
- 业务耦合基础设施

需要设计：

- Job
- JobHandler
- JobBus
- Worker Runtime

支持：

```rust
GenerateReport
SendEmail
SyncOrder
CreateInvoice
```

多个 Job。

---

# 需要输出

请设计完整生产级方案，包括：

- Cargo.toml
- 目录结构
- Job Trait
- JobBus
- Handler 注册
- Worker 启动
- Axum State 集成
- 两个 Job 示例

要求：

- Rust async
- Arc
- trait object 或其他合理动态分发方案
- 可测试
- 可替换 Backend

---

# 架构目标

类似：

```
Application

    JobBus

        |

Infrastructure

    Apalis Adapter

        |

Redis/Postgres/NATS
```

业务层不依赖 Apalis。

---

# 请解释

1. 为什么不能直接暴露 Apalis 泛型类型？
2. 多 Job 类型如何管理？
3. Job、Event、Workflow 的区别？
4. 如何未来替换：
   - Apalis
   - Redis Streams
   - NATS JetStream
   - Sayiir

目标：

设计一个适合 Rust + Axum + DDD 企业系统的后台任务架构。
