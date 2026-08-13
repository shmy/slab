// PostgREST 风格筛选序列化（与后端 libs/filter_kit 对齐，PostgreSQL 生态惯例，Supabase 同款）。
// 每个字段一个 query 参数，值 = `{op}.{value}`，多参数天然 AND：
//   ?q=张&name=ilike.*张*&created_at=gt.2024-03-15
// 操作符：eq / gt / gte / lt / lte / ilike（ilike 值含通配符 `*`，contains 自动包裹）

/** FilterBar 用的条件（op 为 UI 操作符 id） */
export interface FilterCondition {
  field: string;
  op: string;
  value: string;
}

// UI 操作符 id ↔ PostgREST 操作符（保持同一集合，将来扩展操作符两处同步加）
const OP_TO_PG: Record<string, string> = {
  contains: 'ilike',
  eq: 'eq',
  after: 'gt',
  before: 'lt',
};

const PG_TO_OP: Record<string, string> = Object.fromEntries(
  Object.entries(OP_TO_PG).map(([ui, pg]) => [pg, ui]),
);

/** PostgREST 操作符表：从长到短匹配前缀（ilike. 先于 eq. 等，避免子串误配） */
const PG_OPS = ['ilike.', 'gte.', 'lte.', 'gt.', 'lt.', 'eq.'];

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
