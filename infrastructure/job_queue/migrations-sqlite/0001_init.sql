-- 后台任务队列表（v1，sqlite 方言，单机部署）：worker_jobs + 索引。
-- 由 JobBus::try_new_sqlite 启动时执行（版本表 _job_queue_migrations 记录在 sqlite 文件内）。
-- 仅支持单进程消费（sqlite 文件锁语义）；无触发器（进程内 Notify 由 enqueue 直发）。
-- 时间戳为 epoch 毫秒（run_at / lock_at / done_at）。

CREATE TABLE IF NOT EXISTS worker_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_type TEXT NOT NULL,
    payload TEXT NOT NULL,                   -- JSON 文本（sqlite 无 JSONB）
    status TEXT NOT NULL DEFAULT 'Pending',  -- Pending / Running / Done / Failed
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 1,
    run_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    last_error TEXT,
    lock_by TEXT,
    lock_at INTEGER,
    done_at INTEGER
);

CREATE INDEX IF NOT EXISTS worker_jobs_fetch_idx
    ON worker_jobs (job_type, status, run_at);
