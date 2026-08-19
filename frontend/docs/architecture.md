# 前端架构文档

> 管理后台 SPA，基于 Rsbuild + React 19 + TanStack 全家桶，Nord 主题风格。

## 1. 技术栈

| 领域 | 选型 | 版本 | 说明 |
|---|---|---|---|
| 构建 | Rsbuild / Rspack | 2.x | 启用 React Compiler 自动优化 |
| UI | React | 19.2 | |
| 路由 | @tanstack/react-router | 1.170 | 文件式路由 + `routeTree.gen.ts` 自动生成 + 自动代码分割 |
| 状态 | @tanstack/react-store | 0.11 | `createStore` / `useSelector` |
| 表格 | @tanstack/react-table | 9.1 | v9 重写版 API（`useTable` + `tableFeatures`） |
| 表单 | @tanstack/react-form | 1.33 | `useForm` + `form.Field` + Standard Schema |
| 校验 | zod | 4.x | 原生兼容 Standard Schema v1 |
| 组件库 | shadcn/ui | 4.16 | base-nova 风格，基于 @base-ui/react；变量映射到 Nord |
| 类名工具 | clsx + tailwind-merge | | `cn()` 在 src/lib/utils.ts |
| 虚拟滚动 | @tanstack/react-virtual | 3.14 | |
| HTTP | xior | 0.8 | fetch 封装（axios 兼容 API）：拦截器、401 单飞刷新 |
| 图标 | lucide-react | 1.31 | |
| 样式 | Tailwind CSS | 4.x | `@theme` 自定义色板 |
| 质量 | Biome / TypeScript | 2.4 / 7 | lint + format + typecheck |

## 2. 目录结构

```
src/
├── index.tsx                  # 入口：createRouter（preload/scrollRestoration）+ Toaster
├── index.css                  # Tailwind 4：Nord 色板 + 语义变量 + shadcn 变量映射 + 深浅模式
├── lib/
│   ├── utils.ts                # cn()（clsx + tailwind-merge）+ maskPhone 手机号脱敏
│   ├── api.ts                  # xior 客户端：Bearer 附加、401 单飞刷新、Problem Details 归一化（导出 authRequest 供域模块复用）
│   ├── customers.ts            # 客户 CRUD 域模块（列表/详情/创建/更新/删除，cursor 分页）
│   ├── accounts.ts             # 账号（用户）CRUD 域模块（列表/详情/创建/更新/删除）
│   ├── audit.ts                # 审计日志查询（按实体 entity+entity_id）
│   ├── validators.ts           # 共享 zod schema（passwordSchema，与后端 Password 规则对齐）
│   ├── token.ts                # 令牌/用户本地存储 + JWT payload 解码（无 UI 依赖，auth 与 api 共用）
│   └── api-schema.d.ts         # openapi.json 生成的契约类型（勿手改；重新生成见 §8）
├── openapi.json                # 后端 OpenAPI 契约快照（pnpm gen:api 刷新，scripts/fetch-openapi.mjs）
├── components/
│   ├── ui/                     # shadcn 组件（button/input/checkbox/badge/avatar/dropdown-menu/context-menu/dialog/sheet/sonner/tooltip）
│   ├── ThemeToggle.tsx        # 主题切换
│   ├── FontSizeToggle.tsx     # 整站字体大小
│   ├── FullscreenToggle.tsx   # 顶栏全屏切换（fullscreenchange 同步图标，不支持时隐藏）
│   ├── FilterBar.tsx          # 条件构建器（搜索框 debounce + React Query Builder 布尔树：分组/and-or/增删/排序由 react-querybuilder 管理；RSQL 序列化在 lib/filters.ts）
│   ├── UserMenu.tsx           # 侧边栏用户区菜单（个人信息/退出登录+确认）
│   ├── FieldError.tsx         # 表单字段错误展示（TanStack Form field，触碰门控 + 去重）
│   ├── SidebarNav.tsx         # 侧边栏导航（分组 submenu、折叠态 popup；导出 navItems/flatNav）
│   ├── PageTabs.tsx           # Chrome 风格多标签页（右键菜单：刷新/关闭当前/关闭其他/关闭全部）
│   ├── keep-alive.tsx         # 页面 keep-alive（KeepAliveProvider/KeepAliveOutlet/useKeepAlive）
│   ├── RowActions.tsx         # 表格行操作：查看详情 + ⋯ 菜单（菜单项声明式数组，两页共用）
│   ├── AuditHistory.tsx       # 实体变更历史抽屉（审计日志 + 字段级 diff，通用组件）
│   ├── TextField.tsx          # TanStack Form 文本字段控件（label + Input + FieldError 一体）
│   ├── DataTable.tsx          # 业务表格：VirtualTable 薄封装（固定 features + 声明式列配置，见 §4.7）
│   └── VirtualTable.tsx       # 通用虚拟表格（核心组件，见 §4.4）
├── store/
│   ├── auth.ts                # 认证：createStore + localStorage 持久化
│   ├── theme.ts               # 主题模式（html[data-theme] 应用）
│   ├── fontSize.ts            # 字体档位（html style.fontSize 应用）
│   ├── sidebar.ts             # 侧边栏折叠状态（localStorage 持久化）
│   └── tabs.ts                # 多标签页列表（会话级，addTab 去重 / removeTab）
└── routes/
    ├── __root.tsx             # 根路由：QueryClientProvider（react-query）+ KeepAliveProvider 包裹 <Outlet />
    ├── login.tsx              # 登录页：左右分栏 + TanStack Form + Zod
    ├── _app.tsx               # 布局路由：登录守卫 + 侧边栏/顶栏/多标签页/KeepAliveOutlet
    └── _app/
        ├── index.tsx          # 仪表盘
        ├── customers/         # 客户管理（真实 CRUD：TanStack Query 无限滚动 + DataTable + 创建/编辑 Dialog + 删除确认）
        ├── profile.tsx        # 个人信息
        ├── content/           # 文章管理、分类管理
        └── settings/          # 通用设置、权限管理
```

