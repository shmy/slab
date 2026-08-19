# 前端

React 19 + Rsbuild + TanStack Router/Store/Table + shadcn（base-nova）+ Tailwind 4 + Nord + xior。

## 命令

改完：`pnpm exec tsc --noEmit && pnpm run check`。`pnpm run build` 只在提交前。dev：`pnpm run dev`（端口 3000，`/api` → `127.0.0.1:8081`）。不要跑 `pnpm dlx @tanstack/intent`。

## 约定

- 路由：`src/routes/` 文件式；`routeTree.gen.ts` 勿手改。新页面 `src/routes/_app/xxx.tsx`，根节点 `flex min-h-0 flex-1 flex-col`；要保活加 `staticData: { keepAlive: true }`。
- 导航：`src/components/SidebarNav.tsx` 的 `navItems`；面包屑派生。
- 侧栏始终深色；用语义色（`canvas/surface/line/ink/accent`），禁止 `dark:`。
- 状态：`src/store/`（`createStore` + `useSelector`）。
- 对接后端：Grep `src/lib/api-schema.d.ts` 类型名，或 `jq` 取 `openapi.json` 单条 path。禁止整份 Read 这两个文件。刷新：`pnpm gen:api`。不够再看 `e2e/*.hurl`，最后才读 Rust 契约面。
- Biome：含 `[class*='size-']` 的字符串用外双引号、内单引号。
- 改完不生效：重启 dev server，清 `node_modules/.cache`。
- 陷阱不够再读 `docs/architecture.md` §5。Keep-alive / Auth / Table 打开对应文件时会注入 `.cursor/rules/frontend-*.mdc`。
