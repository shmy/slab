|优先级|组件|
|-|-|
|S级|Tenant 多租户|
|S级|Audit Log|
|S级|Permission|
|S级|Notification|
|A级|Import/Export|
|A级|Search|
|A级|Job Center|
|A级|Feature Flag|
|B级|AI Agent|
|B级|Workflow Designer|

[ ] redis/redb/nats 环境变量整理

[x] poll是否高效 / pg listen-notify / sqlite update_hook？——已实现：pg INSERT 触发器 NOTIFY → PgListener 桥接进程内 Notify；sqlite enqueue 直发 Notify；轮询保留兜底。update_hook 不可行（连接级回调收不到跨连接写入，官方文档确认）。见 docs/JOB_QUEUE.md §通知机制
[ ] cache event_bus worker 使用sqlx.toml来迁移