# Skills 参考手册

**没有 always-on skill。** `.agents/skills/` 为空；description 不进系统提示。

全部 skill 在 `.agents/optional-skills/`，**用户点名再读**。来源：mattpocock 工程技能集。路径：`optional-skills/<name>/`。

改已有 endpoint 的 `execute` / 在已有 `mod tests` 里加断言：**不要**先读 skill。

---

## 点名再开

### 本仓编码（读文档，不是 skill）

| 你的状态 | 打开 |
|---------|------|
| 改已有端点 / 加断言 | 该文件（`backend.mdc` 已注入） |
| 新建域或新 endpoint 文件 | `docs/ai/backend.md` 对应小节 |
| 第一次补测试 / 写 Hurl | identity 同文件 `mod tests` + `docs/E2E_HURL.md` |
| 错误 key / HTTP / 陷阱 | `docs/ai/conventions.md` |

### 审查 / 调试 / TDD

- `tdd` — 用户明确要求 TDD / red-green-refactor
- `code-review` — 审查分支 / PR / 自某点以来的改动
- `diagnosing-bugs` — 用户说 diagnose / debug this，或难复现故障
- `resolving-merge-conflicts` — 正在解决 merge/rebase 冲突

### 主流程（idea → ship）

1. `ask-matt` — 不知道该用哪个 skill
2. `grill-with-docs` — 带文档的需求澄清；产出 `CONTEXT.md` + ADR
3. `grill-me` — 同上，不写文档
4. `grilling` — 底层质问原语（一次一个问题）
5. `prototype` — 一次性代码回答设计问题
6. `handoff` — 跨会话交接
7. `to-spec` — 对话 → 规格
8. `to-tickets` — spec → 垂直切片 tickets
9. `implement` — 按 spec/ticket 实现

### 设计与架构

- `codebase-design` — 深模块词汇（module / interface / depth / seam / adapter / leverage / locality）
- `domain-modeling` — 维护 `CONTEXT.md` 与 ADR
- `improve-codebase-architecture` — 扫描 shallow 模块，出 HTML 报告
- `wayfinder` — 超大工作量拆成决策 tickets

### 其他

- `research` — 后台调研，产出 Markdown
- `triage` — 外部 issue/PR
- `teach` — 教学 workspace
- `setup-matt-pocock-skills` — 一次性初始化
- `writing-great-skills` — 编写 skill 时参考

---

## 快速索引

| 你的状态 | 用这个 |
|---------|--------|
| 改已有端点 / 加断言 | 不读 skill，直接改 |
| 新建域或新 endpoint 文件 | `docs/ai/backend.md` |
| 第一次给某端点补测试 / 写 Hurl | identity `mod tests` + `docs/E2E_HURL.md` |
| 用户要求 TDD | `tdd` |
| 有个模糊想法 | `grill-with-docs` |
| 讨论清楚了要做 | `to-spec` → `to-tickets` → `implement` |
| 写完要审查 | `code-review` |
| 遇到难复现的 bug | `diagnosing-bugs` |
| 合并冲突 | `resolving-merge-conflicts` |
| 架构讨论 / 加深模块 | `codebase-design` |
| 需要调研技术问题 | `research` |
| 不知道用哪个 | `ask-matt` |
