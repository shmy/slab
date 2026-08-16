// filters.ts 推导层测试：RQB 布尔树 ⇄ RSQL 串 双向转换 + 契约一致性。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { RuleGroupType, RuleType } from 'react-querybuilder';

import {
  FILTER_COMPARISON_OPS,
  FILTER_OPERATOR_MATRIX,
  filterSchemas,
} from './filter-schema.ts';
import {
  countConditions,
  groupAtPath,
  isEmptyFilters,
  operatorsFor,
  parseFilters,
  rsqlValue,
  serializeFilters,
  usedFieldsIn,
} from './filters.ts';

function stripIds(obj: unknown): unknown {
  if (Array.isArray(obj)) return obj.map(stripIds);
  if (obj && typeof obj === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
      if (k === 'id') continue;
      out[k] = stripIds(v);
    }
    return out;
  }
  return obj;
}
const norm = (q: RuleGroupType) => stripIds(q);

function rule(field: string, operator: string, value: unknown): RuleType {
  return { field, operator, value };
}
function and(rules: RuleGroupType['rules']): RuleGroupType {
  return { combinator: 'and', rules };
}
function or(rules: RuleGroupType['rules']): RuleGroupType {
  return { combinator: 'or', rules };
}
const EMPTY: RuleGroupType = and([]);

test('operatorsFor align with backend operator matrix', () => {
  assert.deepEqual(operatorsFor('text'), ['eq', 'neq', 'ilike']);
  assert.deepEqual(operatorsFor('date'), [
    'eq',
    'neq',
    'gt',
    'gte',
    'lt',
    'lte',
  ]);
  assert.deepEqual(operatorsFor('int'), [
    'eq',
    'neq',
    'gt',
    'gte',
    'lt',
    'lte',
  ]);
  // 未知类型防御空集
  assert.deepEqual(operatorsFor('text' as never), ['eq', 'neq', 'ilike']);
});

test('serialize RQB tree → RSQL (flat AND, or, nesting)', () => {
  // flat AND
  assert.equal(
    serializeFilters(
      and([rule('name', 'ilike', '张'), rule('code', 'eq', 'C-001')]),
    ),
    'name=ilike=*张*;code==C-001',
  );
  // OR → comma
  assert.equal(
    serializeFilters(
      or([rule('name', 'eq', '张伟'), rule('code', 'eq', 'C-002')]),
    ),
    'name==张伟,code==C-002',
  );
  // nesting: (a || b) && c
  assert.equal(
    serializeFilters(
      and([
        or([rule('name', 'eq', '张伟'), rule('code', 'eq', 'C-002')]),
        rule('created_at', 'gt', '2024-03-15'),
      ]),
    ),
    '(name==张伟,code==C-002);created_at=gt=2024-03-15',
  );
});

test('serialize quotes values with delimiters/whitespace/quotes; wildcard kept', () => {
  assert.equal(rsqlValue('a,b;c (d)'), "'a,b;c (d)'");
  assert.equal(rsqlValue("it's"), "'it''s'");
  assert.equal(rsqlValue(''), "''");
  assert.equal(rsqlValue('plain'), 'plain');
  assert.equal(
    serializeFilters(and([rule('name', 'eq', "a,b;c (d)'e")])),
    "name=='a,b;c (d)''e'",
  );
  // 手工通配符保留
  assert.equal(
    serializeFilters(and([rule('phone', 'ilike', '11*')])),
    'phone=ilike=11*',
  );
});

test('parseFilters RSQL → RQB tree', () => {
  // flat AND
  assert.deepEqual(
    norm(parseFilters({ filter: 'name=ilike=*张*;code==C-001' })),
    and([rule('name', 'ilike', '张'), rule('code', 'eq', 'C-001')]),
  );
  // OR
  assert.deepEqual(
    norm(parseFilters({ filter: 'name==张伟,code==C-002' })),
    or([rule('name', 'eq', '张伟'), rule('code', 'eq', 'C-002')]),
  );
  // 括号 + 优先级：a, b; c → a OR (b AND c)
  assert.deepEqual(
    norm(parseFilters({ filter: 'name==a,code==b;amount=gt=5' })),
    or([
      rule('name', 'eq', 'a'),
      and([rule('code', 'eq', 'b'), rule('amount', 'gt', '5')]),
    ]),
  );
  // (a, b); c → (a OR b) AND c
  assert.deepEqual(
    norm(parseFilters({ filter: '(name==a,code==b);amount=gt=5' })),
    and([
      or([rule('name', 'eq', 'a'), rule('code', 'eq', 'b')]),
      rule('amount', 'gt', '5'),
    ]),
  );
});

