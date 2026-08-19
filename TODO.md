|优先级|组件|
|-|-|
|S级|Tenant 多租户|
|S级|Permission|
|S级|Notification|
|A级|Import/Export|
|A级|Search|
|A级|Job Center|
|A级|Feature Flag|
|B级|AI Agent|
|B级|Workflow Designer|

[x] Audit Log（变更历史已落地：同事务 `AuditService::record_*`，快照 + 读时 diff）
[ ] 复杂端点测试补强：`purchase_receipt_create` / `sales_delivery_create` / `work_order_material_pick` / `stock_transfer_approve`（成功 + 前置拒绝 + 规则拒绝 + ledger/costing/audit 副作用）
[ ] file / health 冒烟测试
[ ] 性能：Traefik gzip 或 `CompressionLayer`；pg_stat_statements；k6 压测基线；备份/PITR
[ ] redis/redb/nats 环境变量整理
[x] poll是否高效 / pg listen-notify / sqlite update_hook？——已实现：pg INSERT 触发器 NOTIFY → PgListener 桥接进程内 Notify；sqlite enqueue 直发 Notify；轮询保留兜底。update_hook 不可行。见 docs/JOB_QUEUE.md §通知机制
[ ] cache event_bus worker 使用sqlx.toml来迁移
