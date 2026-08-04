//! PostgreSQL-backed outbox-ish queue：`queues`（消息本体）+ `queue_deliveries`（消息 × 监听者投递状态）+ 应用内 dispatcher 消费。
//!
//! - **语义**：at-least-once；**广播**——同一 topic 可注册多个 `QueueHandler`（监听者），
//!   一条消息投递给所有监听者，各自独立重试/终态（`queue_deliveries` 承载每个监听者的投递状态）。
//! - **入队**：与业务同事务（`enqueue_event` 等接收 `&Transaction`）。
//! - **文档**：仓库根 `docs/PG_QUEUE.md`。

mod dispatcher;
pub mod enqueue;
pub mod event;
mod gc;
mod handler;
pub mod inbox;
mod registry;
mod status;

pub use dispatcher::{DispatcherConfig, run_dispatcher};
pub use enqueue::{enqueue_event, enqueue_event_with_delay};
pub use gc::{
    DEFAULT_DELIVERED_RETENTION_DAYS, delete_delivered_older_than_in_transaction,
    delete_orphaned_inbox_in_transaction,
};
pub use handler::QueueHandler;
pub use inbox::PgInbox;
pub use registry::{FrozenRegistry, Registry};
