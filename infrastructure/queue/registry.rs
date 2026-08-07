use std::collections::HashMap;
use std::sync::Arc;

use crate::handler::QueueHandler;

/// topic → 监听者列表（广播：同一 topic 多个 handler）。
type HandlerMap<C> = HashMap<&'static str, Vec<Arc<dyn QueueHandler<C>>>>;

pub struct Registry<C: Send + Sync + 'static> {
    handlers: HandlerMap<C>,
}

impl<C: Send + Sync + 'static> Default for Registry<C> {
    fn default() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
}

impl<C: Send + Sync + 'static> Registry<C> {
    /// 注册监听者。同一 topic 可注册多个 handler（广播语义），全部保留；
    /// 之前是 `insert`（同 topic 后注册覆盖先注册，静默丢消息），现改为追加。
    /// **同名冲突是编程错误**（同 topic 下两个 handler 的 `name()` 相同会导致
    /// 投递行主键冲突、后者静默丢失），注册时立即 panic（fail fast）。
    pub fn register<H>(&mut self, handler: H) -> &mut Self
    where
        H: QueueHandler<C>,
    {
        let topic = handler.topic();
        let name = handler.name();
        let entry = self.handlers.entry(topic).or_default();
        assert!(
            !entry.iter().any(|existing| existing.name() == name),
            "queue handler name conflict: topic {topic:?} already has a handler named {name:?}"
        );
        entry.push(Arc::new(handler));
        self
    }

    pub fn freeze(self) -> FrozenRegistry<C> {
        FrozenRegistry {
            handlers: Arc::new(self.handlers),
        }
    }
}

#[derive(Clone)]
pub struct FrozenRegistry<C: Send + Sync + 'static> {
    handlers: Arc<HandlerMap<C>>,
}

impl<C: Send + Sync + 'static> FrozenRegistry<C> {
    /// 取某 topic 的所有监听者；无注册时返回 `None`（dispatcher 视为终态失败）。
    pub(crate) fn get(&self, topic: &str) -> Option<&[Arc<dyn QueueHandler<C>>]> {
        self.handlers.get(topic).map(Vec::as_slice)
    }

    /// 遍历全部 (topic, handlers)，供 NATS 后端为每个 handler 建 durable consumer。
    #[cfg(feature = "nats")]
    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = (&'static str, &Vec<Arc<dyn QueueHandler<C>>>)> {
        self.handlers.iter().map(|(k, v)| (*k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rootcause::Result;
    use serde_json::Value;
    use std::future::Future;
    use std::pin::Pin;

    struct DummyHandler(&'static str, &'static str);

    impl<C: Send + Sync + 'static> QueueHandler<C> for DummyHandler {
        fn topic(&self) -> &'static str {
            self.0
        }
        fn name(&self) -> &'static str {
            self.1
        }
        fn handle<'a>(
            &'a self,
            _ctx: &'a C,
            _payload: Value,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn same_topic_registers_multiple_handlers() {
        let mut registry = Registry::<()>::default();
        registry
            .register(DummyHandler("slab.test.evt", "listener_a"))
            .register(DummyHandler("slab.test.evt", "listener_b"));
        let frozen = registry.freeze();

        let handlers = frozen.get("slab.test.evt").expect("two handlers");
        assert_eq!(handlers.len(), 2);
        assert_eq!(handlers[0].name(), "listener_a");
        assert_eq!(handlers[1].name(), "listener_b");
    }

    #[test]
    fn distinct_topics_are_separate() {
        let mut registry = Registry::<()>::default();
        registry.register(DummyHandler("slab.a", "a"));
        let frozen = registry.freeze();

        assert!(frozen.get("slab.b").is_none());
        assert_eq!(frozen.get("slab.a").expect("one").len(), 1);
    }

    #[test]
    #[should_panic(expected = "name conflict")]
    fn same_name_on_same_topic_panics() {
        let mut registry = Registry::<()>::default();
        registry
            .register(DummyHandler("slab.test.evt", "listener_a"))
            .register(DummyHandler("slab.test.evt", "listener_a"));
    }

    #[test]
    fn unknown_topic_returns_none() {
        let frozen = Registry::<()>::default().freeze();
        assert!(frozen.get("nope").is_none());
    }
}
