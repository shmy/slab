# Slab — AI 协作上下文

DDD + 垂直切片（endpoint + repository）+ contract 公共表面。
跨域通道：只读 Port、Outbox/Inbox 事件、模块内领域事件；**例外**——跨域同事务写 Port（`audit_contract::AuditService`，动词 `record_create` / `record_updated` / `record_deleted`）。

约定已注入。**禁止**开场 Read 本文件 / conventions / backend / `CONTEXT.md` / skill。

## 前端（`frontend/`）

独立 SPA。约定只看 `frontend/AGENTS.md`。不要为改前端去读后端规则。

## 最小打开集

| 任务 | 打开 | 到此停止 |
|------|------|----------|
| 改已有 `execute` / handler | 该 endpoint，Read 到 `mod tests` 之前 | skill、conventions、CONTEXT、邻域 endpoint |
| 同文件加断言 | 同文件 `mod tests` | `rust-tests` skill |
| 新错误 key | 该 `error.rs` + `en-US`/`zh-CN` 各一行 | 全文 conventions |
| 对照蓝本新建端点 | identity 蓝本**只到 `mod tests`** + `backend.md`「新增功能」 | 别的域的 endpoint |
| 改 contract / 事件 / DTO | `just who-uses <符号>`，只 Read 命中的调用处 | 把每个命中文件通读；≥3 个 crate 先停下来问 |

用户已给路径则直接 Read，不要先搜。打开后端文件后编码/测试摘要由 `.cursor/rules/backend.mdc` 注入。

## 代码导航

Grep 符号 / 字面量，Read 局部。

**禁止** GitNexus：不要调用 `user-gitnexus` MCP，不要加载 `gitnexus-*` skill。同样不要用 `codebase-memory-mcp`。

公开面改动前 `just who-uses <符号>` 确认调用方，调用面大时先警告再改：`*_contract`、跨域写 Port、`cross_domain/` 或被 ≥2 个 feature 依赖的 `pub` API、重命名/移动/拆分、`FILTER_SCHEMA` / 事件 payload / OpenAPI / DTO 形状。禁止 find-and-replace 重命名公开符号。

私有 `execute` / handler / 测试（不改公开签名）、注释/文案/locale/样式/文档/配置、对照 identity 蓝本新建端点：不用先扫调用方。

## 按需文档

| 场景 | 才读 |
|------|------|
| 新建域、新加 endpoint 文件、加 Job/流程/事件 | [docs/ai/backend.md](docs/ai/backend.md) 对应小节 |
| 写错误 key、HTTP 方法、可筛字段、陷阱拿不准 | [docs/ai/conventions.md](docs/ai/conventions.md) |
| 新增测试模块、某端点第一次补集成测试、写 Hurl | `.agents/skills/rust-tests/SKILL.md` |
| 用户明确要求 TDD | `.agents/skills/tdd/SKILL.md` |
| 改表格 / keep-alive / 主题，且 `frontend/AGENTS.md` 不够 | `frontend/docs/architecture.md` §5 |
| 引入或争议领域术语 | `CONTEXT.md` |

改已有 endpoint 的 `execute` / handler / 同文件加断言：backend 规则摘要够用，不要先读 skill / conventions。

grill / wayfinder / to-spec / prototype / research / domain-modeling / codebase-design 在 `.agents/optional-skills/`，点名再读。索引：[.agents/skills-reference.md](.agents/skills-reference.md)。

## 读文件 / 命令

- 改 endpoint：Read 到 `mod tests` 之前
- **禁止**整份 Read `frontend/openapi.json` 或 `frontend/src/lib/api-schema.d.ts`（已在 `.cursorignore`）。Grep 类型名 / `jq` 单 path；刷新契约用 `pnpm gen:api`
- 大文件 Grep 符号后 Read `offset`/`limit`
- `cargo check -p <crate> --message-format=short`、`cargo test -p <crate> --quiet`；**禁止**无 `-p` 的 workspace test / clippy
- `just pre_commit`、`pnpm run build` 只在提交前；前端日常 `tsc --noEmit` + `pnpm run check`
- postgres-mcp / fff-mcp：用户没在查库就不要 `GetMcpTools`

## 依赖规则

```
features/{domain} (切片)
 ├──→ {domain}_contract ← 自己的公共表面
 └──→ 其他 *_contract（只读 Port）

cross_domain/（共享业务件）
 ├──→ 被 ≥2 个 feature 依赖的**业务规则**
 └──→ 现状：approval / doc_numbering / costing / inventory_ledger

禁止：contract 互依；切片依赖其他 features/{other} runtime；往 cross_domain/ 塞技术件（归 infrastructure/）
```

验证：`cargo test -p server arch_test`

## 常用命令

| 命令 | 用途 |
|------|------|
| `cargo check -p <crate> --message-format=short` / `cargo test -p <crate> --quiet` | 单 crate |
| `just who-uses <符号>` | 公开面调用文件列表（不含行） |
| `cargo test -p server arch_test` | 架构边界（无需 DB） |
| `just e2e` / `just sqlx_up` / `just pre_commit` | E2E / 迁移 / 提交前 |
| `cd frontend && pnpm run dev` | 前端（端口 3000） |
