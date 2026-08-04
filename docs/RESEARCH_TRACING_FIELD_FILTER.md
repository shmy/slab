# 研究：tracing 生态是否存在"全局按字段名过滤 span 字段"的机制

> 研究日期：2026-08
> 研究对象：tracing 0.1.44 / tracing-subscriber 0.3.23（workspace 依赖，`Cargo.toml`）、rootcause 0.13 / rootcause-tracing 0.13（本地 cargo registry 源码 `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`）、rootcause GitHub main 分支。
> 触发背景：`#[tracing::instrument]` 会把 `pg_pool` 等大对象参数记进 span，`RootcauseLayer` 会把所有 span 字段收进错误报告。已通过逐点 `skip(pg_pool)` 解决，本文件回答"有没有全局配置的官方途径"。

## 结论摘要（TL;DR）

**没有。tracing 生态不存在"在 subscriber 层全局按字段名剔除 span 字段"的官方机制。**

- **能全局做的**：按 target / level / span name / span 字段存在性 / 字段值，**决定整个 span 或 event 是否启用**（EnvFilter、Filter trait、Filtered layer）。这是"全有或全无"的开关，不是字段剔除。
- **不能做的**：在 subscriber 层删掉某个 span 里的某个字段。所有过滤接口（`Filter`、`Layer` 回调）拿到的 `&Metadata` / `&Attributes` / `&Record` 都是**只读引用**，只能返回"启用/禁用"，没有任何改写字段的入口。
- **唯一官方字段级手段**：宏层面的 `#[instrument(skip(...))]` / `skip_all`（以及手写 `span!` 时干脆不写该字段），在字段进入 subscriber 之前就不记录。这也正是项目当前逐点 `skip(pg_pool)` 的做法，且是唯一能同时阻止字段进入 fmt / OTLP / rootcause 等**所有**消费者的方式。
- **rootcause 侧**：`rootcause-tracing` 最新发布版 0.13.0（2026-06-14）与 GitHub main 分支源码一致，`RootcauseLayer` 全量收集 span 字段，**无字段黑名单/排除配置**；`SpanCollector` 只有 `capture_span_for_reports_with_children` 一个开关（对应 `ROOTCAUSE_TRACING=leafs` 环境变量），与控制字段无关。

**判定：只能逐点 skip（官方）。** 如果只想让 rootcause 报告不含某字段，可以 fork 一个带黑名单的自定义 Layer（非官方，约 20 行，见下文"推荐做法"），但它只影响 rootcause 这一路消费者。

---

## 1. tracing-subscriber 的过滤机制能做什么、不能做什么

文档：<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/>、<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html>

`filter` 模块提供四类机制：`EnvFilter`、`Targets`、`LevelFilter`、`Filter` trait + `Filtered` layer（per-layer 过滤）。**所有机制的语义都是"是否启用某个 span/event"，没有任何机制能修改 span 的字段集合。**

### 1.1 Filter trait：输入全部只读

`Filter` trait 定义于 tracing-subscriber 0.3.23 `src/layer/mod.rs`（1264 行起，本地源码验证），全部方法：

| 方法 | 输入 | 返回 | 能力 |
|---|---|---|---|
| `enabled` | `&Metadata` | `bool` | 只看 target/level/span name/callsite，**Metadata 不含字段值** |
| `callsite_enabled` | `&'static Metadata` | `Interest` | 同上，可缓存 |
| `event_enabled` | `&Event` | `bool` | 事件字段（能看值），但只决定事件启用与否 |
| `on_new_span` | `&span::Attributes`、`&Id`、`Context` | `()` | 通知钩子，attrs 只读 |
| `on_record` | `&Id`、`&span::Record`、`Context` | `()` | 通知钩子，record 只读 |
| `on_enter` / `on_exit` / `on_close` | `&Id` | `()` | 生命周期通知 |
| `max_level_hint` | — | `Option<LevelFilter>` | 性能提示 |

`on_new_span` / `on_record` 虽然能看到字段，但参数是 `&span::Attributes<'_>` / `&span::Record<'_>`——**只能读，不能删、不能改**。Filter 唯一的输出是 `bool` / `Interest`，即"这个 span 要不要给我的 layer 看"。

### 1.2 EnvFilter：字段匹配只用于"决定启用"，不剔除字段

EnvFilter 指令语法（文档 "Directives" 一节）：

```
target[span{field=value}]=level
```