## 3. 认证流

### 接口契约（后端 `identity` 域；扁平 JSON / Problem Details）

| 动作 | 请求 | 响应 |
|---|---|---|
| 登录 | `POST /api/v1/identity/login` `{phone, password}` | `{access_token, refresh_token, token_type, expires_in}` |
| 刷新 | `POST /api/v1/identity/refresh` `{refresh_token}` | 同上（refresh token 轮换，旧值立即失效） |
| 登出 | `POST /api/v1/identity/logout?access_token=…`（sendBeacon 场景；Bearer 头亦兼容） | `{logged_out}`（吊销 refresh + jti） |
| 当前账号 | `GET /api/v1/profile/current`（Bearer） | `{id, name, phone, privileged}`（从令牌取账号，无需解码 JWT） |
| 改自己密码 | `PATCH /api/v1/identity/password` `{old_password, new_password}` | `{updated}`（不吊销令牌，改密后会话继续） |
| 重置他人密码 | `PATCH /api/v1/accounts/password/{id}` `{new_password}` | `{updated}`（管理员操作；后端暂无 privileged 校验） |

- 错误统一为 RFC 9457 Problem Details（`application/problem+json`）：`{status, error_code, detail, title, trace_id}`；`account_invalid_credentials` → 400，`access_token_*` → 401
- `detail` 按 `Accept-Language` 渲染（前端固定 `zh-CN`）
- 鉴权中间件提取令牌：**`Authorization: Bearer` 头优先，回退 `?access_token=` / `?token=` query**（全端点生效，logout 的 sendBeacon 依赖此机制）

### 页面流程

```
未登录访问 /customers
    │
    ▼
_app.tsx beforeLoad
    ├─ authStore.state.user 有值 → 放行
    └─ 无值 → redirect('/login?redirect=/customers')   ← 用 location.searchStr（坑，见 §5.2）
                        │
                        ▼
login.tsx（validateSearch 解析 redirect + sanitizeRedirect 防开放重定向）
    │
    ├─ 已登录 → redirect 回 redirect 目标
    └─ 提交：validateAllFields('change') 兜底 → login(phone, password) → navigate(redirect)
```

### 令牌生命周期（`src/lib/api.ts` + `src/lib/token.ts`）

