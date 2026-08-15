# 筛选协议单源化：FilterSchema 经 meta 端点 + codegen 进入前端

Status: accepted

> **2026-08-15**：筛选协议（操作符矩阵 + 字段白名单）由前端手抄改为契约承载——后端 `GET /api/v1/meta/filter-schemas` 导出 `filter_kit` 矩阵与各域 `FILTER_SCHEMA`，前端 `pnpm gen:api` 生成 `src/lib/filter-schema.ts`，`filters.ts` 退化为纯推导层，FilterBar 直接消费 (schema, labels)。

## 背景

PostgREST 风格筛选协议在前后端各维护一份：后端 `libs/filter_kit`（操作符矩阵 / 前缀序 / 通配约定）+ 各域端点 `FILTER_SCHEMA` 白名单；前端 `lib/filters.ts`（TYPE_OPERATORS / OP_TO_PG / PG_OPS）+ 页面 `FILTER_FIELDS` 字段注册表。`filters.ts` 文件头注释明写「改矩阵两处同步」。OpenAPI 契约无法承载：`filters` 是 serde flatten 的 `HashMap<String, String>`，utoipa 渲染时完全不可见（spec 只有 `paging`/`q`），前端无从推导只能手抄。旧前端注册表还漏了后端白名单里的 `contact_person`——双源漂移已实际发生。

## 决策要点

1. **事实源 = 后端**：`filter_kit` 矩阵数据化（`OPERATOR_MATRIX` 类型名 → 操作符集、`op_prefixes()` 前缀序、`Op::as_str()`），to_sql 行为不变；各域 `FILTER_SCHEMA` 提升为 `pub` 导出。
2. **meta 端点归 bin/server 组合根**：`GET /api/v1/meta/filter-schemas` 输出 `{operatorMatrix, opPrefixes, entities: {customer: {fields: [{name, type}]}}}`，由 `FILTER_SCHEMAS` 常量表收集（新增可筛实体 = 域内声明 + 此处一行 + 前端补 label，与 modules.rs 同层，不新增跨域依赖）。
3. **构建期 codegen，不做运行时拉取**：扩展 `pnpm gen:api`（fetch-openapi.mjs 顺带拉 meta）→ 生成 `src/lib/filter-schema.ts`（`FilterOperator` / `FilterFieldType` / `FILTER_OPERATOR_MATRIX` / `FILTER_OP_PREFIXES` / `filterSchemas` / 每实体字段联合类型如 `CustomerFilterField`）。与 api-schema.d.ts 的既有工作流同构，FilterBar 保持同步渲染、类型安全。
4. **filters.ts 纯推导层**：操作符集 / 前缀序 / 字段集合全部来自生成物；只留 UI 文案（操作符 label、date/int 语义词）与序列化逻辑（contains 自动包 `*值*`、手工通配保留、前缀最长匹配）。
5. **FilterBar 消费 (schema, labels)**：字段配置合并（契约字段 × 前端文案）内聚在 FilterBar；页面只声明 label 映射，用 `satisfies Record<CustomerFilterField, FilterLabel>` 编译期强制——后端加字段未补文案 → tsc 报错（单源 + 编译期闭环）。
6. **测试**：后端 meta 端点形状单测 + filter_kit 矩阵一致性测试（矩阵与 to_sql 支持集对拍、前缀序）；前端 filters.ts 推导层单测（序列化往返 / 通配 / 前缀最长匹配）。不做双端对拍——生成物即一致性。

## Considered Options

- **运行时能力端点（被否）**：FilterBar 每次渲染拉契约 → 异步加载态 + 字段列表字符串化失去类型安全；「后端改了立即生效」的收益在 api-schema.d.ts 工作流里已被接受为常态。
- **OpenAPI x-扩展（被否）**：`filters` 是 flatten HashMap，utoipa 本就表达不了，逆流而上实现成本高，收益与 codegen 相同。
- **双源 + 对拍测试（被否）**：防漂移不防维护（对拍测试是两份手抄表的旧世界产物）；旧注册表已实际漏字段。
- **label 进后端 locale（被否）**：字段标签是 UI 文案，前端无 i18n 框架，后端 locale 是错误/领域文案系统，不宜背 UI 标签。

## Consequences

- 加可筛字段 = 后端 `FILTER_SCHEMA` 一处 + 前端 label 一行（缺则 tsc 报错）；加实体 = 域声明 + meta 注册一行 + label 映射。
- 旧前端注册表补齐了 `contact_person`（原双源漂移遗漏项）。
- `filters.ts` 零手抄表；FilterBar 接口从 `fields: FilterFieldConfig[]` 改为 `(schema, labels)`。
- 依赖管道：改筛选协议 → 后端重编译 + 重启 → `pnpm gen:api` → 前端类型自动跟随；生成文件入库（与 api-schema.d.ts 同约定）。
- 未来若出现「后端加操作符」，前端文案表（OP_LABELS）回退显示协议名，不阻塞。
