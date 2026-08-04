-- 放宽账号姓名字段长度：VARCHAR(16) 对姓名过紧（e2e 更新姓名超长触发 500）。
ALTER TABLE accounts ALTER COLUMN name TYPE VARCHAR(64);
