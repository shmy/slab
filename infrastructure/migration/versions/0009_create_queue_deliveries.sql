-- 广播投递状态表：queues（消息本体）一行消息 × 每个监听者（handler）一行独立投递状态。
-- 引入原因：queues 行级 status/attempts/next_attempt_at 无法表达「同一 topic 多个监听者各自成败重试」；
-- 同一 topic 可注册多个 QueueHandler（Registry 多值），dispatcher 按 (message_id, handler) 逐个投递。
CREATE TABLE IF NOT EXISTS queue_deliveries (
    message_id      BIGINT NOT NULL REFERENCES queues(id) ON DELETE CASCADE,
    handler         TEXT NOT NULL,
    -- 1=pending, 2=delivered, 3=failed
    status          SMALLINT NOT NULL DEFAULT 1 CHECK (status IN (1, 2, 3)),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts    INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_error      TEXT,
    delivered_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (message_id, handler)
);

CREATE TRIGGER set_updated_at_queue_deliveries BEFORE
UPDATE ON queue_deliveries FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 拉取待投递任务（最重要）：与 dispatcher 的 SELECT 对齐
CREATE INDEX idx_queue_deliveries_pending
  ON queue_deliveries (next_attempt_at, message_id)
  WHERE status = 1
    AND attempts < max_attempts;

-- 已投递任务（GC 与观测用）
CREATE INDEX idx_queue_deliveries_delivered
  ON queue_deliveries (delivered_at)
  WHERE status = 2
    AND delivered_at IS NOT NULL;
