---
name: gitnexus-impact-analysis
description: "Use when changing a public contract/Port/event/shared API, renaming/splitting symbols, or the user asks what will break. Do not use for private execute/handler edits that keep the same signature."
---

# Impact Analysis with GitNexus

本仓库以根目录 `AGENTS.md` 的 Impact 分级为准，不要对每个符号都跑 `impact`。

## When to Use

- 改 `*_contract` / Port / Event / 跨域 `pub` API / FILTER_SCHEMA / OpenAPI DTO
- 重命名、移动、拆分已有符号
- 用户问 "改 X 会炸什么"
- 提交前用 `detect_changes()`（一次），不是每个符号先 `impact`

不要用于：单文件私有 `execute` / handler / 测试（签名不变）、注释、locale、样式、文档

## Workflow

```
1. impact({target: "X", direction: "upstream"})  → What depends on this
2. 看返回里的 d=1 callers 和 affected processes；不要把 `gitnexus://repo/slab/processes` 全量读进上下文
3. 提交前 detect_changes() 一次
4. HIGH / CRITICAL 先警告用户
```

> If "Index is stale" → run `node .gitnexus/run.cjs analyze` in terminal.

## Checklist

```
- [ ] impact({target, direction: "upstream"}) to find dependents
- [ ] Review d=1 items first (these WILL BREAK)
- [ ] Check high-confidence (>0.8) dependencies
- [ ] Use affected processes from the impact result (do not dump all processes)
- [ ] detect_changes() once before commit
- [ ] Warn the user on HIGH / CRITICAL
```

## Understanding Output

| Depth | Risk Level       | Meaning                  |
| ----- | ---------------- | ------------------------ |
| d=1   | **WILL BREAK**   | Direct callers/importers |
| d=2   | LIKELY AFFECTED  | Indirect dependencies    |
| d=3   | MAY NEED TESTING | Transitive effects       |

## Risk Assessment

| Affected                       | Risk     |
| ------------------------------ | -------- |
| <5 symbols, few processes      | LOW      |
| 5-15 symbols, 2-5 processes    | MEDIUM   |
| >15 symbols or many processes  | HIGH     |
| Critical path (auth, payments) | CRITICAL |

## Tools

**impact** — the primary tool for symbol blast radius:

```
impact({
  target: "validateUser",
  direction: "upstream",
  minConfidence: 0.8,
  maxDepth: 3
})

→ d=1 (WILL BREAK):
  - loginHandler (src/auth/login.ts:42) [CALLS, 100%]
  - apiMiddleware (src/api/middleware.ts:15) [CALLS, 100%]

→ d=2 (LIKELY AFFECTED):
  - authRouter (src/routes/auth.ts:22) [CALLS, 95%]
```

**detect_changes** — git-diff based impact analysis:

```
detect_changes({scope: "staged"})

→ Changed: 5 symbols in 3 files
→ Affected: LoginFlow, TokenRefresh, APIMiddlewarePipeline
→ Risk: MEDIUM
```

## Example: "What breaks if I change validateUser?"

```
1. impact({target: "validateUser", direction: "upstream"})
   → d=1: loginHandler, apiMiddleware (WILL BREAK)
   → d=2: authRouter, sessionManager (LIKELY AFFECTED)

2. 从 impact 返回里看 affected processes（LoginFlow, TokenRefresh）
   → 不要全量读取 processes 资源

3. Risk: 2 direct callers, 2 processes = MEDIUM
```