- `token.ts`：纯本地存储（`auth.tokens` / `auth.user` 两个 key）；不在前端解码 JWT，auth store 与 api 层共用，无 UI 依赖
- `api.ts`（xior）：请求自动附 `Authorization: Bearer`；**401 → 同标签页/跨标签页单飞刷新**（并发 401 只发一次 `/refresh`，其余等待同一结果），支持 Web Locks；不支持 Web Locks 时用 BroadcastChannel 广播意图并确定性选举唯一刷新者，localStorage 仅传递状态、不作为非原子互斥锁；并在 401 到达时比较已发送的 access token，避免延迟 401 重复消费轮换 refresh token；刷新后只重试原请求一次；刷新失败或重试仍 401 → 清会话 + 整页跳 `/login?redirect=...`
- 登录/刷新接口自身的 401 不触发刷新循环（白名单 `AUTH_PATHS`）
- 登录成功流程：`/login` 拿令牌 → **先存令牌** → `/profile/current` 自省（Bearer 取账号）→ 存用户 → store 更新；profile 失败 → 清令牌、登录失败（不留半状态）
- 页面加载（hydrate）：有令牌 → 主动 fetch `/profile/current` 更新用户（改名/权限即时生效；401 由 api 层自动刷新，刷新失败强制登出回登录页）
- 刷新（401 单飞）成功后：只更新令牌并重试原请求；不额外请求 profile，避免刷新风暴和重复请求。用户资料由登录/页面 hydrate 获取；跨标签页登录通过 `USER_KEY` 事件同步
- **forceLogout 短路**：已在 `/login` 页（主动登出后 in-flight 请求 401）只清状态、不整页刷新，避免丢表单
- 跨标签页：监听 `storage` 事件——他页登出（令牌清空）→ 本页登出；他页更新 user 缓存 → 同步 store；他页刷新令牌仅更新本地令牌，不触发额外 profile 请求
- 登出：**sendBeacon POST**（sendBeacon 无法设置请求头且实测无论有无 body 都发 POST，故令牌走 query `?access_token=`）→ **`localStorage.clear()` 全量清空**（含主题/字号/侧边栏偏好）→ store 置空。401 强制登出（`forceLogout`）只清 auth 两个 key，不动偏好
- 守卫用 store 直接读取（模块级单例），未用 router context 注入（纯 CSR 场景功能等价且 router 从不重建）

## 4. 核心设计

### 4.1 主题系统（index.css）

- **Nord 色板**：16 色（nord0-15）注册进 `@theme`
- **语义别名**：`canvas`（背景）/ `surface`（卡片面）/ `line`（边框）/ `ink`（文字）/ `accent`（强调色）/ `stripe`（表头+斑马纹）/ `header-line`（顶栏边框）/ `sidebar-line`（侧边栏分隔线，随主题）/ `sidebar`
- **深浅模式**：`theme.ts` 把模式（light/dark/system）统一解析为 `<html data-theme>` 具体值——system 用 `matchMedia` 解析成 dark/light 并监听系统变化实时切换；Tailwind `dark:` variant 只认 data-theme 属性，JS 解析保证它与 CSS 变量始终同步；index.css 的 `@media` 分支仅作无 JS 兑底
  - `:root[data-theme='dark'] { ... }` —— 手动深色
  - `color-scheme` 同步，原生滚动条等自动适配
- **关键原则（Filament Nord 风格）**：侧边栏随主题——亮色下比白色内容区略灰（`#f5f8fa`）+ 浅底胶囊激活态（`bg-sidebar-primary` = nord6 底 + 黑字 `#2e3440`），暗色下 `white/5` 底 + 白字；选中/ hover 文字黑/白、普通文字灰蓝；主色 = 深蓝 `#5e81ac`（nord10，白字），按钮/输入框全圆（rounded-full），表格容器 `rounded-2xl + shadow-md`（暗色渐变底，见 `.vt-card`）
- **多标签页（PageTabs）**：选中 tab 背景 = 内容区底色（亮 `#fff` / 暗 `#252a35`），标签栏无底边框，选中 tab 与内容区无缝相连——亮色靠白底浮起、暗色靠与标签栏（`#3b414e`）的深浅对比
- **hover 统一**：所有中性 hover 背景 = 菜单选中背景色（`--muted`：亮 `#e5e9f0` / 暗 `white/5`）——菜单项、表格行、侧边栏、ghost/outline 按钮、badge、tabs 全走同一色；`--color-accent` 只做静态强调（头像/徽标），不再承担 hover

