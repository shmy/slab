// RSQL 风格筛选的推导层（协议事实源 = 生成物 filter-schema.ts，勿在此手抄矩阵）。
// 核心 = react-querybuilder (RQB) 的 `RuleGroupType` 树模型来做 UI 状态，
// 序列化时把 RQB 树转换成后端 filter_kit 的 RSQL 串：
//   ?filter=(name=ilike=*张*;code==C-001),created_at=gt=2024-03-15
// RSQL 文法（`;`/`and` = AND，`,`/`or` = OR，括号分组，优先级：括号 > AND > OR）与后端对齐，
// 解析器（parseFilters）也在本文件、只读 URL 时用；RQB 负责树的增删改。
// 操作符集 / RSQL 比较串 / 字段白名单全部来自后端契约；本文件只保留 UI 文案与推导逻辑。

import type { Path, RuleGroupType, RuleType } from 'react-querybuilder';
import {
  FILTER_COMPARISON_OPS,
  FILTER_OPERATOR_MATRIX,
  type FilterFieldType,
  type FilterOperator,
} from './filter-schema.ts';

export type {
  FilterFieldType,
  FilterOperator,
  FilterSchema,
} from './filter-schema.ts';

/** 字段文案（UI copy，非协议；字段集合以契约为准） */
export interface FilterLabel {
  label: string;
  placeholder?: string;
}

/** RQB 规则 id 字段（RQB 需要唯一 id；自动生成） */
let seq = 0;
export function newId(prefix = 'r'): string {
  return `${prefix}-${Date.now().toString(36)}-${(seq++).toString(36)}`;
}

// ---------------------------------------------------------------------------
// 运算符映射：RQB 运算符名 ⇄ 后端 RSQL 比较串 / filter_kit Op
// ---------------------------------------------------------------------------
// 用 RQB 自定义运算符名（不依赖它内置的 `=`/`~` 等，避免与 RSQL 语义错位）。
// 值 = RSQL 比较串；`parseFilters`/`serializeFilters` 用它跟后端对齐。
export const RQB_OPERATOR_TO_RSQL: Record<string, string> = Object.fromEntries(
  Object.entries(FILTER_COMPARISON_OPS).map(([pg, comp]) => [pg, comp]),
);
// RSQL 比较串 → RQB 运算符名
const RSQL_TO_RQB: Record<string, string> = Object.fromEntries(
  Object.entries(FILTER_COMPARISON_OPS).map(([pg, comp]) => [comp, pg]),
);

/** 列类型 → RQB 运算符名集（对齐后端 OPERATOR_MATRIX） */
export function operatorsFor(type: FilterFieldType): string[] {
  return (FILTER_OPERATOR_MATRIX[type] ?? []).map(
    (o) => o as unknown as string,
  );
}

/** RQB 运算符显示文案 */
export const OPERATOR_LABELS: Record<string, string> = {
  eq: '等于',
  neq: '不等于',
  gt: '大于',
  gte: '大于等于',
  lt: '小于',
  lte: '小于等于',
  ilike: '包含',
};

// ---------------------------------------------------------------------------
// RQB 树 → RSQL 串
// ---------------------------------------------------------------------------

/** 单个 rule → RSQL 比较式（ilike 自动包 `*值*`；手工通配符 `*` 原样保留） */
function ruleToSql(rule: RuleType): string {
  const pg = rule.operator; // RQB 运算符名 = filter_kit Op.as_str()
  const comp = FILTER_COMPARISON_OPS[pg as FilterOperator] ?? '==';
  let value = String(rule.value ?? '');
  if (pg === 'ilike' && !value.includes('*')) {
    value = `*${value}*`;
  }
  return `${rule.field}${comp}${rsqlValue(value)}`;
}

function isGroup(r: RuleType | RuleGroupType): r is RuleGroupType {
  return 'rules' in r;
}

/** RQB 组树 → RSQL 串（根组不套括号；空组跳过） */
export function serializeFilters(group: RuleGroupType): string {
  return serializeGroup(group, false);
}

