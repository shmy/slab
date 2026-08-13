// PostgREST 风格筛选序列化（与后端 libs/filter_kit 对齐，PostgreSQL 生态惯例，Supabase 同款）。
// 每个字段一个 query 参数，值 = `{op}.{value}`，多参数天然 AND：
//   ?q=张&name=ilike.*张*&amount=gte.1000&created_at=gt.2024-03-15
// 操作符矩阵与后端 FilterSchema 列类型一一对应（text/date/int），改矩阵两处同步：
// 后端 libs/filter_kit::FilterSchema + 本文件 TYPE_OPERATORS / OP_TO_PG。

/** 字段类型（与后端 FilterSchema 三数组对应） */
export type FilterFieldType = 'text' | 'date' | 'int';

/** 可筛字段注册：类型决定操作符集（无需逐字段手写 operators） */
export interface FilterFieldConfig {
  id: string;
  label: string;
  type: FilterFieldType;
  placeholder?: string;
}

/** FilterBar 用的条件（op 为 UI 操作符 id） */
export interface FilterCondition {
  field: string;
  op: string;
  value: string;
}

export interface OperatorOption {
  id: string;
  label: string;
}

// 类型 → 操作符集（与后端操作符矩阵对齐）：
//   text: eq / neq / ilike        date: eq / neq / gt / gte / lt / lte
//   int : eq / neq / gt / gte / lt / lte
export const TYPE_OPERATORS: Record<FilterFieldType, OperatorOption[]> = {
  text: [
    { id: 'contains', label: '包含' }, // ilike（值自动包 * 通配符）
    { id: 'eq', label: '等于' },
    { id: 'neq', label: '不等于' },
  ],
  date: [
    { id: 'eq', label: '等于' },
    { id: 'neq', label: '不等于' },
    { id: 'gt', label: '晚于' },
    { id: 'gte', label: '不早于' },
    { id: 'lt', label: '早于' },
    { id: 'lte', label: '不晚于' },
  ],
  int: [
    { id: 'eq', label: '等于' },
    { id: 'neq', label: '不等于' },
    { id: 'gt', label: '大于' },
    { id: 'gte', label: '大于等于' },
    { id: 'lt', label: '小于' },
    { id: 'lte', label: '小于等于' },
  ],
};

// UI 操作符 id ↔ PostgREST 操作符（contains 为 UI 语义名，其余与后端 op 同名）
const OP_TO_PG: Record<string, string> = {
  contains: 'ilike',
  eq: 'eq',
  neq: 'neq',
  gt: 'gt',
  gte: 'gte',
  lt: 'lt',
  lte: 'lte',
};

const PG_TO_OP: Record<string, string> = Object.fromEntries(
  Object.entries(OP_TO_PG).map(([ui, pg]) => [pg, ui]),
);

/** PostgREST 操作符表：从长到短匹配前缀（ilike. 先于 eq. 等，避免子串误配） */
const PG_OPS = ['ilike.', 'neq.', 'gte.', 'lte.', 'gt.', 'lt.', 'eq.'];

/** 条件数组 → search 参数对象（字段 → `op.value`）；contains 值自动包 `*` 通配符 */
export function serializeFilters(
  conditions: FilterCondition[],
): Record<string, string> {
  const params: Record<string, string> = {};
  for (const c of conditions) {
    const op = OP_TO_PG[c.op] ?? c.op;
    const value = op === 'ilike' ? `*${c.value}*` : c.value;
    params[c.field] = `${op}.${value}`;
  }
  return params;
}

/** 路由 search 对象（URL 动态字段）→ FilterBar 条件数组；排除 `q`；ilike 值去两侧 `*` 还原 */
export function parseFilters(
  search: Record<string, unknown>,
): FilterCondition[] {
  const out: FilterCondition[] = [];
  for (const [field, raw] of Object.entries(search)) {
    if (field === 'q' || typeof raw !== 'string') continue;
    for (const op of PG_OPS) {
      if (raw.startsWith(op)) {
        const pg = op.slice(0, -1); // 去尾点：ilike. → ilike
        const value = raw.slice(op.length);
        out.push({
          field,
          op: PG_TO_OP[pg] ?? pg,
          value: pg === 'ilike' ? value.replace(/^\*|\*$/g, '') : value,
        });
        break;
      }
    }
  }
  return out;
}
