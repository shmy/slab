# Skills 参考手册

日常自动加载的 skill 在 `.agents/skills/`（约 11 个）。
grill / wayfinder / to-spec 等流程在 `.agents/optional-skills/`，**不进系统提示**，用户点名再读。

来源：mattpocock 工程技能集。下表路径：optional 的在 `optional-skills/<name>/`。


---

## 🧭 主流程（idea → ship）

### 1. `ask-matt` — 路由中心
不知道自己该用哪个 skill 时问它。
- **场景**：打开一个新项目、不确定从哪步开始、想知道某个流程的完整路径

### 2. `grill-with-docs` — 带文档的需求澄清
通过一问一答把模糊想法打磨清晰，同时产出 `CONTEXT.md` 词汇表和 ADR 决策记录。
- **场景**：有新想法但还不清晰、需要沉淀领域术语和架构决策
- **产出**：`CONTEXT.md`（领域词汇表）+ ADR（架构决策记录）

### 3. `grill-me` — 无文档的需求澄清
同上，但不保存任何文档。
- **场景**：没有代码库的纯想法打磨

### 4. `grilling` — 底层质问原语
对计划/决策/想法进行压力测试，一次只问一个问题。
- **场景**：被其他 skill 自动调用；或手动对某个决策做系统性质问

### 5. `prototype` — 快速原型验证
写一次性代码来回答一个设计问题。
- **两个分支**：逻辑/状态模型 → 交互式终端程序；UI → 多个变体切换展示
- **场景**：方案在纸上说不清、需要跑起来看看
- **原则**：一次性代码、不保存、不测试、不抛光

### 6. `handoff` — 跨会话交接
把当前对话压缩成交接文档，让下一个 agent 接力。
- **场景**：上下文窗口快满了、需要分叉到另一条线、跨 agent 接力

### 7. `to-spec` — 对话→规格文档
把对话内容合成为 PRD/spec，发布到 issue tracker。
- **场景**：需求讨论清楚了、需要一份正式规格说明
- **特点**：不做追问，只综合已有内容

### 8. `to-tickets` — 计划→分解 tickets
把 spec 拆解为垂直切片的 tracer-bullet tickets，标注阻塞关系。
- **场景**：多 session 并行开发、大功能拆小

### 9. `implement` — 按 spec/ticket 实现
读取 spec/tickets，用 TDD 实现，完成后跑 code-review。
- **场景**：有清晰的 spec/tickets 需要实现

---

## ✅ 代码质量

### 10. `code-review` — 双维度代码审查
并行跑两个 sub-agent：
- **Standards**：编码规范 + 代码坏味道（Fowler 10 种）
- **Spec**：是否忠实实现了原始需求
- **场景**：提 PR/合并前审查
- **特点**：两个维度独立报告，不互相掩盖

### 11. `tdd` — 测试驱动开发
红-绿-重构循环的完整纪律。
- **场景**：任何新功能开发或 bug 修复
- **核心**：在确认的 seam 处测试、每次一个垂直切片、不 mock 实现细节

### 12. `codebase-design` — 深度模块设计词汇表
共享语言（module/interface/depth/seam/adapter/leverage/locality）用于设计和评估模块接口。
- **场景**：设计新模块接口、判断模块是否 shallow、寻找 deepening 机会

### 13. `improve-codebase-architecture` — 架构扫描与优化
扫描代码库发现 shallow 模块，生成 HTML 可视化报告。
- **场景**：觉得代码库越来越难改、想系统性优化
- **产出**：带 Mermaid 图和 before/after 对比的 HTML 报告

---

## 🐛 排错与维护

### 14. `diagnosing-bugs` — 硬 Bug 诊断
6 阶段流程：构建反馈闭环 → 复现+最小化 → 假设生成 → 探测 → 修复+回归测试 → 清理
- **场景**：难以定位的偶发 bug、性能回归
- **铁律**：没有能红能绿的反馈闭环之前，不准猜原因

### 15. `resolving-merge-conflicts` — 解决合并冲突
- **场景**：git merge/rebase 产生冲突时

### 16. `triage` — Issue/PR 处理
分类（bug/enhancement）→ 验证 → 质问 → 产出 agent-ready brief
- **场景**：外部提交的 issue 或 PR 需要处理
- **状态机**：`needs-triage` → `needs-info` → `ready-for-agent` → `ready-for-human` → `wontfix`

---

## 📖 研究与学习

### 17. `research` — 委托调研
启动后台 sub-agent 查阅一手资料，产出 Markdown 调研报告。
- **场景**：需要调研技术问题、查阅 API 文档、收集领域知识

### 18. `teach` — 教学 Workspace
在当前目录创建完整学习空间（MISSION、学习记录、参考文档、交互课程）。
- **场景**：想系统性学习某个技能/概念

---

## 🗺️ 大型规划

### 19. `wayfinder` — 超大工作量探路
把大块工作映射为一组决策 tickets，每张解决一个问题，直到路线明朗。
- **场景**：全新项目、巨大功能、多 session 才能完成的工作
- **限制**：每次 session 只解决一张 ticket（research 除外）

### 20. `domain-modeling` — 领域建模
构建/维护 `CONTEXT.md` 领域词汇表，在关键决策点创建 ADR。
- **场景**：建立统一语言、模糊术语澄清、记录架构决策

---

## ⚙️ 基础设施

### 21. `setup-matt-pocock-skills` — 一次性初始化
配置 issue tracker、triage label 映射、领域文档布局。
- **场景**：首次使用这个 skill 集之前必须先跑一次

### 22. `writing-great-skills` — 编写 Skill 参考
skill 词汇表（leading word、progressive disclosure、premature completion 等）和设计原则。
- **场景**：自己写 skill 时参考

---

## 💡 快速索引

| 你的状态 | 用这个 |
|---------|--------|
| 有个模糊想法 | `grill-with-docs` |
| 讨论清楚了要做 | `to-spec` → `to-tickets` → `implement` |
| 要写代码实现 | `implement`（内部自动用 `tdd`） |
| 写完要审查 | `code-review` |
| 代码库越来越难改 | `improve-codebase-architecture` |
| 遇到难复现的 bug | `diagnosing-bugs` |
| 合并冲突 | `resolving-merge-conflicts` |
| 外部 issue 需要处理 | `triage` |
| 需要调研技术问题 | `research` |
| 想系统学个新东西 | `teach` |
| 超大项目看不清方向 | `wayfinder` |
| 不知道用哪个 | `ask-matt` |

---

## 🔗 主流程关系图

```
grill-with-docs ──→ to-spec ──→ to-tickets ──→ implement ──→ code-review
                     ↑              ↑
                     │              │
                  prototype      diagnosing-bugs
                  handoff        triage
                                 wayfinder
```

- `grill-with-docs` / `grill-me` 内部调用 `grilling`
- `domain-modeling` 和 `codebase-design` 是词汇层，被其他 skill 自动引用
- 首次使用前先跑 `setup-matt-pocock-skills`
