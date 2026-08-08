//! 跨域事件契约：任何希望被事件总线投递的事件类型实现本 trait。
//!
//! 事件定义放在 `{domain}_contract::events`，实现 `shared_contract::event::Event`；
//! 由 `infrastructure/event_bus` 的 `publish` / `publish_with_delay` 发布，
//! 消费方在 `features/{domain}/subscriber/` 实现 `event_bus::Subscriber`。

use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Debug;

/// 跨出域事件：序列化 + topic 常量。
///
/// 每个事件类型实现本 trait，通过 `event_bus::publish` 发布。
pub trait Event: Debug + Serialize + DeserializeOwned {
    /// 消息 topic 标识，全局唯一。
    const TOPIC: &'static str;
}