- 可以按 **字段名存在性** 匹配：`[{field}]`——"匹配任何带有名为 field 字段的 span/event"；
- 可以按 **字段值** 匹配：`[span{field="value"}]`——按 `Debug` 输出或正则匹配（可用 `Builder::with_regex` 关闭正则）。

但文档明确，这些匹配的用途是**启用/禁用**：

> `[span_b{name="bob"}]` will enable all spans or event that: … which has a field named `name` with value `bob`, at any level.

实现上（本地源码 `src/filter/env/mod.rs` 568–626 行），`EnvFilter::on_new_span` 把 attrs 转成内部 `SpanMatch` 存进 `by_id` 表，`on_record` 更新它，后续 `enabled` 用它判断当前 span 上下文是否命中指令——**字段值只被读取用于匹配，从不被改写或删除**。

所以：`RUST_LOG='my_crate[span_a{pg_pool}]=off'` 可以"含有 pg_pool 字段的 span 整个不输出"，但**做不到"span 照常输出、只是去掉 pg_pool 字段"**——若该 span 里还有其他有用的字段，这一招就不可用（且对 rootcause 这类 layer，过滤的是整个 layer 的输入，同样是一刀切）。

### 1.3 小结

| 想做的事 | 官方支持？ |
|---|---|
| 按 target/level/span name 过滤 | ✅ EnvFilter / Targets / LevelFilter |
| 按 span 字段存在性、字段值过滤（作为启用条件） | ✅ EnvFilter `[span{field[=value]}]` |
| 按字段名**剔除** span 里的字段（span 其余照常） | ❌ 无任何官方机制 |

---

## 2. 生态中现成的"字段剔除/脱敏" crate 调查

通过 crates.io API（`https://crates.io/api/v1/crates?q=...`）检索了 `tracing filter fields`（1232 个结果，前列无相关 crate）、`tracing filter span`（593 个结果，前列无相关）、`tracing redact`、`tracing exclude field` 等关键词。**没有找到任何被广泛使用的、能在 subscriber 层按字段名剔除 span 字段的 layer。**

检索中出现的相关 crate 全部是"脱敏（redact）"方向，且都很新、下载量极低，均非官方、非事实标准：

- **trace-redact**（0.1.0，2026-05，总下载 19）：对 `serde_json::Value`（agent trace / OTel span attributes）做**导出前的 JSON 后处理**脱敏，不是 tracing-subscriber Layer，也不作用于 tracing 内部的数据流。见 <https://docs.rs/trace-redact/latest/trace_redact/>。
- **cloakrs-tracing**（0.3.0，2026-05，总下载 17）：与 cloakrs 脱敏库的 tracing 集成。见 <https://crates.io/crates/cloakrs-tracing>。
- **redactable** / **redactkit** / **doxa-protected**：类型层面的脱敏（给类型实现 trait，使 `Debug`/序列化输出被替换），需要改类型定义，不是 layer 机制。
- **chio-log-redact**：某个特定项目（Chio operator）内部的脱敏 layer，非通用。

这些 crate 的语义也都是"把值替换成 `[REDACTED]`"而非"删除字段"，且没有任何一个成为生态共识。**结论：生态层面同样没有官方或主流的全局字段剔除方案。**

---

## 3. rootcause-tracing 最新版是否新增了字段黑名单/排除配置

### 3.1 本地 0.13.0（当前 workspace 使用的版本）

本地源码 `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rootcause-tracing-0.13.0/src/lib.rs`：

- `RootcauseLayer::on_new_span`（235–261 行）：用 `attrs.record(&mut visitor)` **全量收集**所有字段到 `CapturedFields`，无任何字段名过滤、无黑名单参数（`RootcauseLayer` 是 `#[derive(Copy, Clone, Debug, Default)]` 的单元结构体）。
- `SpanCollector`（297–305 行）：仅一个公开字段 `capture_span_for_reports_with_children: bool`；`new()` 读取环境变量 `ROOTCAUSE_TRACING`（仅支持 `leafs` 选项，330 行附近）。
- **没有任何字段排除/黑名单配置。**

### 3.2 GitHub main 分支

抓取 <https://raw.githubusercontent.com/rootcause-rs/rootcause/main/rootcause-tracing/src/lib.rs>（2026-08 研究时），与本地 0.13.0 **逐字节一致**：`RootcauseLayer` 仍是 `attrs.record` 全收，`SpanCollector` 仍只有 `capture_span_for_reports_with_children` + `ROOTCAUSE_TRACING=leafs`。README 同样只提到 `leafs` 一个选项。

