# Identity 域全 HTTP 切片 E2E（Hurl 8）
#
# 依赖：迁移种子管理员（e2e_admin_phone / e2e_admin_password）；
#       e2e_account_phone 须未占用，且勿与管理员手机号相同。
#
# 错误码 / detail / en-US 断言约定与边界清单见：docs/E2E_HURL.md（改 l10n 或校验时先读再改本文件）
#
#   hurl --test --variables-file e2e/env e2e/identity.hurl
#   just e2e

###############################################################################
# 管理员登录
POST {{base_url}}/api/v1/identity/login
Content-Type: application/json
```json
{
  "phone": "{{e2e_admin_phone}}",
  "password": "{{e2e_admin_password}}"
}
```

HTTP 200
[Captures]
admin_at_initial: jsonpath "$.access_token"
admin_rt_initial: jsonpath "$.refresh_token"
[Asserts]
jsonpath "$.token_type" == "Bearer"
jsonpath "$.access_token" matches /^.+$/
jsonpath "$.refresh_token" matches /^.+$/

###############################################################################
# 当前账号自省（profile/current）
GET {{base_url}}/api/v1/profile/current
Authorization: Bearer {{admin_at_initial}}

HTTP 200
[Asserts]
jsonpath "$.phone" == "{{e2e_admin_phone}}"
jsonpath "$.privileged" == true

###############################################################################
# profile/current：无令牌 → 401
GET {{base_url}}/api/v1/profile/current

HTTP 401
[Asserts]
jsonpath "$.error_code" == "access_token_missing"

