// filters.ts 推导层测试：操作符集来自契约（filter-schema.ts），文案/序列化逻辑在推导层。
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  FILTER_OP_PREFIXES,
  FILTER_OPERATOR_MATRIX,
  filterSchemas,
} from './filter-schema.ts';
import {
  operatorOptionsFor,
  parseFilters,
  serializeFilters,
} from './filters.ts';

test('operator options derive from contract matrix', () => {
  // 操作符集与顺序来自契约（text 矩阵序：eq, neq, ilike）
  const textOps = operatorOptionsFor('text').map((o) => o.id);
  assert.deepEqual(textOps, ['eq', 'neq', 'contains']); // ilike 的 UI id 是 contains
  const dateOps = operatorOptionsFor('date').map((o) => o.id);
  assert.deepEqual(dateOps, ['eq', 'neq', 'gt', 'gte', 'lt', 'lte']);
  // date 语义文案
  const dateLabels = operatorOptionsFor('date').map((o) => o.label);
  assert.deepEqual(dateLabels, [
    '等于',
    '不等于',
    '晚于',
    '不早于',
    '早于',
    '不晚于',
  ]);
});

test('serialize contains auto-wraps asterisks; manual wildcard preserved', () => {
  const params = serializeFilters([
    { field: 'name', op: 'contains', value: '张' },
    { field: 'phone', op: 'contains', value: '11*' },
    { field: 'code', op: 'eq', value: 'C-001' },
  ]);
  assert.deepEqual(params, {
    name: 'ilike.*张*',
    phone: 'ilike.11*',
    code: 'eq.C-001',
  });
});

test('parse roundtrips serialize output; manual wildcard kept', () => {
  const search = {
    q: 'keyword',
    name: 'ilike.*张*',
    phone: 'ilike.11*',
    created_at: 'gt.2024-03-15',
  };
  const conditions = parseFilters(search);
  assert.deepEqual(conditions, [
    { field: 'name', op: 'contains', value: '张' },
    { field: 'phone', op: 'contains', value: '11*' },
    { field: 'created_at', op: 'gt', value: '2024-03-15' },
  ]);
  // 往返：serialize(parse(x)) 与 x 逐字段一致（顺序无关）
  const back = serializeFilters(conditions);
  assert.equal(back.name, 'ilike.*张*');
  assert.equal(back.phone, 'ilike.11*');
  assert.equal(back.created_at, 'gt.2024-03-15');
});

test('prefix matching is longest-first (ilike beats eq)', () => {
  // 契约前缀序：ilike. 在 eq. 之前；若从短到长会误配
  const prefixes = FILTER_OP_PREFIXES;
  assert.ok(prefixes.indexOf('ilike.') < prefixes.indexOf('eq.'));
  // 值以 ilike. 开头必须解析为 contains 而非 eq
  const conditions = parseFilters({ name: 'ilike.*a*' });
  assert.equal(conditions[0].op, 'contains');
});

test('q excluded; empty values ignored', () => {
  // 白名单校验在后端（filter_kit），parseFilters 只做 URL 往返，未知字段原样保留
  const conditions = parseFilters({ q: 'x', ok: '', name: 'eq.a' });
  assert.equal(conditions.length, 1);
  assert.equal(conditions[0].field, 'name');
});

test('contract matrix covers every schema field type', () => {
  for (const schema of Object.values(filterSchemas)) {
    for (const field of schema.fields) {
      assert.ok(
        field.type in FILTER_OPERATOR_MATRIX,
        `unknown field type ${field.type} for ${field.name}`,
      );
    }
  }
});
