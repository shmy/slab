// 用户管理：账号 CRUD + 重置密码（范式见 docs/architecture.md §4.6/4.7）
import { useForm } from '@tanstack/react-form';
import {
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { History, KeyRound, Pencil, Plus, Search, Trash2 } from 'lucide-react';
import { type FormEvent, useCallback, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { z } from 'zod';
import { AuditHistorySheet } from '@/components/AuditHistory';
import { CopyableText } from '@/components/CopyableText';
import { type DataColumn, DataTable } from '@/components/DataTable';
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
import { Input } from '@/components/ui/input';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
import {
  type AccountDetail,
  type AccountItem,
  apiCreateAccount,
  apiDeleteAccount,
  apiGetAccount,
  apiSearchAccounts,
  apiUpdateAccount,
} from '@/lib/accounts';
import { ApiError, apiResetAccountPassword } from '@/lib/api';
import { cn } from '@/lib/utils';
import { passwordSchema } from '@/lib/validators';

export const Route = createFileRoute('/_app/settings/users')({
  staticData: { keepAlive: true },
  component: UsersPage,
});

type EditorState =
  | { mode: 'create'; account: null }
  | { mode: 'edit'; account: AccountDetail };

const PAGE_SIZE = 20;
const ACCOUNTS_KEY = ['accounts'] as const;

function UsersPage() {
  const queryClient = useQueryClient();
  const [q, setQ] = useState('');
  const [query, setQuery] = useState('');
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [deleting, setDeleting] = useState<AccountItem | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const [detailTarget, setDetailTarget] = useState<AccountDetail | null>(null);
  const [historyTarget, setHistoryTarget] = useState<AccountItem | null>(null);
  const [resetTarget, setResetTarget] = useState<AccountItem | null>(null);

  const accountsQuery = useInfiniteQuery({
    queryKey: [...ACCOUNTS_KEY, query],
    queryFn: ({ pageParam }) =>
      apiSearchAccounts({
        q: query || undefined,
        limit: PAGE_SIZE,
        nextCursor: pageParam,
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor,
  });

  const items = accountsQuery.data?.pages.flatMap((p) => p.items) ?? [];
  const { fetchNextPage, hasNextPage, isFetchingNextPage } = accountsQuery;

  const openDetail = useCallback(async (account: AccountItem) => {
    setOpening(account.id);
    try {
      const detail = await apiGetAccount(account.id);
      setDetailTarget(detail);
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : '加载用户详情失败',
      );
    } finally {
      setOpening(null);
    }
  }, []);

  const openEditor = useCallback(async (account: AccountItem | null) => {
    if (!account) {
      setEditor({ mode: 'create', account: null });
      return;
    }
    setOpening(account.id);
    try {
      const detail = await apiGetAccount(account.id);
      setEditor({ mode: 'edit', account: detail });
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : '加载用户详情失败',
      );
    } finally {
      setOpening(null);
    }
  }, []);

  function invalidateAccounts() {
    void queryClient.invalidateQueries({ queryKey: [...ACCOUNTS_KEY] });
  }

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiDeleteAccount(id),
    onSuccess: () => {
      const target = deleting;
      setDeleting(null);
      if (target) toast.success(`已删除用户「${target.name}」`);
      invalidateAccounts();
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : '删除用户失败');
    },
  });

  const columns = useMemo<DataColumn<AccountItem>[]>(
    () => [
      {
        key: 'id',
        header: 'ID',
        width: 200,
        render: (a) => (
          <CopyableText
            value={a.id}
            className="font-mono text-xs text-ink-soft"
          />
        ),
      },
      {
        key: 'name',
        header: '姓名',
        width: 160,
        grow: true,
        render: (a) => <span className="font-medium text-ink">{a.name}</span>,
      },
      {
        key: 'phone',
        header: '手机号',
        width: 140,
        render: (a) => <span className="text-ink">{a.phone}</span>,
      },
      {
        key: 'privileged',
        header: '特权',
        width: 100,
        render: (a) => <RoleBadge privileged={a.privileged} />,
      },
      {
        key: 'actions',
        header: '操作',
        width: 88,
        align: 'center',
        render: (a) => (
          <RowActions
            name={a.name}
            busy={opening === a.id}
            onDetail={() => void openDetail(a)}
            items={[
              {
                key: 'edit',
                label: '编辑',
                icon: <Pencil className="h-4 w-4" />,
                onClick: () => void openEditor(a),
                disabled: opening === a.id || a.privileged,
                title: a.privileged ? '特权账号不可编辑' : undefined,
              },
              {
                key: 'reset',
                label: '重置密码',
                icon: <KeyRound className="h-4 w-4" />,
                onClick: () => setResetTarget(a),
                disabled: a.privileged,
                title: a.privileged ? '特权账号不可重置密码' : undefined,
              },
              {
                key: 'history',
                label: '历史',
                icon: <History className="h-4 w-4" />,
                onClick: () => setHistoryTarget(a),
              },
              {
                key: 'delete',
                label: '删除',
                icon: <Trash2 className="h-4 w-4" />,
                onClick: () => setDeleting(a),
                disabled: a.privileged,
                title: a.privileged ? '特权账号不可删除' : undefined,
                destructive: true,
              },
            ]}
          />
        ),
      },
    ],
    [opening, openDetail, openEditor],
  );

  function submitSearch(event: FormEvent) {
    event.preventDefault();
    setQuery(q.trim());
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold">用户管理</h1>
        <Button onClick={() => void openEditor(null)} className="gap-1.5">
          <Plus className="size-4" />
          新增用户
        </Button>
      </div>

      <form className="mt-4 flex items-center gap-2" onSubmit={submitSearch}>
        <div className="relative max-w-sm flex-1">
          <Search className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-soft" />
          <Input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            className="pl-9"
            placeholder="按姓名/手机号搜索用户"
            aria-label="搜索用户"
          />
        </div>
        <Button type="submit" variant="outline">
          搜索
        </Button>
      </form>

      {accountsQuery.isLoading ? (
        <p className="mt-4 p-6 text-sm text-ink-soft">加载中…</p>
      ) : accountsQuery.isError ? (
        <div className="mt-4 rounded-xl border border-line bg-surface p-6">
          <p className="text-sm text-ink-soft">
            {accountsQuery.error instanceof ApiError
              ? accountsQuery.error.message
              : '加载用户列表失败'}
          </p>
          <Button
            variant="outline"
            size="sm"
            className="mt-3"
            onClick={() => void accountsQuery.refetch()}
          >
            重试
          </Button>
        </div>
      ) : items.length === 0 ? (
        <p className="mt-4 p-6 text-sm text-ink-soft">
          暂无用户，点击右上角新增。
        </p>
      ) : (
        <DataTable
          key={query}
          data={items}
          columns={columns}
          getRowId={(row) => row.id}
          selectable
          onLoadMore={
            hasNextPage && !isFetchingNextPage
              ? () => void fetchNextPage()
              : undefined
          }
          loadingMore={isFetchingNextPage}
        />
      )}

      {/* 详情抽屉 */}
      <AccountDetailSheet
        detail={detailTarget}
        open={detailTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDetailTarget(null);
        }}
      />

      {/* 历史（实体级审计） */}
      <AuditHistorySheet
        entity="account"
        entityId={historyTarget?.id ?? ''}
        open={historyTarget !== null}
        onOpenChange={(open) => {
          if (!open) setHistoryTarget(null);
        }}
      />

      {/* 创建 / 编辑抽屉 */}
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
                ? '新增用户'
                : `编辑用户「${editor?.account.name}」`}
            </SheetTitle>
            <SheetDescription>
              带 * 为必填项；特权账号不可修改、删除或重置密码。
            </SheetDescription>
          </SheetHeader>
          {editor && (
            <AccountEditorForm
              state={editor}
              onSaved={() => {
                setEditor(null);
                toast.success(
                  editor.mode === 'create' ? '用户已创建' : '用户已更新',
                );
              }}
            />
          )}
        </SheetContent>
      </Sheet>

      {/* 重置密码 */}
      <ResetPasswordDialog
        target={resetTarget}
        onClose={() => setResetTarget(null)}
        onDone={() => {
          setResetTarget(null);
          toast.success(`已重置「${resetTarget?.name}」的密码`);
        }}
      />

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
              确定要删除用户「{deleting?.name}」吗？此操作不可撤销。
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