### 4.2 布局（_app.tsx）

- **桌面**：侧边栏 `w-56 ↔ w-14` 折叠（收起按钮在顶栏左侧）
- **移动端**（<md）：侧边栏为抽屉（`-translate-x-full` 隐藏）+ 全屏遮罩 + 汉堡按钮
- **高度链**：`h-screen → main(flex-1) → 页面根(flex-1) → VirtualTable(flex-1 min-h-0)` 逐层传递，表格充满剩余区域
- 顶栏：左侧 = 汉堡(移动) + 收起(桌面) + 面包屑（`getBreadcrumbs` 按 navItems 层级推导，ChevronRight 分隔）；右侧 = 日期 + 字体 + 主题
- 多标签页栏在顶栏下方（`PageTabs`），内容区用 `KeepAliveOutlet` 渲染（见 §4.5）；main 滚动容器路由切换时手动回顶
- 侧边栏底部：用户区（头像首字母 + 用户名 + 退出）

### 4.3 表单（login.tsx）

- `useForm({ defaultValues, onSubmit })` + `form.Field` render-prop
- **校验**：字段级 zod schema 只配 `onChange`（单一事件源，避免 change+submit 双事件重复错误）；提交时 `form.validateAllFields('change')` 兜底（自动标 touched，未触碰直接提交也能显示错误）
- 错误显示：`isTouched` 门控 + Set 去重 + issue 对象转文本
- 注意：form 不保留 schema transform 输出 → 提交时自行 trim

### 4.4 VirtualTable（通用组件）

Props：`features / columns / data / initialState / growColumnId / onLoadMore / loadingMore / height / toolbar`

能力：
- **虚拟滚动**：`useVirtualizer` + `measureElement`（行高动态测量），`getItemKey` 用行 id
- **固定列**：`columnPinningFeature` 算偏移，renderer 自己贴 sticky CSS（`pinnedStyle`：`insetInlineStart/End` + `getStart/getAfter` + `getSize` 宽度 + `flexShrink: 0` + zIndex）
- **滚动阴影**：滚动位置检测（`scrollLeft` 边界），start/end 固定列投影，滚到边缘消失
- **无限滚动**：`onLoadMore` 可选，滚动接近底部触发（ref 保存闭包，监听只绑一次）
- **宽度策略**：表格 100% 宽，`growColumnId` 列 `flexGrow` 吸收剩余空间——宽屏充满；窄屏无剩余空间 flexGrow 自动失效，各列保持模型宽，sticky 偏移精确（end 固定列安全的必要条件）
- **渲染**：div + flex 结构（绝对定位行），`role="table/row/cell"` 保语义；背景在单元格上（横向滚动不透视）；斑马纹 + `group-hover` 行 hover
- 类型上使用 `TableColumnApi` 契约接口（v9 泛型限制，见 §5.3）

### 4.5 多标签页与 keep-alive

**多标签页**（`PageTabs.tsx` + `store/tabs.ts`）：
- 路由变化自动开标签（`addTab` 去重）；标签存储是**会话级**（刷新后仅当前页一个标签）
- 激活标签自动滚入视野：`useEffect + requestAnimationFrame` 后手动 `scrollLeft` 计算（`scrollIntoView` 在部分浏览器对横向溢出容器不生效——用户环境实测；rAF 确保布局稳定后再算）
- 首页 `/` 固定：不可关闭、不弹右键菜单
- 右键菜单与右端操作菜单共用配置（`TAB_REFRESH_ACTIONS` / `TAB_CLOSE_ACTIONS`）：刷新 / 关闭当前 / 关闭其他 / 关闭全部

**keep-alive**（`keep-alive.tsx`）：
- `KeepAliveProvider`（根路由持有缓存路由集合：`{ pathname: { version } }`）+ `KeepAliveOutlet`（替代布局 `<Outlet />`）
- 页面路由声明 `staticData: { keepAlive: true }` 参与缓存；首次访问登记，之后切走/切回复用同一棵组件树
- **冻结机制 = Suspense 挂起**（`throw` 未 resolve 的 promise）：hidden 时不渲染 children、保留已提交 DOM 与状态、不响应更新；visible 时 resolve 唤醒恢复
- **激活判断用 `useMatches` 叶子 pathname**（非 `useLocation`）：路由 transition 中 location 先变、matches 后变，若提前解冻缓存页，Outlet 会读到中间态路由导致重建
- 关闭标签调 `destroy(pathname)`（缓存条目删除 → 页面卸载释放状态）；右键「刷新」调 `refresh(pathname)`（version+1 → CacheView key 变化 → 重建页面，状态重置、重新加载）
- 缓存条目（未登记路径）由 fallback `<Outlet />` 兜底渲染当前路由

