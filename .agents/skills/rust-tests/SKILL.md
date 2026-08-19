---
name: rust-tests
description: "Slab 集成测试 / Hurl 规范。仅当新增测试模块、某端点第一次补测试、或写 Hurl E2E 时读取；在已有 mod tests 里加断言不要加载。"
---

# rust-tests

**Trigger**: 新增测试模块、某端点第一次补集成测试、写 Hurl E2E。已有 `mod tests` 里加一个断言：不要读本文件。

同文件 `mod tests` 的做法见 `.cursor/rules/backend.mdc` 测试摘要；陷阱见 `docs/ai/conventions.md`「测试」。crate `[dev-dependencies]` 蓝本：`features/identity/Cargo.toml`。

## Hurl（仅 E2E）

- 入口：`just e2e`；变量 `e2e/env`；单文件 `hurl --test --variables-file e2e/env e2e/identity.hurl`
- jsonpath 用 `$.id`；文件间隔 2s 防 429；边界用例 `Accept-Language: en-US`
- 改种子管理员 / 路由 / 错误码 / Fluent 时同步 `e2e/*.hurl`
- 编写规范：[docs/E2E_HURL.md](../../../docs/E2E_HURL.md)