仓库内的 `rootcause-tracing/CHANGELOG.md` 与根目录 `CHANGELOG.md` 均不存在（HTTP 404 / 超时），但 main 源码即最新发布版源码，足以定论。

### 3.3 版本确认

crates.io API（<https://crates.io/api/v1/crates/rootcause-tracing>）：`rootcause-tracing` 共 4 个版本（0.11.1 / 0.12.0 / 0.12.1 / **0.13.0**），`max_stable_version = "0.13.0"`，发布于 2026-06-14，为当前最新。**即：不存在"新版本加了字段过滤"的可能。**

### 3.4 小结

| rootcause-tracing 能力 | 0.13.0 / main |
|---|---|
| 收集哪些字段 | 全部（`attrs.record` 全量） |
| 字段黑名单 / 排除字段名 | ❌ 无 |
| 报告级控制 | ✅ 仅 `ROOTCAUSE_TRACING=leafs`（只影响"哪些错误报告附带 span"，与字段无关） |

---

## 4. tracing 架构层面：为什么"全局禁止某字段进入 span"在 subscriber 层做不到

关键判断验证：**"span 的字段在 `span!` 宏创建时一次性构造；subscriber 的 `new_span` 回调拿到只读 `Attributes`；各 Layer 各自消费同一份只读数据，一个 Layer 无法影响另一个 Layer 看到的字段"——成立。**

证据（本地源码）：

1. **字段在宏展开时构造**：`span!` / `#[instrument]` 展开时把字段值组装成 `ValueSet`；`Subscriber::new_span(&self, attrs: &Attributes<'_>)` 收到的是**只读借用**（tracing-core 0.1.36 `src/span.rs`，`Attributes` 定义于 23 行起）。
2. **`Attributes` 只读**：`Attributes::record(&self, visitor: &mut dyn Visit)`（`src/span.rs:184`）只是把字段值**遍历**给 visitor；`contains` 只做查询；`is_empty` 只判断空否。**没有任何修改/删除字段的方法。** 文档见 <https://docs.rs/tracing/latest/tracing/span/struct.Attributes.html>。
3. **Layer 各自消费**：`Layer::on_new_span(&self, attrs: &Attributes, ...)`、`on_record(&self, id, values: &Record, ...)` 拿到的都是同一份只读数据，每个 Layer 独立处理（fmt 层自己格式化、rootcause 层自己收进 `CapturedFields`、OTLP 层自己转 OTel attributes）。一个 Layer 无法改写其他 Layer 的输入。
4. **`Span::record()` 只能追加/覆盖，不能删除**：`Span::record(name, value)`（tracing 0.1.44 `src/span.rs:1193`）可以事后为已声明的字段补值/覆盖值，但（a）不能去掉已记录的字段，（b）rootcause 的 `CapturedFields` 在 `on_new_span` 时已固化进 span extensions，事后 `record` 不影响它。
5. **Span extensions 改不了字段**：`span.extensions()` / `extensions_mut()` 是挂在该 span 上的 `TypeMap`（layer 之间可共享的自定义存储，rootcause 的 `CapturedFields` 就存在这里）。它可以被任何 layer 增删（例如 fork 的 Layer 可以删掉 `CapturedFields`），但**它只是各 layer 的私有数据，不是 span 的字段本身**——fmt 层显示的字段来自它自己在 `on_new_span` 时消费的 attrs，不经过 extensions 这一层改写。
6. **官方唯一的字段级控制点在宏层**：`#[instrument]` 的 `skip(...)` / `skip_all`，官方文档原话（<https://docs.rs/tracing-attributes/latest/tracing_attributes/attr.instrument.html>，"Skipping Fields" 一节）："To skip recording one or more arguments … **to exclude an argument with a verbose or costly Debug implementation**"。skip 的字段在 `span!` 展开时根本不进 `ValueSet`，因此**所有** subscriber/layer（fmt、OTLP、rootcause……）都看不到它——这是唯一"全局生效"的字段级机制，但必须逐点标注。

### 为什么"全局"做不到的根因

tracing 的过滤架构（Filter 决定启用与否）和字段记录架构（创建时一次性、只读分发）是分离的。字段一旦进入 `Attributes`，就同时广播给所有 Layer；没有任何"在分发前改写"的钩子。这不是 tracing-subscriber 的疏漏，而是 `tracing-core` 的数据模型决定的：`Attributes`/`Record` 只有读接口。

