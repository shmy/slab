# Slab 文档

按场景打开，不要通读。

## 入口

| 谁 | 读什么 |
|----|--------|
| AI（每轮） | 根目录 [AGENTS.md](../AGENTS.md)（导航 / 打开集）；不要开场读本目录全文 |
| AI（打开后端文件） | `.cursor/rules/backend.mdc` 自动注入编码 / 测试摘要 |
| AI（新建域 / 端点 / Job） | [ai/backend.md](ai/backend.md) 对应小节 |
| AI（错误 key / HTTP / 陷阱） | [ai/conventions.md](ai/conventions.md) |
| AI（术语） | 根目录 [CONTEXT.md](../CONTEXT.md) |
| 人（上手） | 根目录 [README.md](../README.md) |

## 基础设施（动到才读）

| 文档 | 何时 |
|------|------|
| [EVENT_BUS.md](EVENT_BUS.md) | 加事件 / 订阅者 / 切换总线后端 |
| [JOB_QUEUE.md](JOB_QUEUE.md) | 加 Job / 周期任务 |
| [FLOW.md](FLOW.md) | 加长流程 / 跨单据编排 |
| [KV.md](KV.md) | 缓存 / token 吊销 / 切换 KV 后端 |
| [E2E_HURL.md](E2E_HURL.md) | 写或改 Hurl |
| [frontend/docs/architecture.md](../frontend/docs/architecture.md) | 改前端表格 / 标签 / 主题，且 `frontend/AGENTS.md` 不够 |
