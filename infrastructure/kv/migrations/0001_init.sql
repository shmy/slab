-- 缓存表（v1，pg 方言）：_pg_caches（UNLOGGED，可丢语义——崩溃丢缓存、重启自愈）。
-- 由 KvBackend::try_new_pg 启动时执行（版本表 _kv_migrations 记录）。
-- 保留幂等写法：兼容迁移系统引入前已由旧版"启动自建"建好的表；演进走 v2+ ALTER。

CREATE UNLOGGED TABLE IF NOT EXISTS _pg_caches (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pg_caches_expires_at ON _pg_caches (expires_at);
