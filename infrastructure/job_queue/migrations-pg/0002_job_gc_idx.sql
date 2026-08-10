-- 供 JobGc 按保留期清理终态行（Done / Failed）：
-- worker_jobs_fetch_idx 以 job_type 打头，无法服务 status + done_at 过滤；
-- partial index 只覆盖终态行，体积小、写入开销低。
CREATE INDEX worker_jobs_gc_idx ON worker_jobs (done_at)
    WHERE status IN ('Done', 'Failed');
