// 条件构建器：主搜索框（多字段模糊）+ React Query Builder 布尔树（生产级筛选编辑器）。
// 架构：RQB query 由 FilterBar 内部 state 持有（受控），URL(query) 仅作初始值；
// 本地编辑直接更新内部 state，写 URL（触发搜索）用 debounce —— 停止输入后才落 URL，
// 避免每个字符/每次增删都触发搜索；从不回读 URL（避免 serialize→URL→重parse 的 id 抖动）。
// 「＋条件/＋组」/规则值都用 Popover（GitHub 筛选条风格）就地编辑，点「确定」才真正变更。
// 同组内禁止重复字段（字段下拉禁用已用项 + onAddRule 兜底）。
// 可筛字段/操作符集来自生成契约（filter-schema.ts），文案来自 labels——页面只声明 label。
import { Check, ChevronDown, Filter, Plus, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  type ActionProps,
  add,
  type CombinatorSelectorProps,
  type Field,
  type FieldSelectorProps,
  type OperatorSelectorProps,
  type Path,
  QueryBuilder,
  type RuleGroupType,
  type RuleType,
  toFlatOptionArray,
  type ValueEditorProps,
} from 'react-querybuilder';
import 'react-querybuilder/dist/query-builder.css';
import './filter-bar.css';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import type { FilterSchema } from '@/lib/filter-schema';
import {
  countConditions,
  groupAtPath,
  isEmptyFilters,
  newId,
  OPERATOR_LABELS,
  operatorsFor,
  serializeFilters,
  usedFieldsIn,
} from '@/lib/filters';
import { cn } from '@/lib/utils';

interface FilterBarProps {
  /** 筛选布尔树（RQB 根组，空 = 无筛选）；URL 反序列化的初始值 */
  query: RuleGroupType;
  /** 可筛字段契约（filter-schema.ts，后端 FILTER_SCHEMA 导出） */
  schema: FilterSchema;
  /** 字段文案映射（键须覆盖 schema 全部字段，页面用 satisfies Record<XxxFilterField, ...> 强制） */
  labels: Record<string, FilterLabel>;
  /** 树变更回调（父组件把可序列化部分写进 URL） */
  onQueryChange: (query: RuleGroupType) => void;
}

interface FilterLabel {
  label: string;
  placeholder?: string;
}

/** 带类型标注的字段配置（type 供定位操作符矩阵；RQB Field 会剥离 type） */
interface FieldConfig {
  name: string;
  label: string;
  placeholder?: string;
  type: 'text' | 'date' | 'int';
  inputType: 'text' | 'date' | 'number';
  operators: string[];
}

const DEBOUNCE_MS = 300;

// ---- 弹出控件（模块级，hook 在组件顶层，RQB 按组件渲染，状态稳定不重挂载）----

/** ＋条件 下拉：就地选字段/操作符/值，确定才 add() */
interface FilterCtx {
  fieldConfigs: FieldConfig[];
  query: RuleGroupType;
  onCommit: (next: RuleGroupType) => void;
}