---

## 5. 最终结论与推荐做法

### 判定

> **只能逐点 skip（官方）。**
> 存在"部分可以"的替代方案（自定义 Layer / EnvFilter 整 span 禁用），但都不是官方机制，且各有明确限制（见下）。

| 方案 | 官方？ | 能全局按字段名剔除字段？ | 对本项目的适用性 |
|---|---|---|---|
| `#[instrument(skip(pg_pool))]`（当前做法） | ✅ 官方唯一字段级机制 | 是（对所有消费者全局生效，因为字段根本不进 span） | 推荐，保持 |
| EnvFilter `[span{pg_pool}]=off` | ✅ 官方 | 否——整 span 禁用，字段连同其他有用字段一起丢 | 一般不适用（span 里还有其他要保留的字段） |
| 自定义 Layer 包一层过滤（见下） | ❌ 自研 | 只能对"自己消费的那一路"生效 | 仅当只想净化 rootcause 报告时可选 |
| 等 rootcause 加黑名单 | — | rootcause 0.13 / main 均无此功能 | 无期，不建议等待 |

### 推荐做法

1. **维持逐点 `skip(pg_pool)`**（及同类大对象参数），这是官方、零运行时成本、对所有下游消费者（console fmt、OTLP span attributes、rootcause 报告）一致的唯一方案。若嫌散落，可在 `libs/trace_kit` 中为 `#[instrument]` 归纳一个"跳过参数清单"的约定并在 code review 中落实。
2. **（可选，仅针对 rootcause 报告）** 在 `libs/trace_kit/init.rs` 中不用 `RootcauseLayer`，改用 fork 自它的小型自定义 Layer（复制其 `on_new_span` 的 Visitor，约 30 行，在 `record_debug` 里对 `field.name()` 命中黑名单（如 `pg_pool`）时直接 `return`）。这能把字段从 rootcause 的 `CapturedFields` 里剔除；**注意**：它不影响 fmt / OTLP 看到的字段——若那些消费者也要净化，仍必须用宏层 `skip`。
3. **不要试图**用 EnvFilter 字段匹配或 Span extensions 实现"全局剔除"——前者是一刀切的整 span 开关，后者根本改不了字段。
4. rootcause 侧可用 `ROOTCAUSE_TRACING=leafs` 减少冗余报告，但它控制的是"哪些报告带 span"，与字段无关，不能替代 skip。

---

## 附：证据 URL 清单

- tracing-subscriber filter 模块总览：<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/>
- EnvFilter 文档（Directives：`target[span{field=value}]=level` 语法与字段匹配语义）：<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html>
- Filter trait（enabled / event_enabled / on_new_span / on_record，输入只读）：<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/trait.Filter.html>（另见本地源码 `tracing-subscriber-0.3.23/src/layer/mod.rs` 1264 行起、`src/filter/env/mod.rs` 568–626 行）
- tracing `span::Attributes`（`record` 为只读遍历）：<https://docs.rs/tracing/latest/tracing/span/struct.Attributes.html>（另见 `tracing-core-0.1.36/src/span.rs:184`）
- `#[instrument]` skip/skip_all 官方文档：<https://docs.rs/tracing-attributes/latest/tracing_attributes/attr.instrument.html>
- rootcause-tracing main 分支源码（与 0.13.0 一致，无字段过滤）：<https://github.com/rootcause-rs/rootcause/blob/main/rootcause-tracing/src/lib.rs>
- rootcause-tracing crates.io 版本信息（0.13.0 为最新）：<https://crates.io/api/v1/crates/rootcause-tracing>
- 本地源码路径：`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{tracing-0.1.44,tracing-core-0.1.36,tracing-subscriber-0.3.23,rootcause-tracing-0.13.0}/`
- crates.io 搜索结果（无主流字段剔除 crate）：<https://crates.io/api/v1/crates?q=tracing+filter+fields>、<https://crates.io/api/v1/crates?q=tracing+redact>、<https://crates.io/api/v1/crates?q=tracing+filter+span>

> 注：rootcause 仓库的 `rootcause-tracing/CHANGELOG.md` 与根 `CHANGELOG.md` 均不存在（404/超时）；结论由 main 分支 `lib.rs` 与 0.13.0 逐字节一致 + crates.io 版本列表双重确认，不受影响。
