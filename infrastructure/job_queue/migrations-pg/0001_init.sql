-- 后台任务队列表（v1，pg 方言）：worker_jobs + 索引 + 入队通知触发器。
-- 由 JobBus::try_new_pg 启动时执行（版本表 _job_queue_migrations 记录）。
-- 保留幂等写法（IF NOT EXISTS / OR REPLACE / DROP TRIGGER IF EXISTS）：
-- 兼容迁移系统引入前已由旧版"启动自建"建好的表；此后表结构演进走 v2+ ALTER。
-- 状态机与语义见 docs/JOB_QUEUE.md。

CREATE TABLE IF NOT EXISTS worker_jobs (
    id BIGSERIAL PRIMARY KEY,
    job_type VARCHAR(128) NOT NULL,          -- Job::NAME（注册表键 + 路由键）
    payload JSONB NOT NULL,                  -- 序列化后的 Job payload
    status VARCHAR(16) NOT NULL DEFAULT 'Pending',  -- Pending / Running / Done / Failed
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 1,
    run_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error TEXT,
    lock_by TEXT,
    lock_at TIMESTAMPTZ,
    done_at TIMESTAMPTZ
);

-- 拉取索引：按 (job_type, status, run_at) 过滤 + 排序（SKIP LOCKED 子查询）。
CREATE INDEX IF NOT EXISTS worker_jobs_fetch_idx
    ON worker_jobs (job_type, status, run_at, id);

-- 入队通知：INSERT 即 pg_notify（事务内投递，无丢失窗口），worker 侧 PgListener 桥接进程内 Notify。
CREATE OR REPLACE FUNCTION job_queue_notify() RETURNS trigger AS $$
    BEGIN
        PERFORM pg_notify('job_queue_events', NEW.job_type);
        RETURN NEW;
    END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS job_queue_notify_trigger ON worker_jobs;
CREATE TRIGGER job_queue_notify_trigger
    AFTER INSERT ON worker_jobs
    FOR EACH ROW EXECUTE FUNCTION job_queue_notify();
