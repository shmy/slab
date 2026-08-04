# Health 域 HTTP E2E（Hurl 8）
#
# 覆盖公开探针：/livez、/readyz、/healthz
# 约定见 docs/E2E_HURL.md
#
#   hurl --test --variables-file e2e/env e2e/health.hurl

###############################################################################
# 存活探针：进程可响应
GET {{base_url}}/livez

HTTP 200
[Asserts]
jsonpath "$.status" == "ok"

###############################################################################
# 就绪探针：数据库可用时 ready=true
GET {{base_url}}/readyz

HTTP 200
[Asserts]
jsonpath "$.ready" == true

###############################################################################
# 健康探针：返回进程、构建与连接池状态
GET {{base_url}}/healthz

HTTP 200
[Asserts]
jsonpath "$.process.uptime_seconds" >= 0
jsonpath "$.postgres.max_connections" > 0
jsonpath "$.postgres.size" >= 0
jsonpath "$.postgres.num_idle" >= 0
jsonpath "$.postgres.is_closed" == false
