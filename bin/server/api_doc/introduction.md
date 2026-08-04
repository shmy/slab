# 统一返回格式

成功响应体为 JSON 业务对象（见各接口 `JsonResponse<T>` schema）；错误响应遵循 RFC 9457 Problem Details（`application/problem+json`），例如：

```json
{
    "type": "urn:slab:problem:invalid-request-body",
    "title": "Invalid request body",
    "status": 400,
    "detail": "字段校验失败",
    "instance": "/api/v1/auth/login",
    "error_code": "invalid_request_body",
    "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736"
}
```

- `type`：错误类型标识（URI/URN）
- 常见取值：`urn:slab:problem:invalid-request-body`、`urn:slab:problem:invalid-path-params`、`urn:slab:problem:invalid-query-params`、`urn:slab:problem:unauthorized`、`urn:slab:problem:domain-error`、`urn:slab:problem:internal-server-error`
- `title`：稳定短标题
- `status`：HTTP 状态码（与响应状态一致）
- `detail`：本次错误的详细说明（通常经 l10n 翻译）
- `instance`：错误发生位置（请求路径）
- `error_code`：稳定机器错误码（便于客户端分支）
- `trace_id`：纯 trace id（32 位十六进制，便于日志链路检索）
