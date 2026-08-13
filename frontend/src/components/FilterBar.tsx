// 条件构建器（GitHub Issues 式）：主搜索框（多字段模糊）+ 「＋筛选」字段条件 chips。
// 控件数量恒定 = 一个搜索框 + 一个下拉，任意字段组合通过 chips 累积，可单删/全清。
// 受控组件：q / filters 由父组件持有（路由 search params），本组件只做 UI 与 debounce。
import { Filter, Search, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

export interface FilterFieldConfig {
  id: string;
  label: string;
  type: 'text' | 'date';
  operators: { id: string; label: string }[];
  placeholder?: string;
}

export interface FilterCondition {
  field: string;
  op: string;
  value: string;
}

interface FilterBarProps {
  /** 已生效搜索词（URL 状态）；输入框本地值 debounce 300ms 后回调 */
  q: string;
  filters: FilterCondition[];
  fields: FilterFieldConfig[];
  placeholder?: string;
  onQChange: (q: string) => void;
  onFiltersChange: (filters: FilterCondition[]) => void;
}

const DEBOUNCE_MS = 300;

export function FilterBar({
  q,
  filters,
  fields,
  placeholder,
  onQChange,
  onFiltersChange,
}: FilterBarProps) {
  // z.record search 缺失键时为 undefined，防御兜底
  const [localQ, setLocalQ] = useState(q ?? '');
  // URL 外部变化（前进/后退、分享链接直达）时同步输入框；
  // 同步引发的 localQ 变化会重跑下方 debounce effect 并 cleanup 掉旧 timer，旧值不会误提交
  useEffect(() => setLocalQ(q ?? ''), [q]);
  // 输入防抖：停止输入 300ms 后提交；q 同步后 localQ===q 直接跳过
  useEffect(() => {
    if (localQ.trim() === q) return;
    const timer = setTimeout(() => onQChange(localQ.trim()), DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [localQ, q, onQChange]);

  const [pickerOpen, setPickerOpen] = useState(false);
  // draft：新增（editingIndex=null）或编辑已有条件（editingIndex=index，保存时替换）
  const [draft, setDraft] = useState<{
    field: FilterFieldConfig;
    op: string;
    value: string;
    editingIndex: number | null;
  } | null>(null);

  function addCondition() {
    if (!draft?.value.trim()) return;
    const next = {
      field: draft.field.id,
      op: draft.op,
      value: draft.value.trim(),
    };
    onFiltersChange(
      draft.editingIndex === null
        ? [...filters, next]
        : filters.map((f, i) => (i === draft.editingIndex ? next : f)),
    );
    setDraft(null);
    setPickerOpen(false);
  }

  /** 编辑已有条件：打开下拉并预填当前值 */
  function editCondition(index: number) {
    const f = filters[index];
    const field = fields.find((x) => x.id === f.field);
    if (!field) return; // URL 手改的未知字段无法编辑
    setDraft({ field, op: f.op, value: f.value, editingIndex: index });
    setPickerOpen(true);
  }

  function removeCondition(index: number) {
    onFiltersChange(filters.filter((_, i) => i !== index));
  }

  function fieldLabel(id: string) {
    return fields.find((f) => f.id === id)?.label ?? id;
  }

  function opLabel(fieldId: string, opId: string) {
    return (
      fields.find((f) => f.id === fieldId)?.operators.find((o) => o.id === opId)
        ?.label ?? opId
    );
  }

  return (
    <div className="mt-4">
      <div className="flex items-center gap-2">
        <div className="relative max-w-sm flex-1">
          <Search className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-soft" />
          <Input
            value={localQ}
            onChange={(e) => setLocalQ(e.target.value)}
            className="pr-9 pl-9"
            placeholder={placeholder ?? '搜索…'}
            aria-label="搜索"
          />
          {/* 一键清空：立即生效（绕过 debounce），同时同步输入框 */}
          {localQ !== '' && (
            <button
              type="button"
              aria-label="清空搜索"
              className="absolute top-1/2 right-2.5 flex size-4 -translate-y-1/2 items-center justify-center rounded-full text-ink-soft transition-colors hover:bg-muted hover:text-ink"
              onClick={() => {
                setLocalQ('');
                onQChange('');
              }}
            >
              <X className="size-3" />
            </button>
          )}
        </div>
        <DropdownMenu
          open={pickerOpen}
          onOpenChange={(open) => {
            setPickerOpen(open);
            // 关闭时重置编辑态：下次打开回到字段列表（避免残留 chip 编辑视图）
            if (!open) setDraft(null);
          }}
        >
          <DropdownMenuTrigger
            render={
              <Button
                variant="outline"
                className="gap-1.5"
                title={
                  filters.length > 0
                    ? `已添加 ${filters.length} 个筛选条件`
                    : '添加筛选条件'
                }
              >
                <Filter className="size-3.5" />
                筛选
                {/* 条件计数徽标 */}
                {filters.length > 0 && (
                  <span className="flex size-4 items-center justify-center rounded-full bg-primary text-[10px] font-medium text-primary-foreground">
                    {filters.length}
                  </span>
                )}
              </Button>
            }
          />
          <DropdownMenuContent align="start" className="w-56">
            {draft ? (
              // 编辑态：操作符 + 值 + 确认（一行内完成，不铺控件）
              // 标题用普通文本：DropdownMenuLabel 是 Menu.Group 的 label part，裸用会缺 GroupContext
              <>
                <p className="px-2 pt-1.5 pb-1 text-xs font-medium text-muted-foreground">
                  {draft.field.label}
                </p>
                <div className="flex flex-wrap gap-1 px-1.5 pb-1">
                  {draft.field.operators.map((op) => (
                    <button
                      key={op.id}
                      type="button"
                      onClick={() => setDraft({ ...draft, op: op.id })}
                      className={cn(
                        'rounded-full px-2 py-0.5 text-xs transition-colors',
                        draft.op === op.id
                          ? 'bg-primary text-primary-foreground'
                          : 'bg-muted text-ink-soft hover:bg-muted',
                      )}
                    >
                      {op.label}
                    </button>
                  ))}
                </div>
                <div className="px-1.5 pb-1.5">
                  <Input
                    autoFocus
                    type={draft.field.type === 'date' ? 'date' : 'text'}
                    value={draft.value}
                    onChange={(e) =>
                      setDraft({ ...draft, value: e.target.value })
                    }
                    onKeyDown={(e) => {
                      // 阻断冒泡：Menu 的 typeahead 会 stopEvent 所有单字符按键
                      // （不检查焦点元素），不拦截的话英文/数字会被吞掉，只剩 IME 中文能输入
                      e.stopPropagation();
                      if (e.key === 'Enter') addCondition();
                      if (e.key === 'Escape') setDraft(null);
                    }}
                    placeholder={draft.field.placeholder}
                  />
                </div>
                <div className="flex justify-end gap-1.5 px-1.5 pb-1.5">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setDraft(null)}
                  >
                    取消
                  </Button>
                  <Button size="sm" onClick={addCondition}>
                    {draft.editingIndex === null ? '添加' : '保存'}
                  </Button>
                </div>
              </>
            ) : (
              // 字段列表：已用字段置灰，其余可点进编辑态
              fields.map((field) => {
                const used = filters.some((f) => f.field === field.id);
                return (
                  <DropdownMenuItem
                    key={field.id}
                    disabled={used}
                    // 点击字段不关闭菜单：原地切换到编辑态（base-ui 默认 closeOnClick=true）
                    closeOnClick={false}
                    onClick={() =>
                      setDraft({
                        field,
                        op: field.operators[0]?.id ?? '',
                        value: '',
                        editingIndex: null,
                      })
                    }
                  >
                    {field.label}
                    {used && (
                      <span className="ml-auto text-xs text-muted-foreground">
                        已筛
                      </span>
                    )}
                  </DropdownMenuItem>
                );
              })
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* 生效条件 chips：可视化 + 单删 + 全清 */}
      {filters.length > 0 && (
        <div className="mt-2 flex flex-wrap items-center gap-1.5">
          {filters.map((f, index) => (
            // 同字段条件禁止重复添加（下拉里已筛字段置灰），key=field 天然唯一
            <span
              key={f.field}
              className="inline-flex items-center gap-0.5 rounded-full bg-muted py-0.5 pr-1 pl-1 text-xs text-ink"
            >
              <button
                type="button"
                title="点击编辑条件"
                className="rounded-full py-0.5 pl-1.5 transition-colors hover:text-ink"
                onClick={() => editCondition(index)}
              >
                {fieldLabel(f.field)}：{opLabel(f.field, f.op)}{' '}
                {/* 值高亮：扫读时一眼定位条件值 */}
                <span className="font-semibold text-primary">{f.value}</span>
              </button>
              <button
                type="button"
                aria-label={`移除条件 ${fieldLabel(f.field)}`}
                className="flex size-4 items-center justify-center rounded-full text-ink-soft transition-colors hover:bg-muted hover:text-ink"
                onClick={() => removeCondition(index)}
              >
                <X className="size-3" />
              </button>
            </span>
          ))}
          <button
            type="button"
            className="text-xs text-ink-soft transition-colors hover:text-ink"
            onClick={() => onFiltersChange([])}
          >
            清除全部
          </button>
        </div>
      )}
    </div>
  );
}