###############################################################################
# 登录：手机号过短
POST {{base_url}}/api/v1/identity/login
Accept-Language: en-US
Content-Type: application/json
```json
{
  "phone": " 138 ",
  "password": "admin123!"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"
jsonpath "$['type']" == "urn:slab:problem:invalid-request-body"
jsonpath "$.detail" contains "Phone number is too short"

###############################################################################
# 登录：非大陆 11 位号段
POST {{base_url}}/api/v1/identity/login
Accept-Language: en-US
Content-Type: application/json
```json
{
  "phone": "12812345678",
  "password": "admin123!"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"
jsonpath "$.detail" contains "mainland China"

###############################################################################
# 登录：密码过短（≥4 字符）
POST {{base_url}}/api/v1/identity/login
Accept-Language: en-US
Content-Type: application/json
```json
{
  "phone": "{{e2e_admin_phone}}",
  "password": "abc"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"
jsonpath "$.detail" contains "Password is too short"

###############################################################################
# 登录：密码过长（≤64）
POST {{base_url}}/api/v1/identity/login
Accept-Language: en-US
Content-Type: application/json
```json
{
  "phone": "{{e2e_admin_phone}}",
  "password": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"
jsonpath "$.detail" contains "Password is too long"

###############################################################################
# 管理员假密码
POST {{base_url}}/api/v1/identity/login
Content-Type: application/json
```json
{
  "phone": "{{e2e_admin_phone}}",
  "password": "wrong-password-for-hurl-9z"
}
```

HTTP 400
[Asserts]
header "Content-Type" contains "application/problem+json"
jsonpath "$.error_code" == "account_invalid_credentials"

###############################################################################
# 登录：非法 JSON
POST {{base_url}}/api/v1/identity/login
Content-Type: application/json
```
{"phone": "13888888888", "password":
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"
jsonpath "$['type']" == "urn:slab:problem:invalid-request-body"

###############################################################################
# 刷新：空 refresh_token（仍反序列化成功，业务拒绝）
POST {{base_url}}/api/v1/identity/refresh
Content-Type: application/json
```json
{
  "refresh_token": ""
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "refresh_token_invalid"

###############################################################################
# 刷新：乱填 token
POST {{base_url}}/api/v1/identity/refresh
Content-Type: application/json
```json
{
  "refresh_token": "not-a-real-refresh-token-hurl-9z"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "refresh_token_invalid"

###############################################################################
# 刷新：缺少 refresh_token 字段
POST {{base_url}}/api/v1/identity/refresh
Content-Type: application/json
```json
{}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"

###############################################################################
# 管理员 refresh（旋转）
POST {{base_url}}/api/v1/identity/refresh
Content-Type: application/json
```json
{
  "refresh_token": "{{admin_rt_initial}}"
}
```

HTTP 200
[Asserts]
jsonpath "$.token_type" == "Bearer"
jsonpath "$.access_token" != "{{admin_at_initial}}"
jsonpath "$.refresh_token" != "{{admin_rt_initial}}"
[Captures]
admin_at: jsonpath "$.access_token"
admin_rt: jsonpath "$.refresh_token"

###############################################################################
# 已消费的 admin refresh 再刷 → 失败
POST {{base_url}}/api/v1/identity/refresh
Content-Type: application/json
```json
{
  "refresh_token": "{{admin_rt_initial}}"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "refresh_token_invalid"

###############################################################################
# 受保护：无 Authorization
GET {{base_url}}/api/v1/accounts?limit=5

HTTP 401
[Asserts]
jsonpath "$.error_code" == "access_token_missing"

###############################################################################
# 受保护：Authorization 仅 `Bearer` 无 token（实现可能报 missing 或 invalid）
GET {{base_url}}/api/v1/accounts?limit=5
Authorization: Bearer

HTTP 401
[Asserts]
jsonpath "$.error_code" matches /^access_token_(missing|invalid)$/

###############################################################################
# Path：id 无法解析为整数
GET {{base_url}}/api/v1/accounts/not-an-int-id
Authorization: Bearer {{admin_at}}

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_path_params"

###############################################################################
# Query：limit 非数字
GET {{base_url}}/api/v1/accounts?limit=not-a-number
Authorization: Bearer {{admin_at}}

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_query_params"

###############################################################################
# 创建：手机号过短
POST {{base_url}}/api/v1/accounts
Accept-Language: en-US
Authorization: Bearer {{admin_at}}
Content-Type: application/json
```json
{
  "name": "X",
  "phone": "139",
  "password": "admin123!"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"
jsonpath "$.detail" contains "Phone number is too short"

###############################################################################
# 创建：密码过短
POST {{base_url}}/api/v1/accounts
Accept-Language: en-US
Authorization: Bearer {{admin_at}}
Content-Type: application/json
```json
{
  "name": "X",
  "phone": "{{e2e_account_phone}}",
  "password": "ab"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"
jsonpath "$.detail" contains "Password is too short"

###############################################################################
# 创建：缺必填字段
POST {{base_url}}/api/v1/accounts
Authorization: Bearer {{admin_at}}
Content-Type: application/json
```json
{}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"

###############################################################################
# 创建员工（employee_create）
POST {{base_url}}/api/v1/accounts
Authorization: Bearer {{admin_at}}
Content-Type: application/json
```json
{
  "name": "E2E Worker",
  "phone": "{{e2e_account_phone}}",
  "password": "{{e2e_account_password}}"
}
```

HTTP 200
[Captures]
new_account_id: jsonpath "$.id"
[Asserts]
jsonpath "$.name" == "E2E Worker"

###############################################################################
# 重复手机号 → account_duplicated
POST {{base_url}}/api/v1/accounts
Authorization: Bearer {{admin_at}}
Content-Type: application/json
```json
{
  "name": "Dup",
  "phone": "{{e2e_account_phone}}",
  "password": "{{e2e_account_password}}"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "account_duplicated"

###############################################################################
# 游标分页搜索（employee_search）
GET {{base_url}}/api/v1/accounts?q={{e2e_account_phone}}&limit=20
Authorization: Bearer {{admin_at}}

HTTP 200
[Asserts]
jsonpath "$.items" count >= 1
jsonpath "$.items[0].id" == "{{new_account_id}}"
jsonpath "$.items[0].phone" == "{{e2e_account_phone}}"

###############################################################################
# 按 id 查询（employee_get）
GET {{base_url}}/api/v1/accounts/{{new_account_id}}
Authorization: Bearer {{admin_at}}

HTTP 200
[Asserts]
jsonpath "$.id" == "{{new_account_id}}"
jsonpath "$.phone" == "{{e2e_account_phone}}"
jsonpath "$.name" == "E2E Worker"

###############################################################################
# 更新（employee_update）
PATCH {{base_url}}/api/v1/accounts/{{new_account_id}}
Authorization: Bearer {{admin_at}}
Content-Type: application/json
```json
{
  "name": "E2E Worker Updated",
  "phone": "{{e2e_account_phone}}"
}
```

HTTP 200
[Asserts]
jsonpath "$.id" == "{{new_account_id}}"
jsonpath "$.name" == "E2E Worker Updated"
jsonpath "$.phone" == "{{e2e_account_phone}}"

###############################################################################
# 更新：非法手机号
PATCH {{base_url}}/api/v1/accounts/{{new_account_id}}
Accept-Language: en-US
Authorization: Bearer {{admin_at}}
Content-Type: application/json
```json
{
  "name": "Bad Phone",
  "phone": "12000000000"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "invalid_request_body"
jsonpath "$.detail" contains "mainland China"

###############################################################################
# 新员工登录
POST {{base_url}}/api/v1/identity/login
Content-Type: application/json
```json
{
  "phone": "{{e2e_account_phone}}",
  "password": "{{e2e_account_password}}"
}
```

HTTP 200
[Captures]
nu_at_initial: jsonpath "$.access_token"
nu_rt_initial: jsonpath "$.refresh_token"
[Asserts]
jsonpath "$.token_type" == "Bearer"

###############################################################################
# 新员工假密码
POST {{base_url}}/api/v1/identity/login
Content-Type: application/json
```json
{
  "phone": "{{e2e_account_phone}}",
  "password": "wrong-password-for-hurl-9z"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "account_invalid_credentials"

###############################################################################
# 新员工 refresh
POST {{base_url}}/api/v1/identity/refresh
Content-Type: application/json
```json
{
  "refresh_token": "{{nu_rt_initial}}"
}
```

HTTP 200
[Asserts]
jsonpath "$.access_token" != "{{nu_at_initial}}"
jsonpath "$.refresh_token" != "{{nu_rt_initial}}"
[Captures]
nu_at: jsonpath "$.access_token"
nu_rt: jsonpath "$.refresh_token"

###############################################################################
# 已消费的 refresh
POST {{base_url}}/api/v1/identity/refresh
Content-Type: application/json
```json
{
  "refresh_token": "{{nu_rt_initial}}"
}
```

HTTP 400
[Asserts]
jsonpath "$.error_code" == "refresh_token_invalid"

###############################################################################
# 持新员工令牌访问 GET
GET {{base_url}}/api/v1/accounts/{{new_account_id}}
Authorization: Bearer {{nu_at}}

HTTP 200
[Asserts]
jsonpath "$.name" == "E2E Worker Updated"

###############################################################################
# 登出：无令牌
POST {{base_url}}/api/v1/identity/logout

HTTP 401
[Asserts]
jsonpath "$.error_code" == "access_token_missing"

###############################################################################
# 新员工登出（employee_logout）
POST {{base_url}}/api/v1/identity/logout
Authorization: Bearer {{nu_at}}

HTTP 200
[Asserts]
jsonpath "$.logged_out" == true

###############################################################################
# 登出后再访 → 401
GET {{base_url}}/api/v1/accounts/{{new_account_id}}
Authorization: Bearer {{nu_at}}

HTTP 401
[Asserts]
jsonpath "$['type']" == "urn:slab:problem:unauthorized"
jsonpath "$.error_code" == "access_token_revoked"

###############################################################################
# 管理员删除员工（employee_delete）
DELETE {{base_url}}/api/v1/accounts/{{new_account_id}}
Authorization: Bearer {{admin_at}}

HTTP 200

###############################################################################
# 删除后查询 → account_not_found
GET {{base_url}}/api/v1/accounts/{{new_account_id}}
Authorization: Bearer {{admin_at}}

HTTP 400
[Asserts]
jsonpath "$.error_code" == "account_not_found"
