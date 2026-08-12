// 从运行中的后端拉取 OpenAPI 契约，存为 openapi.json 快照。
// 后端原生提供 GET /openapi.json（bin/server/router.rs 序列化 ApiDoc::openapi()）。
// 用法：pnpm gen:api（需后端已启动；可用 SLAB_API_BASE 覆盖地址）
import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const BASE = process.env.SLAB_API_BASE ?? 'http://127.0.0.1:8081';

const res = await fetch(`${BASE}/openapi.json`);
if (!res.ok)
  throw new Error(`GET /openapi.json -> ${res.status}（后端启动了吗？）`);
const spec = await res.json();

const out = resolve(process.cwd(), 'openapi.json');
writeFileSync(out, `${JSON.stringify(spec, null, 2)}\n`);
console.log(
  `✓ ${Object.keys(spec.paths).length} paths / ${
    Object.keys(spec.components?.schemas ?? {}).length
  } schemas -> ${out}`,
);
