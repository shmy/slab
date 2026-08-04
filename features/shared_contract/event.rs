//! 跨域事件契约：任何希望被 `queue` 投递的事件类型实现本 trait。
//!
//! 事件定义放在 `{domain}_contract::events`，实现 `shared_contract::event::Event`；
//! 由 `infrastructure/queue` 的 `enqueue_event` / `enqueue_event_with_delay` 入队，
//! 消费方在 `features/{domain}/subscriber/` 实现 `queue::QueueHandler`。

use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Debug;

/// 跨出域事件：序列化 + topic 常量。
///
/// 每个事件类型实现本 trait，通过 `queue::enqueue_event` 入队。
pub trait Event: Debug + Serialize + DeserializeOwned {
    /// 消息 topic 标识，全局唯一。
    const TOPIC: &'static str;
}