function serializeGroup(group: RuleGroupType, nested: boolean): string {
  const joiner = group.combinator === 'or' ? ',' : ';';
  const parts: string[] = [];
  for (const rule of group.rules) {
    if (isGroup(rule)) {
      const inner = serializeGroup(rule, true);
      if (inner !== '') parts.push(inner);
    } else {
      parts.push(ruleToSql(rule));
    }
  }
  const body = parts
    .map((p) => (p.startsWith('(') && group.combinator === 'and' ? p : p))
    .join(joiner);
  if (body === '') return '';
  return nested ? `(${body})` : body;
}

/** RSQL 值序列化：为空或含分隔符/空白/引号时单引号包裹，`'` 转义为 `''`。 */
export function rsqlValue(value: string): string {
  if (value === '' || /[,;()'\s]/.test(value)) {
    return `'${value.replace(/'/g, "''")}'`;
  }
  return value;
}

// ---------------------------------------------------------------------------
// RSQL 串 → RQB 树
// ---------------------------------------------------------------------------

/** RQB rule 的 operator/value 从解析结果反推；ilike 还原 UI 值 */
function makeRule(field: string, pg: string, value: string): RuleType {
  return {
    id: newId('r'),
    field,
    operator: pg,
    value,
  };
}

class RsqlParser {
  private pos = 0;
  private readonly input: string;

  constructor(input: string) {
    this.input = input;
  }

  private peek(): string | undefined {
    return this.input[this.pos];
  }

  atEnd(): boolean {
    return this.pos >= this.input.length;
  }

  skipWs(): void {
    while (!this.atEnd() && /\s/.test(this.input[this.pos] ?? '')) {
      this.pos++;
    }
  }

  private eat(s: string): boolean {
    if (this.input.startsWith(s, this.pos)) {
      this.pos += s.length;
      return true;
    }
    return false;
  }

  private eatKeyword(kw: string): boolean {
    const rest = this.input.slice(this.pos);
    if (!rest.toLowerCase().startsWith(kw)) return false;
    const after = rest[kw.length];
    if (after !== undefined && /[A-Za-z0-9_]/.test(after)) return false;
    this.pos += kw.length;
    return true;
  }

  /** 解析完整 RSQL：顶层 OR（`,`/`or`），每个 OR 分支是 AND 组或括号子树 */
  parseOr(): RuleType | RuleGroupType | null {
    const members: (RuleType | RuleGroupType)[] = [];
    const first = this.parseAndGroup();
    if (first) members.push(first);
    for (;;) {
      this.skipWs();
      if (!(this.eat(',') || this.eatKeyword('or'))) break;
      const next = this.parseAndGroup();
      if (!next) break;
      members.push(next);
    }
    if (members.length === 0) return null;
    if (members.length === 1) return members[0];
    return { id: newId('g'), combinator: 'or', rules: members };
  }

  /** 解析一个 AND 项：`(子树)` 或比较式；同层 `;`/`and` 平铺进单个 and 组 */
  private parseAndGroup(): RuleType | RuleGroupType | null {
    this.skipWs();
    const first = this.parseAtom();
    if (!first) return null;
    const rules: (RuleType | RuleGroupType)[] = [first];
    for (;;) {
      const saved = this.pos;
      this.skipWs();
      if (!(this.eat(';') || this.eatKeyword('and'))) {
        this.pos = saved;
        break;
      }
      const next = this.parseAtom();
      if (!next) {
        this.pos = saved;
        break;
      }
      rules.push(next);
    }
    if (rules.length === 1) return first;
    return { id: newId('g'), combinator: 'and', rules };
  }

  /** 原子：`( or_expr )`（保留为独立子组，即便只有一个成员）或单个比较式 */
  private parseAtom(): RuleType | RuleGroupType | null {
    this.skipWs();
    if (this.eat('(')) {
      const inner = this.parseOr();
      if (!inner) return null;
      this.skipWs();
      if (!this.eat(')')) return null;
      // 括号恒保组：单成员也包成 group，避免结构被折叠（两个单成员子组并存的必需语义）
      const rules = isGroup(inner) ? inner.rules : [inner];
      return {
        id: newId('g'),
        combinator: isGroup(inner) ? inner.combinator : 'and',
        rules,
      };
    }
    const cmp = this.parseComparison();
    if (!cmp) return null;
    return makeRule(cmp.field, cmp.pg, cmp.value);
  }

  private parseComparison(): {
    field: string;
    pg: string;
    value: string;
  } | null {
    this.skipWs();
    const fieldStart = this.pos;
    while (!this.atEnd() && /[A-Za-z0-9_]/.test(this.input[this.pos] ?? '')) {
      this.pos++;
    }
    if (this.pos === fieldStart) return null;
    const field = this.input.slice(fieldStart, this.pos);

    this.skipWs();
    let comp: string | null = null;
    const compOps = [...new Set(Object.values(FILTER_COMPARISON_OPS))].sort(
      (a, b) => b.length - a.length,
    );
    for (const op of compOps) {
      if (this.eat(op)) {
        comp = op;
        break;
      }
    }
    if (!comp) return null;

    this.skipWs();
    const value = this.parseValue();
    if (value === null) return null;

    const pg = RSQL_TO_RQB[comp];
    if (!pg) return null;
    // ilike 自动包装 `*值*` → 还原 UI 值；手工通配 `11*` 保留
    let v = value;
    if (
      pg === 'ilike' &&
      v.startsWith('*') &&
      v.endsWith('*') &&
      v.length >= 2
    ) {
      v = v.slice(1, -1);
    }
    return { field, pg, value: v };
  }

  private parseValue(): string | null {
    if (this.peek() === "'") {
      this.pos++;
      let out = '';
      while (!this.atEnd()) {
        const ch = this.input[this.pos] ?? '';
        this.pos++;
        if (ch === "'") {
          if (this.peek() === "'") {
            this.pos++;
            out += "'";
          } else {
            return out;
          }
        } else {
          out += ch;
        }
      }
      return null;
    }
    const start = this.pos;
    while (!this.atEnd()) {
      const ch = this.input[this.pos] ?? '';
      if (
        ch === ',' ||
        ch === ';' ||
        ch === '(' ||
        ch === ')' ||
        /\s/.test(ch)
      ) {
        break;
      }
      this.pos++;
    }
    if (this.pos === start) return null;
    return this.input.slice(start, this.pos);
  }
}

/** RSQL 串 → RQB 根组（无法解析 → 空 and 组） */
export function parseFilters(search: Record<string, unknown>): RuleGroupType {
  const empty: RuleGroupType = { id: newId('g'), combinator: 'and', rules: [] };
  const raw = search.filter;
  if (typeof raw !== 'string' || raw.trim() === '') return empty;
  const parser = new RsqlParser(raw);
  const node = parser.parseOr();
  if (!node) return empty;
  parser.skipWs();
  if (!parser.atEnd()) return empty;
  if (isGroup(node)) return node;
  // 根节点是单条 rule → 包成根 and 组
  return { id: newId('g'), combinator: 'and', rules: [node] };
}

// ---------------------------------------------------------------------------
// 空过滤 / 无筛选
// ---------------------------------------------------------------------------

/** 过滤是否为空（无任何条件） */
export function isEmptyFilters(group: RuleGroupType): boolean {
  return group.rules.length === 0;
}

/** 条件总数（徽标用） */
export function countConditions(group: RuleGroupType): number {
  let n = 0;
  for (const r of group.rules) {
    if (isGroup(r)) n += countConditions(r);
    else n++;
  }
  return n;
}

// ---------------------------------------------------------------------------
// 组内字段去重（onAddRule 拦截用）
// ---------------------------------------------------------------------------

/** RQB 组内成员判断 */
export function isRuleGroup(r: RuleType | RuleGroupType): r is RuleGroupType {
  return 'rules' in r;
}

/** 沿 path 取到「目标组」（path 即 RQB onAddRule 的 parentPath：根组为空，嵌套组为到该组的路径） */
export function groupAtPath(group: RuleGroupType, path: Path): RuleGroupType {
  let cur = group;
  for (let i = 0; i < path.length; i++) {
    const member = cur.rules[path[i]];
    if (!member || !isRuleGroup(member)) break;
    cur = member;
  }
  return cur;
}

/** 找组内直接成员（不含嵌套组内）已用字段 */
export function usedFieldsIn(group: RuleGroupType): Set<string> {
  const used = new Set<string>();
  for (const r of group.rules) {
    if (!isRuleGroup(r)) used.add(r.field);
  }
  return used;
}