/** 弹层底部：取消 / 确定 按钮组 */
function EditorFooter({
  onCancel,
  onConfirm,
  disabled,
}: {
  onCancel: () => void;
  onConfirm: () => void;
  disabled?: boolean;
}) {
  return (
    <div className="rqb-editor-footer">
      <Button size="sm" variant="ghost" onClick={onCancel}>
        取消
      </Button>
      <Button size="sm" onClick={onConfirm} disabled={disabled}>
        确定
      </Button>
    </div>
  );
}
function AddRuleDropdown({ path, context }: ActionProps) {
  const { fieldConfigs, query, onCommit } = context as FilterCtx;
  const [open, setOpen] = useState(false);
  const parent = groupAtPath(query, path);
  const used = parent ? usedFieldsIn(parent) : new Set<string>();
  const first = fieldConfigs.find((f) => !used.has(f.name));
  const [field, setField] = useState(first?.name ?? '');
  const [op, setOp] = useState(first?.operators[0] ?? 'eq');
  const [value, setValue] = useState('');

  function openDd() {
    const f = fieldConfigs.find((x) => !used.has(x.name));
    if (!f) {
      toast.error('该组已使用全部可筛字段，请删除条件后再添加');
      return;
    }
    setField(f.name);
    setOp(f.operators[0] ?? 'eq');
    setValue('');
    setOpen(true);
  }

  function onFieldChange(name: string) {
    setField(name);
    const cfg = fieldConfigs.find((x) => x.name === name);
    setOp(cfg?.operators[0] ?? 'eq');
    setValue('');
  }

  function confirm() {
    if (used.has(field)) {
      toast.error('该字段在组内已存在，请选择其他字段');
      return;
    }
    if (!value.trim()) {
      toast.error('请填写筛选值');
      return;
    }
    const rule: RuleType = {
      id: newId('r'),
      field,
      operator: op,
      value: value.trim(),
    };
    onCommit(add(query, rule, path));
    setOpen(false);
  }

  const fieldCfg = fieldConfigs.find((f) => f.name === field);

  return (
    <Popover open={open} onOpenChange={(o) => (o ? openDd() : setOpen(false))}>
      <PopoverTrigger className="rqb-action rqb-add-rule" title="添加条件">
        <Plus className="size-3.5" />
        条件
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64 p-2">
        <div className="flex flex-col gap-2">
          <div className="flex flex-col gap-1">
            <label
              htmlFor={`rqb-f-${path.join('.')}`}
              className="text-xs text-muted-foreground"
            >
              字段
            </label>
            <RqbSelect
              id={`rqb-f-${path.join('.')}`}
              value={field}
              onChange={onFieldChange}
              options={fieldConfigs.map((f) => ({
                value: f.name,
                label: f.label,
                disabled: used.has(f.name),
              }))}
              ariaLabel="筛选字段"
            />
          </div>
          <div className="flex flex-col gap-1">
            <label
              htmlFor={`rqb-o-${path.join('.')}`}
              className="text-xs text-muted-foreground"
            >
              操作符
            </label>
            <RqbSelect
              id={`rqb-o-${path.join('.')}`}
              value={op}
              onChange={(v) => setOp(v)}
              options={(fieldCfg?.operators ?? ['eq']).map((o) => ({
                value: o,
                label: OPERATOR_LABELS[o] ?? o,
              }))}
              ariaLabel="操作符"
            />
          </div>
          <div className="flex flex-col gap-1">
            <label
              htmlFor={`rqb-v-${path.join('.')}`}
              className="text-xs text-muted-foreground"
            >
              值
            </label>
            <Input
              id={`rqb-v-${path.join('.')}`}
              autoFocus
              type={
                fieldCfg?.type === 'date'
                  ? 'date'
                  : fieldCfg?.type === 'int'
                    ? 'number'
                    : 'text'
              }
              placeholder={fieldCfg?.placeholder}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') confirm();
                if (e.key === 'Escape') setOpen(false);
              }}
            />
          </div>
          <EditorFooter
            onCancel={() => setOpen(false)}
            onConfirm={confirm}
            disabled={!value.trim()}
          />
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** ＋组 下拉：选 并且/或者，确定才加入 */
function AddGroupDropdown({ path, context }: ActionProps) {
  const { query, onCommit } = context as FilterCtx;
  const [open, setOpen] = useState(false);
  const [connector, setConnector] = useState<'and' | 'or'>('and');

  function confirm() {
    const group: RuleGroupType = {
      id: newId('g'),
      combinator: connector,
      rules: [],
    };
    onCommit(add(query, group, path));
    setOpen(false);
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className="rqb-action rqb-add-group" title="添加分组">
        <Plus className="size-3.5" />组
      </PopoverTrigger>
      <PopoverContent align="start" className="w-48 p-2">
        <div className="flex flex-col gap-2">
          <span className="text-xs text-muted-foreground">组内组合方式</span>
          <div className="flex items-center gap-2">
            {(['and', 'or'] as const).map((c) => (
              <Button
                key={c}
                size="sm"
                variant={connector === c ? 'default' : 'outline'}
                onClick={() => setConnector(c)}
              >
                {c === 'and' ? '并且' : '或者'}
              </Button>
            ))}
          </div>
          <EditorFooter onCancel={() => setOpen(false)} onConfirm={confirm} />
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** 规则值编辑器：值显示为胶囊按钮，点击 Popover 就地编辑，确定才 handleOnChange
 * （配合 debounce，不在每个字符时写 URL 触发搜索 / 丢焦点） */
function RuleValueEditor({
  value,
  handleOnChange,
  inputType,
  fieldData,
}: ValueEditorProps) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(String(value ?? ''));
  const isEmpty = value === undefined || value === null || value === '';
  const inputKind =
    inputType === 'number' ? 'number' : inputType === 'date' ? 'date' : 'text';

  function openEdit() {
    setDraft(String(value ?? ''));
    setOpen(true);
  }
  function confirm() {
    handleOnChange(draft.trim());
    setOpen(false);
  }

  return (
    <Popover
      open={open}
      onOpenChange={(o) => (o ? openEdit() : setOpen(false))}
    >
      <PopoverTrigger
        className={cn('rqb-value-btn', isEmpty && 'rqb-value-empty')}
        title="编辑值"
      >
        {isEmpty ? '未填写' : String(value)}
      </PopoverTrigger>
      <PopoverContent align="start" className="w-56 p-2">
        <div className="flex flex-col gap-2">
          <Input
            autoFocus
            type={inputKind}
            placeholder={fieldData.placeholder ?? '筛选值'}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') confirm();
              if (e.key === 'Escape') setOpen(false);
            }}
          />
          <EditorFooter onCancel={() => setOpen(false)} onConfirm={confirm} />
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** 删除按钮：lucide × 图标（替换 RQB 默认文本 ×，更清晰） */
function RemoveAction({ handleOnClick, className }: ActionProps) {
  return (
    <button
      type="button"
      className={className}
      title="删除"
      aria-label="删除"
      onClick={() => handleOnClick()}
    >
      <X className="size-4" />
    </button>
  );
}

/** 美化下拉：native <select> + lucide 箭头（appearance:none，隐藏浏览器默认样式） */
function RqbSelect({
  value,
  onChange,
  options,
  disabled,
  className,
  ariaLabel,
  id,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string; disabled?: boolean }[];
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
  id?: string;
}) {
  const [open, setOpen] = useState(false);
  const current = options.find((o) => o.value === value);
  const showEmpty = options.length === 0;

  function pick(option: { value: string; label: string; disabled?: boolean }) {
    if (option.disabled) return;
    onChange(option.value);
    setOpen(false);
  }

  return (
    <div id={id} className={cn('rqb-select', className)}>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          className={cn(
            'rqb-select-trigger',
            disabled && 'rqb-select-disabled',
          )}
          disabled={disabled || showEmpty}
          title={ariaLabel}
        >
          <span className="rqb-select-value">
            {showEmpty ? '—' : (current?.label ?? '—')}
          </span>
          <ChevronDown
            className={cn(
              'rqb-select-chevron',
              open && 'rqb-select-chevron-up',
            )}
          />
        </PopoverTrigger>
        <PopoverContent
          align="start"
          sideOffset={4}
          className="rqb-select-panel"
        >
          {options.map((o) => (
            <button
              key={o.value}
              type="button"
              disabled={o.disabled}
              className={cn(
                'rqb-select-item',
                o.value === value && 'rqb-select-item-active',
                o.disabled && 'rqb-select-item-disabled',
              )}
              onClick={() => pick(o)}
            >
              {o.label}
              {o.value === value && <Check className="rqb-select-check" />}
            </button>
          ))}
        </PopoverContent>
      </Popover>
    </div>
  );
}