test('parseFilters handles quoted values / keywords / roundtrip', () => {
  assert.deepEqual(
    norm(parseFilters({ filter: "name=='a,b;c (d)''e'" })),
    and([rule('name', 'eq', "a,b;c (d)'e")]),
  );
  assert.deepEqual(
    norm(parseFilters({ filter: 'name==张 and code==C-001' })),
    and([rule('name', 'eq', '张'), rule('code', 'eq', 'C-001')]),
  );
  assert.deepEqual(
    norm(parseFilters({ filter: 'name==张 OR code==C-001' })),
    or([rule('name', 'eq', '张'), rule('code', 'eq', 'C-001')]),
  );
  // roundtrip
  const q = parseFilters({
    filter: 'name=ilike=*张*;phone=ilike=11*;created_at=gt=2024-03-15',
  });
  assert.equal(
    serializeFilters(q),
    'name=ilike=*张*;phone=ilike=11*;created_at=gt=2024-03-15',
  );
});

test('parseFilters malformed / empty → empty and group', () => {
  assert.deepEqual(norm(parseFilters({ q: 'x' })), and([]));
  assert.deepEqual(norm(parseFilters({ filter: '' })), and([]));
  assert.deepEqual(norm(parseFilters({ filter: '(name==a' })), and([]));
  assert.deepEqual(norm(parseFilters({ filter: 'name=foo=张' })), and([]));
  assert.deepEqual(norm(parseFilters({ filter: 'name==a)' })), and([]));
});

test('isEmptyFilters & countConditions', () => {
  assert.equal(isEmptyFilters(EMPTY), true);
  assert.equal(isEmptyFilters(and([rule('a', 'eq', '1')])), false);
  assert.equal(countConditions(EMPTY), 0);
  assert.equal(
    countConditions(
      and([
        rule('a', 'eq', '1'),
        or([rule('b', 'eq', '2'), rule('c', 'eq', '3')]),
      ]),
    ),
    3,
  );
});

test('contract comparison ops cover every matrix operator', () => {
  for (const ops of Object.values(FILTER_OPERATOR_MATRIX)) {
    for (const op of ops) {
      assert.ok(FILTER_COMPARISON_OPS[op], `missing RSQL comparison for ${op}`);
    }
  }
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

test('groupAtPath locates the parent group for onAddRule dedup', () => {
  const root: RuleGroupType = and([
    rule('name', 'eq', 'a'),
    or([rule('code', 'eq', 'b'), rule('phone', 'eq', 'c')]),
  ]);
  // 根组（path 空）
  assert.equal(groupAtPath(root, []).combinator, 'and');
  assert.equal(groupAtPath(root, []).rules.length, 2);
  // 根组第 1 个成员（or 子组）
  const sub = groupAtPath(root, [1]);
  assert.equal(sub.combinator, 'or');
  // 子组再嵌套：往 [1] 组的第 0 个成员上再加组
  const deeper: RuleGroupType = and([
    and([rule('a', 'eq', '1')]),
    rule('b', 'eq', '2'),
  ]);
  assert.equal(groupAtPath(deeper, [0]).combinator, 'and');
});

test('usedFieldsIn only counts direct members (not nested group internals)', () => {
  // 直接成员含 name/phone（or 子组本身不算成员，其内部 code 不计入）
  const group: RuleGroupType = and([
    rule('name', 'eq', 'a'),
    or([rule('code', 'eq', 'b')]),
    rule('phone', 'eq', 'c'),
  ]);
  assert.deepEqual([...usedFieldsIn(group)].sort(), ['name', 'phone']);
  // 嵌套组内部字段不计入外层
  const root: RuleGroupType = or([
    rule('name', 'eq', 'a'),
    and([rule('code', 'eq', 'b')]),
  ]);
  assert.deepEqual([...usedFieldsIn(root)].sort(), ['name']);
});

test('parseFilters preserves sibling sub-groups (no folding)', () => {
  const tree: RuleGroupType = and([
    rule('name', 'eq', '张'),
    or([rule('code', 'eq', 'C-001')]),
    or([rule('phone', 'eq', '138')]),
  ]);
  const sql = serializeFilters(tree);
  assert.equal(sql, 'name==张;(code==C-001);(phone==138)');
  const back = parseFilters({ filter: sql });
  // 根组 3 个成员：name + 两个独立子组（不被折叠成裸规则）
  assert.equal(back.rules.length, 3);
  assert.deepEqual(stripIds(back.rules[0]), stripIds(rule('name', 'eq', '张')));
  assert.deepEqual(
    stripIds(back.rules[1]),
    stripIds(and([rule('code', 'eq', 'C-001')])),
  );
  assert.deepEqual(
    stripIds(back.rules[2]),
    stripIds(and([rule('phone', 'eq', '138')])),
  );
});
