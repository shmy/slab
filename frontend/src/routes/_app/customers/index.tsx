// 客户管理：TanStack Query 无限滚动 + DataTable + 表单 Dialog（范式见 docs/architecture.md §4.6/4.7）
// 筛选状态在 URL（validateSearch）：q（多字段模糊）+ filters（RSQL 子集串），刷新/分享/回退保留
import { useForm } from '@tanstack/react-form';
import {
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { History, Pencil, Plus, Trash2 } from 'lucide-react';
import { type FormEvent, useCallback, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { z } from 'zod';
import { AuditHistorySheet } from '@/components/AuditHistory';
import { CopyableText } from '@/components/CopyableText';
import { type DataColumn, DataTable } from '@/components/DataTable';
import { FilterBar } from '@/components/FilterBar';
import { InfoRow } from '@/components/InfoRow';
import { RowActions } from '@/components/RowActions';
import { TextField } from '@/components/TextField';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
import { ApiError } from '@/lib/api';
import {
  apiCreateCustomer,
  apiDeleteCustomer,
  apiGetCustomer,
  apiSearchCustomers,
  apiUpdateCustomer,
  type CustomerDetail,
  type CustomerItem,
} from '@/lib/customers';
import {
  type FilterFieldConfig,
  parseFilters,
  parseSorting,
  serializeFilters,
  serializeSorting,
} from '@/lib/filters';
import { cn, maskPhone } from '@/lib/utils';

// 路由 search 校验：筛选状态唯一事实源（URL）。PostgREST 风格——每个字段独立参数
// （name=ilike.*张*&created_at=gt.2024-03-15），q 为多字段搜索词，页面内转 FilterBar 条件数组
const customerSearchSchema = z.record(z.string(), z.string());

export const Route = createFileRoute('/_app/customers/')({
  staticData: { keepAlive: true },
  validateSearch: customerSearchSchema,
  component: CustomersPage,
});

type CustomerSearch = z.infer<typeof customerSearchSchema>;

// 可筛字段注册表（后端白名单：code/name/phone/contact_person + created_at；加字段 = 加一行，
// 操作符集由字段类型自动映射，见 lib/filters.ts TYPE_OPERATORS）
const FILTER_FIELDS: FilterFieldConfig[] = [
  { id: 'name', label: '名称', type: 'text', placeholder: '客户名称' },
  { id: 'code', label: '编码', type: 'text', placeholder: '客户编码' },
  { id: 'phone', label: '电话', type: 'text', placeholder: '手机号片段' },
  { id: 'created_at', label: '创建时间', type: 'date' },
];

type EditorState =
  | { mode: 'create'; customer: null }
  | { mode: 'edit'; customer: CustomerDetail };

const PAGE_SIZE = 20;
// 查询缓存键：搜索词变化即换一批数据（游标分页，累积追加）
const CUSTOMERS_KEY = ['customers'] as const;

function CustomersPage() {
  const queryClient = useQueryClient();
  // 筛选状态来自 URL（validateSearch）：q + filters；变更即导航，queryKey 自动联动
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [deleting, setDeleting] = useState<CustomerItem | null>(null);
  const [opening, setOpening] = useState<string | null>(null); // 正在拉取详情的编辑目标 id
  const [detailTarget, setDetailTarget] = useState<CustomerDetail | null>(null); // 详情抽屉内容
  const [historyTarget, setHistoryTarget] = useState<CustomerItem | null>(null);

  // 更新 URL 筛选状态（前进/后退/分享链接均可还原）
  // 更新 URL 筛选：patch 为字段参数（q / 筛选字段）；旧筛选字段不在 patch 中的删除（保持 URL 干净）
  const updateSearch = useCallback(
    (patch: Record<string, string>) => {
      navigate({
        search: (prev: CustomerSearch) => {
          const next: CustomerSearch = { ...prev };
          for (const key of Object.keys(prev)) {
            if (key !== 'q' && !(key in patch)) delete next[key];
          }
          return { ...next, ...patch };
        },
      });
    },
    [navigate],
  );

  // URL 动态字段 → FilterBar 条件数组（排除 q）；筛选参数 = 除 q 外的全部字段
  const filterConditions = useMemo(() => parseFilters(search), [search]);
  const filterParams = useMemo(() => {
    const { q: _q, ...rest } = search;
    return rest;
  }, [search]);

  // 服务端排序：URL order 串 ↔ TanStack SortingState；表头点击 → URL → 重查（游标自动重置）
  const sorting = useMemo(
    () => parseSorting(search.order ?? ''),
    [search.order],
  );
  const handleSortingChange = useCallback(
    (updater: unknown) => {
      const next =
        typeof updater === 'function'
          ? (updater as (s: typeof sorting) => typeof sorting)(sorting)
          : (updater as typeof sorting);
      updateSearch({ order: serializeSorting(next) });
    },
    [sorting, updateSearch],
  );

  // 游标分页 → 无限滚动：每页一个 pageParam（next_cursor），pages 累积追加；
  // queryKey 含 q + 筛选字段：任一变化即换一批数据
  const customersQuery = useInfiniteQuery({
    queryKey: [...CUSTOMERS_KEY, search],
    queryFn: ({ pageParam }) =>
      apiSearchCustomers({
        q: search.q || undefined,
        filters: filterParams,
        order: search.order,
        limit: PAGE_SIZE,
        nextCursor: pageParam,
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor,
  });

  const items = customersQuery.data?.pages.flatMap((p) => p.items) ?? [];
  const { fetchNextPage, hasNextPage, isFetchingNextPage } = customersQuery;

  /** 查看详情：拉取完整字段（列表项只有 id/code/name/is_active） */
  const openDetail = useCallback(async (customer: CustomerItem) => {
    setOpening(customer.id);
    try {
      const detail = await apiGetCustomer(customer.id);
      setDetailTarget(detail);
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : '加载客户详情失败',
      );
    } finally {
      setOpening(null);
    }
  }, []);

  /** 编辑前先取详情（列表项只有 id/code/name/is_active）；内部只用稳定 setter，空依赖安全 */
  const openEditor = useCallback(async (customer: CustomerItem | null) => {
    if (!customer) {
      setEditor({ mode: 'create', customer: null });
      return;
    }
    setOpening(customer.id);
    try {
      const detail = await apiGetCustomer(customer.id);
      setEditor({ mode: 'edit', customer: detail });
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : '加载客户详情失败',
      );
    } finally {
      setOpening(null);
    }
  }, []);

  function invalidateCustomers() {
    void queryClient.invalidateQueries({ queryKey: [...CUSTOMERS_KEY] });
  }

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiDeleteCustomer(id),
    onSuccess: () => {
      const target = deleting;
      setDeleting(null);
      if (target) toast.success(`已删除客户「${target.name}」`);
      invalidateCustomers();
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : '删除客户失败');
    },
  });

  // 列配置（业务声明式；render 覆盖默认值渲染；依赖 opening 使操作列禁用态跟随）
  const columns = useMemo<DataColumn<CustomerItem>[]>(
    () => [
      {
        key: 'id',
        header: 'ID',
        width: 180,
        render: (c) => (
          <CopyableText
            value={c.id}
            className="font-mono text-xs text-ink-soft"
          />
        ),
      },
      {
        key: 'code',
        header: '编码',
        width: 120,
        render: (c) => (
          <span className="font-mono text-xs text-ink-soft">{c.code}</span>
        ),
      },
      {
        key: 'name',
        header: '名称',
        width: 200,
        grow: true,
        render: (c) => <span className="font-medium text-ink">{c.name}</span>,
      },
      {
        key: 'is_active',
        header: '状态',
        width: 90,
        render: (c) => <StatusBadge active={c.is_active} />,
      },
      {
        key: 'actions',
        header: '操作',
        width: 88,
        align: 'center',
        render: (c) => (
          <RowActions
            name={c.name}
            busy={opening === c.id}
            onDetail={() => void openDetail(c)}
            items={[
              {
                key: 'edit',
                label: '编辑',
                icon: <Pencil className="h-4 w-4" />,
                onClick: () => void openEditor(c),
                disabled: opening === c.id,
              },
              {
                key: 'history',
                label: '历史',
                icon: <History className="h-4 w-4" />,
                onClick: () => setHistoryTarget(c),
              },
              {
                key: 'delete',
                label: '删除',
                icon: <Trash2 className="h-4 w-4" />,
                onClick: () => setDeleting(c),
                destructive: true,
              },
            ]}
          />
        ),
      },
    ],
    [opening, openDetail, openEditor],
  );

  return (
    // 固定高度链：页面根撑满 main，DataTable 根 flex-1 内部滚动
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold">客户管理</h1>
        <Button onClick={() => void openEditor(null)} className="gap-1.5">
          <Plus className="size-4" />
          新增客户
        </Button>
      </div>

      {/* 条件构建器：搜索框（多字段模糊，debounce 即时查询）+ ＋筛选 chips；状态在 URL（PostgREST 风格字段参数） */}
      <FilterBar
        q={search.q ?? ''}
        filters={filterConditions}
        fields={FILTER_FIELDS}
        placeholder="搜索名称/编码/电话/联系人…"
        onQChange={(q) => updateSearch({ q })}
        onFiltersChange={(conditions) =>
          updateSearch(serializeFilters(conditions))
        }
      />

      {customersQuery.isLoading ? (
        <p className="mt-4 p-6 text-sm text-ink-soft">加载中…</p>
      ) : customersQuery.isError ? (
        <div className="mt-4 rounded-xl border border-line bg-surface p-6">
          <p className="text-sm text-ink-soft">
            {customersQuery.error instanceof ApiError
              ? customersQuery.error.message
              : '加载客户列表失败'}
          </p>
          <Button
            variant="outline"
            size="sm"
            className="mt-3"
            onClick={() => void customersQuery.refetch()}
          >
            重试
          </Button>
        </div>
      ) : items.length === 0 ? (
        <p className="mt-4 p-6 text-sm text-ink-soft">
          暂无客户，点击右上角新增。
        </p>
      ) : (
        // key=筛选态序列化：条件变化时重挂载表格（回顶 + 重置虚拟化），无需外部滚动容器
        <DataTable
          key={JSON.stringify(search)}
          data={items}
          columns={columns}
          getRowId={(row) => row.id}
          selectable
          // 双门控：请求中不传回调（VirtualTable 内部另有 loadingMore 门控）
          onLoadMore={
            hasNextPage && !isFetchingNextPage
              ? () => void fetchNextPage()
              : undefined
          }
          loadingMore={isFetchingNextPage}
          sorting={sorting}
          onSortingChange={handleSortingChange}
        />
      )}

      {/* 详情抽屉 */}
      <CustomerDetailSheet
        detail={detailTarget}
        open={detailTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDetailTarget(null);
        }}
      />

      {/* 变更历史（实体级审计；entity/entityId 契约见 lib/audit.ts） */}
      <AuditHistorySheet
        entity="customer"
        entityId={historyTarget?.id ?? ''}
        open={historyTarget !== null}
        onOpenChange={(open) => {
          if (!open) setHistoryTarget(null);
        }}
      />

      {/* 创建 / 编辑（抽屉） */}
      <Sheet
        open={editor !== null}
        onOpenChange={(open) => {
          if (!open) setEditor(null);
        }}
      >
        <SheetContent className="data-[side=right]:w-full data-[side=right]:sm:max-w-xl">
          <SheetHeader>
            <SheetTitle>
              {editor?.mode === 'create'
                ? '新增客户'
                : `编辑客户「${editor?.customer.name}」`}
            </SheetTitle>
            <SheetDescription>
              带 * 为必填项；编码由系统按序列自动生成。
            </SheetDescription>
          </SheetHeader>
          {editor && (
            <CustomerEditorForm
              state={editor}
              onSaved={() => {
                setEditor(null);
                toast.success(
                  editor.mode === 'create' ? '客户已创建' : '客户已更新',
                );
              }}
            />
          )}
        </SheetContent>
      </Sheet>

      {/* 删除确认 */}
      <Dialog
        open={deleting !== null}
        onOpenChange={(open) => {
          if (!open) setDeleting(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>确认删除</DialogTitle>
            <DialogDescription>
              确定要删除客户「{deleting?.name}」吗？此操作不可撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleting(null)}>
              取消
            </Button>
            <Button
              variant="destructive"
              disabled={deleteMutation.isPending}
              onClick={() => {
                if (deleting) deleteMutation.mutate(deleting.id);
              }}
            >
              删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** 客户详情抽屉（只读；编辑在 ⋯ 菜单里） */
function CustomerDetailSheet({
  detail,
  open,
  onOpenChange,
}: {
  detail: CustomerDetail | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="data-[side=right]:w-full data-[side=right]:sm:max-w-xl">
        <SheetHeader>
          <SheetTitle>{detail?.name}</SheetTitle>
          <SheetDescription>客户详情</SheetDescription>
        </SheetHeader>
        <dl className="min-h-0 flex-1 divide-y divide-line overflow-y-auto px-4 pb-6">
          <InfoRow
            label="ID"
            value={
              detail?.id ? (
                <CopyableText value={detail.id} className="font-mono text-xs" />
              ) : undefined
            }
          />
          <InfoRow label="编码" value={detail?.code} />
          <InfoRow label="联系人" value={detail?.contact_person ?? undefined} />
          <InfoRow
            label="手机号"
            value={detail?.phone ? maskPhone(detail.phone) : undefined}
          />
          <InfoRow label="地址" value={detail?.address ?? undefined} />
          <InfoRow
            label="结算方式"
            value={detail?.payment_terms ?? undefined}
          />
          <InfoRow
            label="状态"
            value={detail ? (detail.is_active ? '启用' : '停用') : undefined}
          />
        </dl>
      </SheetContent>
    </Sheet>
  );
}

function StatusBadge({ active }: { active: boolean }) {
  return (
    <Badge
      variant={active ? 'default' : 'secondary'}
      className={cn(
        active ? 'bg-nord14/25 text-ink' : 'bg-nord4/40 text-ink-soft',
      )}
    >
      {active ? '启用' : '停用'}
    </Badge>
  );
}

// 表单校验（对齐后端契约：name 必填；phone 可选，填了须为 11 位大陆手机号）
const nameSchema = z.string().trim().min(1, '请输入客户名称');
const phoneSchema = z
  .string()
  .trim()
  .regex(/^(|1[3-9]\d{9})$/, '请输入正确的 11 位手机号');

function CustomerEditorForm({
  state,
  onSaved,
}: {
  state: EditorState;
  onSaved: () => void;
}) {
  const queryClient = useQueryClient();
  const isEdit = state.mode === 'edit';

  // 变更：成功后失效列表缓存（无限滚动保留当前页位置自动刷新）
  const saveMutation = useMutation({
    mutationFn: async (body: {
      name: string;
      contact_person?: string;
      phone?: string;
      address?: string;
      payment_terms?: string;
    }) => {
      if (isEdit) {
        await apiUpdateCustomer(state.customer.id, body);
      } else {
        await apiCreateCustomer(body);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [...CUSTOMERS_KEY] });
      onSaved();
    },
    onError: (error) => {
      toast.error(
        error instanceof ApiError ? error.message : '保存失败，请稍后重试',
      );
    },
  });

  const form = useForm({
    defaultValues: {
      // state.mode 判别窄化：edit 分支的 customer 必非空
      name: isEdit ? state.customer.name : '',
      contact_person: isEdit ? (state.customer.contact_person ?? '') : '',
      phone: isEdit ? (state.customer.phone ?? '') : '',
      address: isEdit ? (state.customer.address ?? '') : '',
      payment_terms: isEdit ? (state.customer.payment_terms ?? '') : '',
    },
    onSubmit: async ({ value }) => {
      // 空串 → undefined：不提交（后端字段可空）
      saveMutation.mutate({
        name: value.name.trim(),
        contact_person: value.contact_person.trim() || undefined,
        phone: value.phone.trim() || undefined,
        address: value.address.trim() || undefined,
        payment_terms: value.payment_terms.trim() || undefined,
      });
    },
  });

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    event.stopPropagation();
    const errors = await form.validateAllFields('change');
    if (errors.length > 0) return;
    form.handleSubmit();
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 pb-6"
    >
      <form.Field name="name" validators={{ onChange: nameSchema }}>
        {(field) => (
          <TextField
            field={field}
            id="customer-name"
            label="名称"
            required
            placeholder="客户名称"
          />
        )}
      </form.Field>
      <form.Field name="contact_person">
        {(field) => (
          <TextField
            field={field}
            id="customer-contact"
            label="联系人"
            placeholder="联系人姓名"
          />
        )}
      </form.Field>
      <form.Field name="phone" validators={{ onChange: phoneSchema }}>
        {(field) => (
          <TextField
            field={field}
            id="customer-phone"
            label="手机号"
            inputMode="numeric"
            placeholder="11 位手机号（可选）"
          />
        )}
      </form.Field>
      <form.Field name="address">
        {(field) => (
          <TextField
            field={field}
            id="customer-address"
            label="地址"
            placeholder="联系地址（可选）"
          />
        )}
      </form.Field>
      <form.Field name="payment_terms">
        {(field) => (
          <TextField
            field={field}
            id="customer-terms"
            label="结算方式"
            placeholder="如：月结 30 天（可选）"
          />
        )}
      </form.Field>
      <SheetFooter>
        <Button
          type="submit"
          disabled={saveMutation.isPending}
          className="w-full disabled:opacity-60"
        >
          {saveMutation.isPending ? '保存中…' : '保存'}
        </Button>
      </SheetFooter>
    </form>
  );
}