function RoleBadge({ privileged }: { privileged: boolean }) {
  return (
    <Badge
      className={cn(
        privileged
          ? 'bg-accent text-nord6'
          : 'bg-surface text-ink-soft border-line',
      )}
    >
      {privileged ? '是' : '否'}
    </Badge>
  );
}

/** 用户详情抽屉（只读；编辑/重置密码在 ⋯ 菜单里） */
function AccountDetailSheet({
  detail,
  open,
  onOpenChange,
}: {
  detail: AccountDetail | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="data-[side=right]:w-full data-[side=right]:sm:max-w-xl">
        <SheetHeader>
          <SheetTitle>{detail?.name}</SheetTitle>
          <SheetDescription>用户详情</SheetDescription>
        </SheetHeader>
        <dl className="min-h-0 flex-1 divide-y divide-line overflow-y-auto px-4 pb-6">
          <InfoRow
            label="ID"
            value={
              detail ? (
                <CopyableText value={detail.id} className="font-mono text-xs" />
              ) : undefined
            }
          />
          <InfoRow label="姓名" value={detail?.name} />
          <InfoRow label="手机号" value={detail?.phone} />
          <InfoRow
            label="特权"
            value={detail ? (detail.privileged ? '是' : '否') : undefined}
          />
        </dl>
      </SheetContent>
    </Sheet>
  );
}

