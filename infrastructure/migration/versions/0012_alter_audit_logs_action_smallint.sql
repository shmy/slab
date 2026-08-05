-- 变更历史 action：业务动作字符串（"account.create" 等）改为枚举 SMALLINT
-- （AuditAction: Created=1 / Updated=2 / Deleted=3）。旧字符串数据无法映射，删除。
DELETE FROM audit_logs WHERE action !~ '^[0-9]+$';
ALTER TABLE audit_logs ALTER COLUMN action TYPE SMALLINT USING action::smallint;