/** 组合器（并且/或者）下拉 */
function CombinatorSelector({
  value,
  handleOnChange,
  options,
}: CombinatorSelectorProps) {
  const flat = toFlatOptionArray(options) as { name: string; label: string }[];
  const COMB_LABELS: Record<string, string> = { and: '并且', or: '或者' };
  return (
    <RqbSelect
      value={(value ?? '').toString()}
      onChange={handleOnChange}
      options={flat.map((o) => ({
        value: o.name,
        label: COMB_LABELS[o.name] ?? o.label,
      }))}
      ariaLabel="组内组合方式"
      className="rqb-combinator"
    />
  );
}

/** 规则行操作符下拉 */
function OperatorSelector({
  value,
  handleOnChange,
  options,
}: OperatorSelectorProps) {
  const flat = toFlatOptionArray(options) as { name: string; label: string }[];
  return (
    <RqbSelect
      value={(value ?? '').toString()}
      onChange={handleOnChange}
      options={flat.map((o) => ({
        value: o.name,
        label: OPERATOR_LABELS[o.name] ?? o.name,
      }))}
      ariaLabel="操作符"
    />
  );
}

export function FilterBar({
  query,
  schema,
  labels,
  onQueryChange,
}: FilterBarProps) {
  // 契约字段 × 前端文案 → 字段配置
  const fieldConfigs = useMemo<FieldConfig[]>(
    () =>
      schema.fields.map((f) => ({
        name: f.name,
        label: labels[f.name]?.label ?? f.name,
        placeholder: labels[f.name]?.placeholder,
        type: f.type,
        inputType:
          f.type === 'date' ? 'date' : f.type === 'int' ? 'number' : 'text',
        operators: operatorsFor(f.type),
      })),
    [schema, labels],
  );
  const fields = useMemo<Field[]>(
    () => fieldConfigs.map(({ type: _t, ...rest }) => rest as Field),
    [fieldConfigs],
  );

  // 内部受控 state：以 URL 反序列化的 query 为初值
  const [localQuery, setLocalQuery] = useState<RuleGroupType>(() =>
    query && !isEmptyFilters(query)
      ? query
      : { id: newId('g'), combinator: 'and', rules: [] },
  );

  // URL 初值（外部变化才重置本地）
  const externalQueryKey = useMemo(() => serializeFilters(query), [query]);
  const initialKey = useRef(externalQueryKey);
  useEffect(() => {
    const currentKey = serializeFilters(localQuery);
    if (
      externalQueryKey !== initialKey.current &&
      externalQueryKey !== currentKey
    ) {
      setLocalQuery(
        query && !isEmptyFilters(query)
          ? query
          : { id: newId('g'), combinator: 'and', rules: [] },
      );
      initialKey.current = externalQueryKey;
    }
  }, [externalQueryKey, localQuery, query]);

  const conditionCount = useMemo(
    () => countConditions(localQuery),
    [localQuery],
  );
  const hasFilters = !isEmptyFilters(localQuery);

  /** RQB 树编辑：localQuery 即时更新（不丢焦点）；写 URL 用 debounce（避免每字符搜索） */
  const searchDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  function handleQueryChange(next: RuleGroupType) {
    setLocalQuery(next);
    if (searchDebounceRef.current) clearTimeout(searchDebounceRef.current);
    searchDebounceRef.current = setTimeout(() => {
      onQueryChange(next);
      searchDebounceRef.current = null;
    }, DEBOUNCE_MS);
  }
  useEffect(
    () => () => {
      if (searchDebounceRef.current) clearTimeout(searchDebounceRef.current);
    },
    [],
  );

  /** 同组内禁止重复字段：兜底拦截 */
  function handleAddRule(
    rule: RuleType,
    _parentPath: Path,
    curQuery: RuleGroupType,
  ): RuleType | boolean {
    const parent = groupAtPath(curQuery, _parentPath);
    const used = parent ? usedFieldsIn(parent) : new Set<string>();
    if (used.has(rule.field)) {
      const available = fieldConfigs.find((f) => !used.has(f.name));
      if (available) {
        return {
          ...rule,
          field: available.name,
          operator: available.operators[0] ?? 'eq',
        };
      }
      return false;
    }
    return rule;
  }

  /** 字段下拉：禁用同组已用字段（当前规则自身所选字段除外） */
  const fieldSelector = function FilterFieldSelector({
    value,
    options,
    handleOnChange,
    path,
    disabled,
  }: FieldSelectorProps) {
    const parent = groupAtPath(localQuery, path.slice(0, -1));
    const used = parent ? usedFieldsIn(parent) : new Set<string>();
    used.delete(value ?? '');
    const flat = toFlatOptionArray(options).filter((o) => 'name' in o) as {
      name: string;
      label: string;
    }[];
    return (
      <RqbSelect
        value={(value ?? '').toString()}
        onChange={(v) => handleOnChange(v)}
        options={flat.map((o) => ({
          value: o.name,
          label: o.label,
          disabled: used.has(o.name),
        }))}
        disabled={disabled || flat.length === 0}
        ariaLabel="筛选字段"
        className="rqb-field"
      />
    );
  };

  return (
    <div className="mt-4 flex flex-col gap-3">
      <div className="flex items-center gap-2">
        {hasFilters && (
          <button
            type="button"
            className="inline-flex items-center gap-1.5 text-xs text-ink-soft transition-colors hover:text-ink"
            title={`已添加 ${conditionCount} 个条件`}
          >
            <Filter className="size-3.5" />
            {conditionCount}
            <span>条条件</span>
          </button>
        )}
      </div>

      <div className="rqb-wrap rounded-xl border border-line bg-surface p-3">
        <QueryBuilder
          fields={fields}
          query={localQuery}
          onQueryChange={handleQueryChange}
          onAddRule={handleAddRule}
          showCombinatorsBetweenRules
          context={{
            fieldConfigs,
            query: localQuery,
            onCommit: handleQueryChange,
          }}
          controlElements={{
            fieldSelector,
            combinatorSelector: CombinatorSelector,
            operatorSelector: OperatorSelector,
            valueEditor: RuleValueEditor,
            addRuleAction: AddRuleDropdown,
            addGroupAction: AddGroupDropdown,
            removeRuleAction: RemoveAction,
            removeGroupAction: RemoveAction,
          }}
        />
        {!hasFilters && (
          <p className="mt-2 text-xs text-ink-soft">
            点「＋条件」添加筛选，或「＋组」组合分组（并且 / 或者）。
          </p>
        )}
      </div>
    </div>
  );
}
