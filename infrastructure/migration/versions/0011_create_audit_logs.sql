-- 变更历史（Audit Logs）：资源维度的操作留痕（创建 / 更新 / 删除）。
-- 业务事务内由 audit_contract::record() 同步写入（同事务原子，回滚即消失，无 Outbox）。
-- before / after 为审计视图 JSONB 快照（敏感字段在实体序列化层排除），查询端读时算字段级 diff。
-- id 应用生成（tsid），单调递增天然等同时序，游标分页按 id 倒序。
-- operator_id 不设外键：历史是不可变记录，操作人不因账户删除而被抹除；
-- 展示层 LEFT JOIN accounts 取姓名，账户不存在时显示 null。

CREATE TABLE audit_logs (
    id BIGINT PRIMARY KEY,
    operator_id BIGINT NOT NULL,
    action VARCHAR(64) NOT NULL,          -- 业务动作，如 account.create / purchase_order.approve
    entity VARCHAR(64) NOT NULL,          -- 资源类型（snake_case）
    entity_id BIGINT NOT NULL,
    before JSONB,                          -- 变更前快照（创建时为 NULL）
    after JSONB,                           -- 变更后快照（删除时为 NULL）
    ip INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_audit_logs_lookup ON audit_logs (entity, entity_id, id DESC);
