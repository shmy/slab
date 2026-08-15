// 从运行中的后端拉取前端契约：
//  1. GET /openapi.json           → openapi.json 快照（openapi-typescript 生成 api-schema.d.ts 的输入）
//  2. GET /api/v1/meta/filter-schemas → src/lib/filter-schema.ts（筛选协议事实源，自动生成）
// 后端原生提供（bin/server/router.rs）。用法：pnpm gen:api（需后端已启动；SLAB_API_BASE 覆盖地址）。
import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const BASE = process.env.SLAB_API_BASE ?? 'http://127.0.0.1:8081';

// ---- 1. OpenAPI 快照 ----
const res = await fetch(`${BASE}/openapi.json`);
if (!res.ok)
  throw new Error(`GET /openapi.json -> ${res.status}（后端启动了吗？）`);
const spec = await res.json();
const specOut = resolve(process.cwd(), 'openapi.json');
writeFileSync(specOut, `${JSON.stringify(spec, null, 2)}\n`);
console.log(
  `✓ ${Object.keys(spec.paths).length} paths / ${
    Object.keys(spec.components?.schemas ?? {}).length
  } schemas -> ${specOut}`,
);

// ---- 2. 筛选协议（filter_kit 矩阵 + 各域 FILTER_SCHEMA 白名单）----
const metaRes = await fetch(`${BASE}/api/v1/meta/filter-schemas`);
if (!metaRes.ok)
  throw new Error(`GET /api/v1/meta/filter-schemas -> ${metaRes.status}`);
const meta = await metaRes.json();

const ops = new Set(Object.values(meta.operatorMatrix).flat().filter(Boolean));
const opUnion = [...ops].map((o) => `'${o}'`).join(' | ');
const matrixLines = Object.entries(meta.operatorMatrix)
  .map(
    ([kind, list]) => `  ${kind}: [${list.map((o) => `'${o}'`).join(', ')}],`,
  )
  .join('\n');
const prefixLines = meta.opPrefixes.map((p) => `  '${p}',`).join('\n');
const entityLines = Object.entries(meta.entities)
  .map(([name, ent]) => {
    const fields = ent.fields
      .map((f) => `    { name: '${f.name}', type: '${f.type}' },`)
      .join('\n');
    return `  ${name}: {\n    fields: [\n${fields}\n    ],\n  },`;
  })
  .join('\n');
// 每实体的字段名联合类型（label 映射用 satisfies Record<XxxFilterField, ...> 编译期强制补全）
const fieldUnionLines = Object.entries(meta.entities)
  .map(([name, ent]) => {
    const names = ent.fields.map((f) => `'${f.name}'`).join(' | ');
    const typeName = `${name[0].toUpperCase()}${name.slice(1)}FilterField`;
    return `/** ${name} 实体可筛字段（label 映射键集合） */\nexport type ${typeName} = ${names};`;
  })
  .join('\n\n');

const generated = `// 自动生成（pnpm gen:api）——勿手改。
// 事实源：后端 libs/filter_kit 操作符矩阵 + 各域 endpoint 的 FILTER_SCHEMA 白名单，
// 经 GET /api/v1/meta/filter-schemas 导出（bin/server/meta.rs）。改筛选协议 = 改后端，重新 gen。

export type FilterOperator = ${opUnion};

export type FilterFieldType = ${Object.keys(meta.operatorMatrix)
  .map((k) => `'${k}'`)
  .join(' | ')};

/** 列类型 → 支持的操作符集（协议事实源） */
export const FILTER_OPERATOR_MATRIX: Record<FilterFieldType, FilterOperator[]> = {
${matrixLines}
};

/** 操作符前缀（含尾点，从长到短匹配） */
export const FILTER_OP_PREFIXES: string[] = [
${prefixLines}
];

export interface FilterSchemaField {
  name: string;
  type: FilterFieldType;
}

export interface FilterSchema {
  fields: FilterSchemaField[];
}

/** 实体 → 可筛字段白名单 */
export const filterSchemas: Record<string, FilterSchema> = {
${entityLines}
};

${fieldUnionLines}
`;

const filterOut = resolve(process.cwd(), 'src/lib/filter-schema.ts');
writeFileSync(filterOut, generated);
console.log(
  `✓ filter-schema.ts（${Object.keys(meta.entities).length} 实体，${
    Object.keys(meta.operatorMatrix).length
  } 种列类型）-> ${filterOut}`,
);
