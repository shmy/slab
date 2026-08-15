// PostgREST 风格筛选的纯推导层（协议事实源 = 生成物 filter-schema.ts，勿在此手抄矩阵）。
// 每个字段一个 query 参数，值 = `{op}.{value}`，多参数天然 AND：
//   ?q=张&name=ilike.*张*&amount=gte.1000&created_at=gt.2024-03-15
// 操作符集 / 前缀序 / 字段白名单全部来自后端契约；本文件只保留 UI 文案与推导逻辑。

import {
  FILTER_OP_PREFIXES,
  FILTER_OPERATOR_MATRIX,
  type FilterFieldType,
  type FilterOperator,
} from './filter-schema.ts';

export type { FilterFieldType, FilterSchema } from './filter-schema.ts';

/** 字段文案（UI copy，非协议；字段集合以契约为准） */
export interface FilterLabel {
  label: string;
  placeholder?: string;
}

/** FilterBar 用的条件（op 为 UI 操作符 id：contains/eq/neq/gt/gte/lt/lte） */
export interface FilterCondition {
  field: string;
  op: string;
  value: string;
}

export interface OperatorOption {
  id: string;
  label: string;
}

// UI 操作符 id ↔ 协议操作符（ilike 在 UI 语义名是 contains，其余同名）
const UI_TO_PG: Record<string, FilterOperator> = {
  contains: 'ilike',
};
const PG_TO_UI: Record<string, string> = Object.fromEntries(
  Object.entries(UI_TO_PG).map(([ui, pg]) => [pg, ui]),
);

// 操作符文案（UI copy）：date 用时间语义，其余用数值/通用语义。
// 未列出的操作符（契约新增）回退显示协议名。
const OP_LABELS: Partial<Record<FilterOperator, string>> = {
  eq: '等于',
  neq: '不等于',
  ilike: '包含',
};
const DATE_OP_LABELS: Partial<Record<FilterOperator, string>> = {
  gt: '晚于',
  gte: '不早于',
  lt: '早于',
  lte: '不晚于',
};
const NUM_OP_LABELS: Partial<Record<FilterOperator, string>> = {
  gt: '大于',
  gte: '大于等于',
  lt: '小于',
  lte: '小于等于',
};

/** 列类型 → 操作符选项（操作符集来自契约矩阵，文案来自 UI 表） */
export function operatorOptionsFor(type: FilterFieldType): OperatorOption[] {
  return (FILTER_OPERATOR_MATRIX[type] ?? []).map((op) => {
    const uiId = PG_TO_UI[op] ?? op;
    const label =
      (type === 'date' ? DATE_OP_LABELS[op] : undefined) ??
      (type === 'int' ? NUM_OP_LABELS[op] : undefined) ??
      OP_LABELS[op] ??
      op;
    return { id: uiId, label };
  });
}

/** 条件数组 → search 参数对象（字段 → `op.value`）。
 * contains（ilike）：值不含通配符时自动包两侧（`*值*` = 包含）；
 * 值已含 `*`（手工通配符，如 `11*`）则原样传递，不重复包装。 */
export function serializeFilters(
  conditions: FilterCondition[],
): Record<string, string> {
  const params: Record<string, string> = {};
  for (const c of conditions) {
    const op = UI_TO_PG[c.op] ?? c.op;
    const value =
      op === 'ilike' && !c.value.includes('*') ? `*${c.value}*` : c.value;
    params[c.field] = `${op}.${value}`;
  }
  return params;
}

/** 路由 search 对象（URL 动态字段）→ FilterBar 条件数组；排除 `q`。
 * ilike 值：仅当首尾都是 `*`（自动包装形态 `*值*`）时去两侧还原；
 * 单侧星号（如 `11*` 前缀匹配 / `*11` 后缀匹配）视为手工通配符，原样保留。 */
export function parseFilters(
  search: Record<string, unknown>,
): FilterCondition[] {
  const out: FilterCondition[] = [];
  for (const [field, raw] of Object.entries(search)) {
    if (field === 'q' || typeof raw !== 'string') continue;
    for (const prefix of FILTER_OP_PREFIXES) {
      if (raw.startsWith(prefix)) {
        const pg = prefix.slice(0, -1); // 去尾点：ilike. → ilike
        const value = raw.slice(prefix.length);
        out.push({
          field,
          op: PG_TO_UI[pg] ?? pg,
          value:
            pg === 'ilike' && value.startsWith('*') && value.endsWith('*')
              ? value.slice(1, -1)
              : value,
        });
        break;
      }
    }
  }
  return out;
}
