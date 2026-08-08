//! 跨出域事件 trait（re-export 自 `shared_contract::event::Event`）。
//!
//! 历史路径兼容：`event_bus::event::Event` 与 `shared_contract::event::Event` 是同一 trait。
//! 新代码直接 `use shared_contract::event::Event;`。

pub use shared_contract::event::Event;