### 4.6 服务端状态（@tanstack/react-query）
- 根路由 `QueryClientProvider`（全局单例）：`staleTime 30s / retry 1 / refetchOnWindowFocus false`；keep-alive 页面共享同一缓存，切走切回不重复请求
- **列表范式（customers 页）**：`useInfiniteQuery` + 游标分页——`initialPageParam: null`、`getNextPageParam: (last) => last.nextCursor`、`queryKey: ['customers', query]`（搜索词入 key，换词即换批数据）；无限滚动用底部 sentinel + `IntersectionObserver`（`rootMargin: 200px` 预取），`hasNextPage`/`isFetchingNextPage` 门控防重复
- **变更范式**：`useMutation`（create/update/delete）→ `onSuccess` 里 `invalidateQueries({ queryKey: ['customers'] })`（infinite 保留当前页位置自动刷新）+ 关闭 Dialog + toast；删除确认按钮 `isPending` 防连点
- 详情类一次性读取（如编辑前 `apiGetCustomer`）不缓存，保持简单

### 4.7 DataTable（业务表格）
`DataTable` 是 VirtualTable 的薄封装：内部固定 features（排序 + 固定列），业务方零 tanstack 知识。

```tsx
const columns: DataColumn<Item>[] = [
  { key: 'code', header: '编码', width: 120 },
  { key: 'name', header: '名称', grow: true, render: (r) => <b>{r.name}</b> },
  { key: 'is_active', header: '状态', render: (r) => <StatusBadge active={r.is_active} /> },
  { key: 'actions', header: '操作', width: 96, align: 'center', render: (r) => <Actions /> },
];
<DataTable data={items} columns={columns} getRowId={(r) => r.id}
  onLoadMore={hasNextPage ? fetchNextPage : undefined} loadingMore={isFetchingNextPage} />
```

- 列配置：`key`（字段名或自定义标识）、`header`、`width`、`align`、`grow`（撑满一列）、`pinned`（固定列 start/end）、`render`（缺省显示原始值字符串）
- 动态 string key 在泛型组件中用函数式 accessor 表达（v9 泛型签名限制）；pinning state 字段是 `start`/`end`（非 left/right）

## 5. 踩坑记录（重要）

### 5.1 TanStack Table v9 是重写版
`useReactTable` → `useTable`；`getCoreRowModel()` → features 槽位（`coreRowModel: createCoreRowModel()` 放 `tableFeatures` 内）；列定义用 `columnHelper.columns([...])` 保类型；features 需静态定义（`tableFeatures({...})`）。`getIsPinned()` 返回 `'start' | 'end' | false`（不是 'center'）。

### 5.2 location.search 是对象
TanStack Router 新版 `ParsedLocation.search` 是解析后的对象，拼接 URL 要用 `location.searchStr`（带前导 `?`）。`pathname + search` 会抛 "Cannot convert object to primitive value"——该 bug 只在无登录态（如隐身模式）时触发，因为登录态下 beforeLoad 不会执行拼接分支。

### 5.3 v9 泛型限制
`Column<TFeatures, TData>` 在通用组件泛型场景下退化为 `Column_Core` union，feature API 不可见。VirtualTable 用最小契约接口 `TableColumnApi` + `as unknown as` 转换，调用方（具体 features 类型）仍获完整推断。

### 5.4 CSS 特异性陷阱
`hover:bg-*`（伪类，0,2,0）永远压过 `bg-*`（0,1,0）。激活态导航若同时存在 hover 背景类，hover 时会被覆盖成错误颜色 → 激活/非激活样式必须通过 `activeProps`/`inactiveProps` 彻底分离。