// 表单校验（对齐后端契约：name/phone 必填；创建时 password 4–64 位）
const nameSchema = z.string().trim().min(1, '请输入姓名');
const phoneSchema = z
  .string()
  .trim()
  .regex(/^1[3-9]\d{9}$/, '请输入正确的 11 位手机号');

function AccountEditorForm({
  state,
  onSaved,
}: {
  state: EditorState;
  onSaved: () => void;
}) {
  const queryClient = useQueryClient();
  const isEdit = state.mode === 'edit';
  // 判别窄化提取：update 分支提交时必非空（onSubmit 只发对应 kind）
  const accountId = isEdit ? state.account.id : null;

  // 创建/更新 payload 分型：create 带 password，update 不带（无需断言）
  const saveMutation = useMutation({
    mutationFn: (
      payload:
        | { kind: 'create'; name: string; phone: string; password: string }
        | { kind: 'update'; name: string; phone: string },
    ) =>
      payload.kind === 'create'
        ? apiCreateAccount(payload)
        : apiUpdateAccount(accountId ?? '', payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [...ACCOUNTS_KEY] });
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
      name: isEdit ? state.account.name : '',
      phone: isEdit ? state.account.phone : '',
      password: '',
    },
    onSubmit: async ({ value }) => {
      const base = { name: value.name.trim(), phone: value.phone.trim() };
      if (isEdit) {
        saveMutation.mutate({ kind: 'update', ...base });
      } else {
        saveMutation.mutate({
          kind: 'create',
          ...base,
          password: value.password.trim(),
        });
      }
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
            id="account-name"
            label="姓名"
            required
            placeholder="用户姓名"
          />
        )}
      </form.Field>
      <form.Field name="phone" validators={{ onChange: phoneSchema }}>
        {(field) => (
          <TextField
            field={field}
            id="account-phone"
            label="手机号"
            required
            inputMode="numeric"
            placeholder="11 位手机号"
          />
        )}
      </form.Field>
      {!isEdit && (
        <form.Field name="password" validators={{ onChange: passwordSchema }}>
          {(field) => (
            <TextField
              field={field}
              id="account-password"
              label="初始密码"
              required
              type="password"
              autoComplete="new-password"
              placeholder="至少 4 位"
            />
          )}
        </form.Field>
      )}
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

/** 重置密码：为指定用户设置新密码（PATCH /accounts/password/{id}） */
function ResetPasswordDialog({
  target,
  onClose,
  onDone,
}: {
  target: AccountItem | null;
  onClose: () => void;
  onDone: () => void;
}) {
  const queryClient = useQueryClient();

  const resetMutation = useMutation({
    mutationFn: ({ id, newPassword }: { id: string; newPassword: string }) =>
      apiResetAccountPassword(id, newPassword),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [...ACCOUNTS_KEY] });
      onDone();
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : '重置密码失败');
    },
  });

  const form = useForm({
    defaultValues: { password: '' },
    onSubmit: async ({ value }) => {
      // 对话框只在 target 非空时打开，提交必然有目标（构造期判空，不依赖 ! 断言）
      if (!target) return;
      resetMutation.mutate({
        id: target.id,
        newPassword: value.password.trim(),
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
    <Dialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>重置密码</DialogTitle>
          <DialogDescription>
            为「{target?.name}」设置新密码（4–64 位）。
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="space-y-4">
          <form.Field name="password" validators={{ onChange: passwordSchema }}>
            {(field) => (
              <TextField
                field={field}
                id="reset-password"
                label="新密码"
                required
                type="password"
                autoComplete="new-password"
                placeholder="至少 4 位"
              />
            )}
          </form.Field>
          <DialogFooter>
            <Button
              type="submit"
              disabled={resetMutation.isPending}
              className="w-full disabled:opacity-60"
            >
              {resetMutation.isPending ? '提交中…' : '确认重置'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
