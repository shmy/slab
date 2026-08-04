-- 更新时间触发器
CREATE OR REPLACE FUNCTION fn_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = CURRENT_TIMESTAMP;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 非持久化语义的热点 KV（见 `infrastructure/adapters/pg_cache`）；`UNLOGGED` 可丢崩溃前未刷盘的写入。
CREATE UNLOGGED TABLE IF NOT EXISTS caches (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_caches_expires_at ON caches (expires_at);

-- 域外投递队列
CREATE TABLE queues (
    id BIGSERIAL PRIMARY KEY,
    topic VARCHAR(255) NOT NULL,
    payload TEXT NOT NULL,
    -- 1=pending, 2=delivered, 3=failed
    status SMALLINT NOT NULL DEFAULT 1 CHECK (status IN (1, 2, 3)),
    delivered_at TIMESTAMPTZ,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER set_updated_at_queues BEFORE
UPDATE ON queues FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 拉取待消费任务（最重要）
CREATE INDEX idx_queues_pending
  ON queues (next_attempt_at, id)
  WHERE status = 1
    AND attempts < max_attempts;

-- 按 topic 拉取待消费任务
CREATE INDEX idx_queues_topic_pending
  ON queues (topic, next_attempt_at, id)
  WHERE status = 1
    AND attempts < max_attempts;

-- 已投递任务
CREATE INDEX idx_queues_delivered
  ON queues (delivered_at)
  WHERE status = 2
    AND delivered_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS queue_inbox (
    message_id BIGINT NOT NULL,
    handler TEXT NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (message_id, handler)
);
