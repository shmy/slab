# HTTP E2E（Hurl）

面向 **已启动的 `server` + 可达 PostgreSQL**（与迁移、种子数据一致）的 **黑盒 HTTP 测试**。与 Rust 侧的 `#[sqlx::test]` 互补：前者验 **真实 Axum 栈、中间件、序列化与 RFC 9457 错误体**；后者验 **execute / 库表 / 队列** 且无需起监听端口。

## 依赖

- [Hurl](https://hurl.dev) **8.x**（本仓库用 Hurl 8 语法，例如 `jsonpath`、`--test`）。
- 本机或可访问的 API 服务（默认 `http://127.0.0.1:8080`，以变量为准）。
- 数据库已执行迁移。

## 运行

```bash
just e2e
```

`justfile` 中 **按固定顺序** 执行各 `e2e/*.hurl`（当前顺序：`file → identity → erp_foundations → health`），文件之间 **`sleep 2`**，避免 `bin/server/app.rs` 中 **`GovernorLayer`** 在同一次进程里对多域用例连续轰炸时返回 **429**（若省略间隔、或用单条命令 `hurl ... e2e/` 一次扫目录内全部 `.hurl`，容易误报失败）。

仅调试某一域时可直接：

```bash
hurl --test --variables-file e2e/env e2e/identity.hurl
```

- **`--test`**：只输出通过/失败摘要与统计，不把每一步 response body 刷满终端。
- **默认（无 `--test`）**：仅将 **最后一次** 响应的 body 输出到 stdout（见 `hurl --help` 中 `--no-output` 的说明）。

调试单文件：

```bash
hurl --verbose --variables-file e2e/env e2e/identity.hurl
```

## 变量（`e2e/env`）

使用 **Java properties** 格式（`key=value`），由 `--variables-file` 注入。

| 变量                                           | 含义                                                                                        |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `base_url`                                     | API 根，如 `http://127.0.0.1:8080`                                                          |
| `e2e_admin_phone` / `e2e_admin_password`       | 种子管理员（与迁移一致）                                                                     |
| `e2e_account_phone` / `e2e_account_password` | Identity：库内未占用的账号手机号/密码（勿与管理员重复）；用例内会创建再删除                 |

命令行可叠加覆盖，例如：

```bash
hurl --test --variables-file e2e/env --variable base_url=http://127.0.0.1:3000 e2e/identity.hurl
```

## 现有用例

### `e2e/identity.hurl`

单文件串联 Identity 主要 HTTP 切片（约 35 条 HTTP）：

- 公开：`POST /api/v1/identity/login`、`POST /api/v1/identity/refresh`
- 需 Bearer：`/api/v1/accounts` 的创建/查询/搜索/更新/删除、`POST /api/v1/identity/logout`
- 负例：假密码、`refresh_token_invalid`、`account_duplicated`、`account_not_found`、登出后 `access_token_revoked`
- 参数边界：`invalid_request_body`（非法 JSON、缺字段、手机号/密码长度与号段）、`invalid_path_params` / `invalid_query_params`、`access_token_missing`、空/异常 `refresh_token` 等（校验类请求带 `Accept-Language: en-US` 以便断言英文 `detail`）

流程概要：**管理员登录并 refresh → 创建临时账号 → 搜索/GET/PUT → 新账号登录与 refresh → GET/登出/吊销后 GET → 管理员删除 → GET 确认 not_found**；边界与负例穿插在主流程前后。

### `e2e/file.hurl`

- `POST /api/v1/files/images`，`multipart/form-data`，字段名 **`image`**；Hurl 使用 **`[Multipart]`** 段。
- 依赖 **S3 可写**（`AppState` 中配置）；否则成功上传步会在 `write` 失败。
- 夹具：`e2e/fixtures/one-by-one.png`（极小 PNG）、`e2e/fixtures/not-a-real.png`（纯文本内容、扩展名 `.png`，用于 `file_not_image`）。

### `e2e/erp_foundations.hurl`

- 覆盖 Phase 1 主数据和库存域的核心 HTTP 切片（约 25 条 HTTP）：
- 物料分类树形管理（创建子分类 → 获取树）
- 物料 CRUD + 搜索 + 自动编码（`RAW-000001` / `PRD-000001`）
- 单位换算 + 成本记录
- 客户/供应商 CRUD + 搜索 + 自动编码（`C-000001` / `S-000001`）
- 仓库 CRUD + 库存台账查询 + 期初库存录入
- 依赖管理员登录获取 token，流程覆盖成功路径和关键断言

### `e2e/health.hurl`

- 覆盖公开探针：`GET /livez`、`GET /readyz`、`GET /healthz`。
- `livez` 断言 `status = ok`（仅验证进程可响应，不访问 DB）。
- `readyz` 断言 `ready = true`。
- `healthz` 断言进程 uptime 与连接池关键字段存在且格式正确。

---

### 须记牢：错误响应与断言约定（维护 E2E 必读本节）

1. **统一 Problem Details**  
   错误时响应为 `application/problem+json`，字段含义见 `bin/server/app.rs` OpenAPI 说明。`locale` 中间件（`infrastructure/locale/middleware.rs`）会把 `WebError` 转成 `ProblemDetails`：**`error_code` 取机器码**，**`detail` 为经 Fluent 翻译后的文案**（内层 key 来自 `err.to_string()`，校验失败时常为 validify 的 l10n key，如 `phone_number_too_short`）。

2. **`error_code` 分层（不要和 `detail` 混用）**
   - **请求体 / Path / Query 校验失败**：`invalid_request_body`、`invalid_path_params`、`invalid_query_params`
   - **领域/鉴权**：`account_invalid_credentials`、`refresh_token_invalid`、`account_duplicated`、`account_not_found`、`file_not_image`、`access_token_missing`、`access_token_invalid`、`access_token_revoked` 等（`error_code` 即稳定 key）。

3. **为何边界用例要带 `Accept-Language: en-US`**  
   对依赖 **`detail` 子串** 的断言（如 `contains "Phone number is too short"`），必须固定语言，否则 `zh-CN` 下 `detail` 为中文会导致用例不稳定。默认不带该头时，中间件按 `Accept-Language` / cookie / `en-US` 默认（见 `locale/middleware.rs`）。**校验类边界请求显式加 `Accept-Language: en-US`**，并保证英文文案与 `infrastructure/locale/locales/en-US/{shared,account}.ftl` 一致。

4. **`Authorization` 单独一行（无 token）**  
   解析结果因 Axum/头格式可能落在 `access_token_missing` 或 `access_token_invalid`；`identity.hurl` 使用 `matches /^access_token_(missing|invalid)$/` 兼容。

5. **实现快速对照（改代码时同步想 E2E）**  
   | 行为 | 代码入口 |
   |------|----------|
   | JSON + validify | `infrastructure/web/extract/valid_json.rs` → `WebError::InvalidRequestBody` |
   | Path + validify | `infrastructure/web/extract/valid_path.rs` → `InvalidPathParams` |
   | Query + validify | `infrastructure/web/extract/valid_query.rs` → `InvalidQueryParams` |
   | 机器码与 HTTP 状态 | `infrastructure/web/error.rs` |

### `identity.hurl` 边界与负例清单（与文件内注释一一对照）

| 场景                             | 方法 / 路径要点                      | 预期 HTTP | `error_code`（或约定）         | 备注                                              |
| -------------------------------- | ------------------------------------ | --------- | ------------------------------ | ------------------------------------------------- |
| 手机号过短（trim 后仍过短）      | `POST .../identity/login`            | 400       | `invalid_request_body`         | `en-US` + `detail` 含 _Phone number is too short_ |
| 非大陆 11 位号段                 | `POST .../login`，如 `128...`         | 400       | `invalid_request_body`         | `detail` 含 _mainland China_                      |
| 密码过短（<4）                   | `POST .../login`                     | 400       | `invalid_request_body`         | _Password is too short_                           |
| 密码过长（>64）                  | `POST .../login`                     | 400       | `invalid_request_body`         | _Password is too long_（65 个 `a`）               |
| 假密码                           | `POST .../login`                     | 400       | `account_invalid_credentials`  | 领域错误                                          |
| 非法 JSON 体                     | `POST .../login`                     | 400       | `invalid_request_body`         | 仅断言 `type`，不绑 `detail`                      |
| refresh 空串 / 乱 token          | `POST .../refresh`                   | 400       | `refresh_token_invalid`        |                                                   |
| refresh 缺字段 `{}`              | `POST .../refresh`                   | 400       | `invalid_request_body`         |                                                   |
| 重复使用已消费 refresh           | `POST .../refresh`                   | 400       | `refresh_token_invalid`        |                                                   |
| 无 `Authorization`               | `GET .../accounts`                   | 401       | `access_token_missing`         |                                                   |
| `Authorization` 无 token          | `GET .../accounts`                   | 401       | `missing` 或 `invalid`         | 见上正则                                          |
| Path `id` 非整数                 | `GET .../accounts/not-an-int-id`     | 400       | `invalid_path_params`          |                                                   |
| `limit` 非数字                   | `GET .../accounts?limit=not-a-number`| 400       | `invalid_query_params`         |                                                   |
| 创建：手机过短 / 密码过短 / `{}` | `POST .../accounts`                  | 400       | `invalid_request_body`         | 前两者用 `en-US` + `detail`                       |
| 重复手机号                       | `POST .../accounts`                  | 400       | `account_duplicated`           |                                                   |
| 更新：非法手机号                 | `PUT .../accounts/{id}`              | 400       | `invalid_request_body`         | `12000000000`，_mainland China_                   |
| 登出无令牌                       | `POST .../identity/logout`           | 401       | `access_token_missing`         |                                                   |
| 登出后再访问                     | `GET .../accounts/{id}`              | 401       | `access_token_revoked`         | `type` = unauthorized                             |
| 删除后再查询                     | `GET .../accounts/{id}`              | 400       | `account_not_found`            |                                                   |

**改 Fluent 英文、Validify key 或 Problem 映射时：先跑 `just e2e`，再按需改 `identity.hurl` 与上表。**

---

## 新增 E2E

1. 在 `e2e/` 增加 `*.hurl`；**若用例较多，在 `justfile` 的 `e2e` 列表中注册**并考虑是否需延长文件间间隔以规避 429。
2. 新变量写入 `e2e/env`（或文档化单独 env 文件），并更新本文或 `docs/README.md` 索引。
3. 新增/改种子数据或路由时，同步修正对应 `.hurl`，避免 CI 或本地 `just e2e` 漂移失败。

## 与 Rust 集成测试的分工

|      | Hurl E2E                        | `#[sqlx::test]` + `testing::build(pool)` |
| ---- | ------------------------------- | ---------------------------------------------- |
| 依赖 | 真实 server + DB                | 可连接的 PostgreSQL（通常 `DATABASE_URL`）     |
| 侧重 | HTTP 契约、中间件、整条用户路径 | `execute`、表数据、`_pg_events` / `caches` 等 |

写测清单与 crate 约定见 `.agents/skills/rust-tests`。

## 运行前置

e2e 使用固定手机号/编码（非幂等），连续运行前需清空业务表并重启服务（重新种子管理员账号）：

```bash
# TRUNCATE 全部业务表（accounts 在内）→ 重启 server（before_starting 重新种子 admin）
```

否则第二次运行会因 `account_duplicated` / 残留未付发票导致账龄、余额断言失败。
