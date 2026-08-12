// 客户管理：TanStack Query 无限滚动 + DataTable + 表单 Dialog（范式见 docs/architecture.md §4.6/4.7）
import { useForm } from '@tanstack/react-form';
import {
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { Pencil, Plus, Search, Trash2 } from 'lucide-react';
import { type FormEvent, useCallback, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { z } from 'zod';
import { type DataColumn, DataTable } from '@/components/DataTable';
import { FieldError } from '@/components/FieldError';
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
import { Input } from '@/components/ui/input';
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
import { cn } from '@/lib/utils';

export const Route = createFileRoute('/_app/customers/')({
  staticData: { keepAlive: true },
  component: CustomersPage,
});

type EditorState =
  | { mode: 'create'; customer: null }
  | { mode: 'edit'; customer: CustomerDetail };

const PAGE_SIZE = 20;
// 查询缓存键：搜索词变化即换一批数据（游标分页，累积追加）
const CUSTOMERS_KEY = ['customers'] as const;

function CustomersPage() {
  const queryClient = useQueryClient();
  const [q, setQ] = useState(''); // 输入框值
  const [query, setQuery] = useState(''); // 已提交的搜索词（queryKey 依赖）
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [deleting, setDeleting] = useState<CustomerItem | null>(null);
  const [opening, setOpening] = useState<string | null>(null); // 正在拉取详情的编辑目标 id

  // 游标分页 → 无限滚动：每页一个 pageParam（next_cursor），pages 累积追加
  const customersQuery = useInfiniteQuery({
    queryKey: [...CUSTOMERS_KEY, query],
    queryFn: ({ pageParam }) =>
      apiSearchCustomers({
        q: query || undefined,
        limit: PAGE_SIZE,
        nextCursor: pageParam,
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor,
  });

  const items = customersQuery.data?.pages.flatMap((p) => p.items) ?? [];
  const { fetchNextPage, hasNextPage, isFetchingNextPage } = customersQuery;

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
        width: 96,
        align: 'center',
        render: (c) => (
          <div className="flex items-center justify-end gap-1">
            <Button
              variant="ghost"
              size="icon"
              aria-label={`编辑 ${c.name}`}
              title="编辑"
              disabled={opening === c.id}
              className="text-ink-soft"
              onClick={() => void openEditor(c)}
            >
              <Pencil />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              aria-label={`删除 ${c.name}`}
              title="删除"
              className="text-ink-soft hover:text-destructive"
              onClick={() => setDeleting(c)}
            >
              <Trash2 />
            </Button>
          </div>
        ),
      },
    ],
    [opening, openEditor],
  );

  function submitSearch(event: FormEvent) {
    event.preventDefault();
    setQuery(q.trim());
  }

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

      {/* 搜索（提交后 queryKey 变化自动重新查询） */}
      <form className="mt-4 flex items-center gap-2" onSubmit={submitSearch}>
        <div className="relative max-w-sm flex-1">
          <Search className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-soft" />
          <Input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            className="pl-9"
            placeholder="按名称搜索客户"
            aria-label="搜索客户"
          />
        </div>
        <Button type="submit" variant="outline">
          搜索
        </Button>
      </form>

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
        // key=query：新搜索时重挂载表格（回顶 + 重置虚拟化），无需外部滚动容器
        <DataTable
          key={query}
          data={items}
          columns={columns}
          getRowId={(row) => row.id}
          // 双门控：请求中不传回调（VirtualTable 内部另有 loadingMore 门控）
          onLoadMore={
            hasNextPage && !isFetchingNextPage
              ? () => void fetchNextPage()
              : undefined
          }
          loadingMore={isFetchingNextPage}
        />
      )}

      {/* 创建 / 编辑 */}
      <Dialog
        open={editor !== null}
        onOpenChange={(open) => {
          if (!open) setEditor(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {editor?.mode === 'create'
                ? '新增客户'
                : `编辑客户「${editor?.customer.name}」`}
            </DialogTitle>
            <DialogDescription>
              带 * 为必填项；编码由系统按序列自动生成。
            </DialogDescription>
          </DialogHeader>
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
        </DialogContent>
      </Dialog>

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
    <form onSubmit={handleSubmit} className="space-y-4">
      <form.Field name="name" validators={{ onChange: nameSchema }}>
        {(field) => (
          <label htmlFor="customer-name" className="block">
            <span className="text-sm text-ink-soft">
              名称 <span className="text-destructive">*</span>
            </span>
            <Input
              id="customer-name"
              value={field.state.value}
              onBlur={field.handleBlur}
              onChange={(e) => field.handleChange(e.target.value)}
              className="mt-1"
              placeholder="客户名称"
            />
            <FieldError field={field} />
          </label>
        )}
      </form.Field>
      <form.Field name="contact_person">
        {(field) => (
          <label htmlFor="customer-contact" className="block">
            <span className="text-sm text-ink-soft">联系人</span>
            <Input
              id="customer-contact"
              value={field.state.value}
              onBlur={field.handleBlur}
              onChange={(e) => field.handleChange(e.target.value)}
              className="mt-1"
              placeholder="联系人姓名"
            />
            <FieldError field={field} />
          </label>
        )}
      </form.Field>
      <form.Field name="phone" validators={{ onChange: phoneSchema }}>
        {(field) => (
          <label htmlFor="customer-phone" className="block">
            <span className="text-sm text-ink-soft">手机号</span>
            <Input
              id="customer-phone"
              inputMode="numeric"
              value={field.state.value}
              onBlur={field.handleBlur}
              onChange={(e) => field.handleChange(e.target.value)}
              className="mt-1"
              placeholder="11 位手机号（可选）"
            />
            <FieldError field={field} />
          </label>
        )}
      </form.Field>
      <form.Field name="address">
        {(field) => (
          <label htmlFor="customer-address" className="block">
            <span className="text-sm text-ink-soft">地址</span>
            <Input
              id="customer-address"
              value={field.state.value}
              onBlur={field.handleBlur}
              onChange={(e) => field.handleChange(e.target.value)}
              className="mt-1"
              placeholder="联系地址（可选）"
            />
            <FieldError field={field} />
          </label>
        )}
      </form.Field>
      <form.Field name="payment_terms">
        {(field) => (
          <label htmlFor="customer-terms" className="block">
            <span className="text-sm text-ink-soft">结算方式</span>
            <Input
              id="customer-terms"
              value={field.state.value}
              onBlur={field.handleBlur}
              onChange={(e) => field.handleChange(e.target.value)}
              className="mt-1"
              placeholder="如：月结 30 天（可选）"
            />
            <FieldError field={field} />
          </label>
        )}
      </form.Field>
      <DialogFooter>
        <Button
          type="submit"
          disabled={saveMutation.isPending}
          className="w-full disabled:opacity-60"
        >
          {saveMutation.isPending ? '保存中…' : '保存'}
        </Button>
      </DialogFooter>
    </form>
  );
}