### 5.5 sticky 必须不透明背景
固定列/表头若无自己的不透明背景，滚动内容会从下方透出。背景不能放行级（行盒宽度=视口宽，横向滚动覆盖不到右侧），必须放单元格；且不能用 inline style（否则 `group-hover` 类压不过，固定列 hover 失效）。

### 5.6 Tailwind 4 深浅模式
Biome 的 CSS parser 不识别 `@theme` → `index.css` 在 biome `files.includes` 排除。深色覆盖用 CSS 变量（语义类自动适配），组件代码零 `dark:` 类。

### 5.7 主题/字体持久化
`store/theme.ts`、`store/fontSize.ts` 模块加载即应用（localStorage + html 属性/inline style），避免闪烁。隐身模式下 localStorage 可能受限，读操作需 try/catch（auth 已有，theme/fontSize 的 setItem 若需健壮可加）。

### 5.8 React Compiler 与 TanStack Table v9 不兼容（文件级豁免）
`useTable` 使用 render-phase store（`get`/`markCommitted` 配对的状态机），React Compiler 自动 memo 化会缓存渲染期间的读取，导致 `table.state`/引用不更新——表现：全选后行选中正常但表头 checkbox 卡在初始值、`getIsAllRowsSelected()` 恒为 false。诊断要点：`table.state` 在 effect 中读取会返回 undefined（render-phase 值仅渲染期有效）；table-core 层逻辑正常（node 复刻验证 `toggleAllRowsSelected` 后 all/some 均 true）。

**当前方案**：`reactCompiler: true` 全局启用，受影响文件（`VirtualTable.tsx`、`DataTable.tsx`）顶部加 `"use no memo"` 指令豁免（React Compiler 官方文件级禁用机制），其余文件继续享受编译优化。

### 5.9 shadcn/ui 接入
- 变量全部映射到 Nord 语义（`--primary: #5e81ac`、`--background/foreground/border/ring` 对应 canvas/ink/line/accent-soft），深浅主题三套变量（:root / data-theme='dark' / media query）自动切换
- **accent 冲突**：shadcn 的 accent（悬停背景）与我们的 `--color-accent`（主色）同名，保留主色语义，shadcn 组件的 `bg-accent` 落为主色
- `@custom-variant dark (&:is([data-theme='dark'] *))` 匹配项目切换机制（跟随系统模式下 dark: 类不触发，变量机制保证颜色正确）
- **CLI 不可用**：MCP SDK 的 `zod/v3` 导入与 pnpm 解析冲突（overrides 无效），组件改为从 registry（`ui.shadcn.com/r/styles/base-nova/*.json`）拉源码手动创建，IconPlaceholder 替换为 lucide 图标
- 组件基于 @base-ui/react（非 Radix）；Base UI Checkbox 原生支持 `indeterminate` prop
- `@import 'shadcn/tailwind.css'` 提供组件基础样式；biome 忽略 index.css（Tailwind 语法）

### 5.10 keep-alive 与多标签页
- **Suspense 挂起 vs Activity**：`<Activity mode="hidden">` 官方语义是 *renders children but keeps them hidden*——hidden 缓存页仍渲染当前路由，路由一切走内容被覆盖、切回必重建（曾实测踩坑）。Suspense 挂起（throw 未 resolve promise）才真正"不渲染"，保留已提交 DOM/状态
- **激活判断必须用 matches 叶子路径**：navigate 是 transition（location 立即变、matches 渐进更新）。用 `useLocation` 判断 active 会在中间态提前解冻缓存页，Outlet 读到旧 matches 渲染错误内容 → 页面重建
- **scrollIntoView 对横向容器不可靠**：标签自动滚入视野用手动 `scrollLeft += rect 差值`（rAF 后执行），不要依赖 `scrollIntoView({ inline: 'nearest' })`
- **测试输入必须用真实键盘**：原生 `setter + dispatchEvent('input')` 不更新 React state（React 19 value tracker 会在下次渲染重置）；headless 验证用 CDP `Input.insertText`（先 focus 可见 input）
- **dev server 产物缓存**：外部改文件偶发不触发重编译（或浏览器缓存旧 chunk），验证"没生效"前先重启 dev + 清 `node_modules/.cache` + 换全新浏览器 profile（曾因此误判多次）
- **第三方 keep-alive 库均不可用**：react-activation（React 19 不兼容）、tanstack-router-keepalive（依赖旧 `getRouterContext`）、tanstack-router-cache（依赖 router.stores 内部结构，1.171 重构后崩溃）→ 最终用官方公开 API 自研（见 §4.5）

