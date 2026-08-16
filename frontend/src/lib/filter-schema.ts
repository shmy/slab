// 自动生成（pnpm gen:api）——勿手改。
// 事实源：后端 libs/filter_kit 操作符矩阵 + 各域 endpoint 的 FILTER_SCHEMA 白名单，
// 经 GET /api/v1/meta/filter-schemas 导出（bin/server/meta.rs）。改筛选协议 = 改后端，重新 gen。

export type FilterOperator =
  | 'eq'
  | 'neq'
  | 'ilike'
  | 'gt'
  | 'gte'
  | 'lt'
  | 'lte';

export type FilterFieldType = 'text' | 'date' | 'int';

/** 列类型 → 支持的操作符集（协议事实源） */
export const FILTER_OPERATOR_MATRIX: Record<FilterFieldType, FilterOperator[]> =
  {
    text: ['eq', 'neq', 'ilike'],
    date: ['eq', 'neq', 'gt', 'gte', 'lt', 'lte'],
    int: ['eq', 'neq', 'gt', 'gte', 'lt', 'lte'],
  };

/** RSQL 比较操作符（协议事实源：操作符名 → wire 串，如 eq → '=='） */
export const FILTER_COMPARISON_OPS: Record<FilterOperator, string> = {
  eq: '==',
  neq: '!=',
  ilike: '=ilike=',
  gt: '=gt=',
  gte: '=ge=',
  lt: '=lt=',
  lte: '=le=',
};

export interface FilterSchemaField {
  name: string;
  type: FilterFieldType;
}

export interface FilterSchema {
  fields: FilterSchemaField[];
}

/** 实体 → 可筛字段白名单 */
export const filterSchemas: Record<string, FilterSchema> = {
  customer: {
    fields: [
      { name: 'code', type: 'text' },
      { name: 'name', type: 'text' },
      { name: 'phone', type: 'text' },
      { name: 'contact_person', type: 'text' },
      { name: 'created_at', type: 'date' },
    ],
  },
};

/** customer 实体可筛字段（label 映射键集合） */
export type CustomerFilterField =
  | 'code'
  | 'name'
  | 'phone'
  | 'contact_person'
  | 'created_at';
