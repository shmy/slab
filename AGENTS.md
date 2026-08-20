# Slab — AI 协作

DDD + 垂直切片。前端是独立 SPA。

## 上下文加载策略（按需渐进式）

**首次对话**：只读取顶层结构（Cargo.toml、feature 目录列表、frontend/package.json），不深入任何模块。

**任务相关加载**：
- 后端：根据任务涉及的 feature，按需加载对应的 `features/<name>/` 和 `<name>_contract/`，失败后扩大到 `cross_domain/` / `libs/` / `infrastructure/`。只在涉及具体修改时才读 `.cursor/rules/backend.mdc`。
- 前端：只读 `frontend/AGENTS.md` 和相关组件，按需加载匹配的 `.cursor/rules/frontend-*.mdc`。
- 禁止预读 `docs/ai/`、`CONTEXT.md`。

**搜索优先级**：
1. 小改动：用 grep/glob 直接定位符号或文件
2. 架构理解：用 code_search（避免并行 subagent）
3. 避免对简单任务也调用 subagent

## 快速响应规则

- 用户给了路径就直接 Read。Grep 符号/字面量，Read 局部。大文件先 Grep 再 `offset`/`limit`。
- **优先使用 fff-mcp** 进行文件搜索：
  - 文件名模糊搜索：用 `fff-mcp.find_files`（比内置 `find_file_by_name` 更灵活）
  - 多模式搜索：用 `fff-mcp.multi_grep`（OR 逻辑，比多次 grep 更高效）
  - 单内容搜索：可优先用 `fff-mcp.grep` 或内置 `grep`
- **禁止** GitNexus（`user-gitnexus` MCP / `gitnexus-*` skill）和 `codebase-memory-mcp`。
- **禁止**整份 Read `frontend/openapi.json`、`api-schema.d.ts`、`routeTree.gen.ts`（已 `.cursorignore`）。Grep 类型名或 `jq` 单 path；刷新 `pnpm gen:api`。

## 构建与测试

- `cargo check` / `test` / `clippy` 必须 `-p <crate>`。
- `just pre_commit` 与 `pnpm run build` 只在提交前。
- 前端日常：`tsc --noEmit` + `pnpm run check`。
- postgres-mcp / fff-mcp：用户没在查库就不要 `GetMcpTools`。

## 依赖管理

**公开面修改**（需扫调用方）：
- `*_contract`、跨域写 Port、`cross_domain/`、被 ≥2 feature 依赖的 `pub`、`FILTER_SCHEMA` / 事件 payload / OpenAPI / DTO、重命名拆分
- 操作：先 `just who-uses <符号>`；≥3 个 crate 先问
- 禁止 find-and-replace 重命名公开符号

**私有面修改**（无需扫调用方）：
- 私有 `execute` / handler / 测试、locale / 样式 / 文档、对照 identity 新建端点

## 特殊技能

审查 / TDD / 难 debug / 合并冲突：用户点名后再读 `.agents/optional-skills/` 下对应目录。