### 5.11 axum Query flatten 的 wire 格式是顶层平铺键
后端列表端点的分页参数 `#[serde(flatten)] pub paging: CursorPagingQuery`（openapi 呈现为 `paging` inline object）在 **serde_urlencoded 0.7 下展开为 query 顶层键**：`?limit=20&next_cursor=<id>`。实测 `paging[limit]=1` 嵌套形式被**静默忽略**（无报错、走默认 limit=10、cursor 过滤失效），顶层 `limit`/`next_cursor` 才生效。对接列表端点时以 curl 实测为准，别信 openapi 的 object 呈现。

### 5.12 Sheet/Dialog 覆盖类必须带原变体前缀
组件默认类常带变体前缀（如 SheetContent 的 `data-[side=right]:sm:max-w-sm`），twMerge 对**不同变体组合不判冲突**（两类都保留），而带前缀的默认类**特异性更高**恒赢——覆盖宽度等样式必须写完整前缀（如 `data-[side=right]:sm:max-w-xl`），否则看起来"没生效"。

## 6. 开发命令

```bash
pnpm run dev        # 开发服务器（http://localhost:3000）
pnpm run typecheck  # tsc --noEmit 类型检查（日常改完必跑）
pnpm run check      # biome check --write（日常改完必跑）
pnpm run build      # 生产构建（提交前，不要每次改完都跑）
pnpm run preview    # 预览生产构建
pnpm run format     # biome format --write
```

## 7. 扩展指南

- **加页面**：`src/routes/_app/xxx.tsx` 创建文件路由（routeTree 自动生成），页面根用 `flex min-h-0 flex-1 flex-col` 以充满剩余高度；需要标签页保留状态时加 `staticData: { keepAlive: true }`
- **加侧边栏菜单**：`SidebarNav.tsx` 的 `navItems` 数组加 `{ to, label, icon }`（或 `{ label, icon, children }` 分组）
- **新表格**：定义 features（只注册用到的）+ columns（`columnHelper.columns`），套 `<VirtualTable>`；需要弹性列时传 `growColumnId`
- **新下拉/右键菜单**：复用 `dropdown-menu.tsx` / `context-menu.tsx`（base-ui）
- **换品牌色**：只改 `index.css` 的 `--color-accent*` 等语义变量，深浅两处

## 8. 对接后端契约（省 token 工作流）

后端（Rust + utoipa）的 OpenAPI 契约可从运行中的服务直接提取，**不要为拿契约逐个读后端源码**，也**不要把快照整文件读进上下文**：

1. **按名查类型**：Grep `src/lib/api-schema.d.ts` 的 schema 名（该文件约 130KB，禁止整份 Read）。`api.ts` 已引用 `components['schemas'][...]`。
2. **按 path 查 spec**：对 `openapi.json` 用 `jq` 取单条 path / schema（该文件约 234KB，禁止整份 Read）。刷新快照：`pnpm gen:api` → 直拉后端原生 `GET /openapi.json`。重新生成类型（当前 TS7 与生成器不兼容，需临时 TS5 环境）：
   ```bash
   cd /tmp && npm i typescript@5 openapi-typescript \
   && npx openapi-typescript <frontend>/openapi.json -o <frontend>/src/lib/api-schema.d.ts
   ```
3. **`e2e/*.hurl`**：真实请求/响应样本（含 token 捕获、错误断言），比源码精炼。
4. **curl 实测**运行中的后端（`http://127.0.0.1:8081`），确认实际 JSON 形状（扁平响应、Problem Details）。

最后仍需要时才读后端源码，且只看契约面（`endpoint/*.rs` 的 DTO + `*_contract` 的 value object / error），不看 SQL 与业务实现。

注意：spec 中的响应 schema 是 `JsonResponse_*` 包装（utoipa 的 untagged 单成员枚举），实际 HTTP 响应是**扁平 JSON**——以 hurl/curl 实测为准。

另见 §5.11：列表 query 参数（flatten 分页对象）的 wire 格式同样以 curl 实测为准（顶层平铺键，非 openapi 呈现的嵌套对象）。
