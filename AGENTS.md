# Slab — AI 协作

DDD + 垂直切片。前端是独立 SPA。

- **后端**：打开 `features/` / `cross_domain/` / `infrastructure/` / `libs/` / `bin/` / `e2e/` 后看注入的 `.cursor/rules/backend.mdc`。不要先读 `docs/ai/`、`CONTEXT.md`。
- **前端**：只看 `frontend/AGENTS.md` 与匹配的 `.cursor/rules/frontend-*.mdc`。不要读后端规则。
- 用户给了路径就直接 Read。Grep 符号 / 字面量，Read 局部。大文件先 Grep 再 `offset`/`limit`。
- **禁止** GitNexus（`user-gitnexus` MCP / `gitnexus-*` skill）和 `codebase-memory-mcp`。
- **禁止**整份 Read `frontend/openapi.json`、`api-schema.d.ts`、`routeTree.gen.ts`（已 `.cursorignore`）。Grep 类型名或 `jq` 单 path；刷新 `pnpm gen:api`。
- `cargo check` / `test` / `clippy` 必须 `-p <crate>`。`just pre_commit` 与 `pnpm run build` 只在提交前。前端日常 `tsc --noEmit` + `pnpm run check`。
- postgres-mcp / fff-mcp：用户没在查库就不要 `GetMcpTools`。
- 公开面（`*_contract`、跨域写 Port、`cross_domain/`、被 ≥2 feature 依赖的 `pub`、`FILTER_SCHEMA` / 事件 payload / OpenAPI / DTO、重命名拆分）先 `just who-uses <符号>`；≥3 个 crate 先问。禁止 find-and-replace 重命名公开符号。私有 `execute` / handler / 测试、locale / 样式 / 文档、对照 identity 新建端点：不用扫调用方。
- 审查 / TDD / 难 debug / 合并冲突：用户点名后再读 `.agents/optional-skills/` 下对应目录。
