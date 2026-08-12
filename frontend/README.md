# Admin 管理后台

现代管理后台 SPA：**React 19 + Rsbuild + TanStack 全家桶 + shadcn/ui（Nord 主题）**。

架构说明见 [docs/architecture.md](docs/architecture.md)（含踩坑记录，改代码前建议先读）。

## 技术栈

| 领域 | 选型 |
|---|---|
| 构建 | Rsbuild / Rspack（React Compiler 自动优化） |
| UI | React 19 + Tailwind CSS 4（Nord 色板，语义变量适配深浅主题） |
| 路由 | @tanstack/react-router（文件式路由 + 自动代码分割） |
| 状态 | @tanstack/react-store |
| 表格 | @tanstack/react-table v9 + react-virtual（虚拟滚动） |
| 表单/校验 | @tanstack/react-form + zod |
| 组件库 | shadcn/ui（base-nova 风格，基于 @base-ui/react） |
| 质量 | Biome + TypeScript |

## 功能特性

- **深浅主题**（Nord）与整站字体缩放，持久化到 localStorage
- **侧边栏导航**：分组折叠菜单、桌面端收起、移动端抽屉
- **Chrome 风格多标签页**：自动打开/去重、激活标签自动滚入视野、右键菜单（刷新 / 关闭当前 / 关闭其他 / 关闭全部）、首页固定不可关闭
- **页面 keep-alive**：切走保留页面状态（输入框、表格筛选等），关闭标签销毁缓存，右键「刷新」重建页面
- **虚拟滚动表格**：固定列、滚动阴影、无限加载
- **认证守卫**：未登录跳转登录页（记录来源页），TanStack Form + Zod 校验

## 命令

```bash
pnpm install        # 安装依赖
pnpm run dev        # 开发服务器（http://localhost:3000）
pnpm run build      # 生产构建
pnpm run preview    # 预览生产构建
pnpm run typecheck  # tsc --noEmit 类型检查
pnpm run check      # biome check --write（lint + format）
pnpm run format     # biome format --write
```
