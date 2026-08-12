// React Compiler 豁免：HeaderCheckbox / 列定义 render 函数依赖 useTable 的 render-phase store（详见 docs/architecture.md §5.8）
'use no memo';
import { createFileRoute } from '@tanstack/react-router';
import {
  columnFilteringFeature,
  columnPinningFeature,
  columnSizingFeature,
  createColumnHelper,
  createFilteredRowModel,
  createSortedRowModel,
  filterFn_includesString,
  globalFilteringFeature,
  rowSelectionFeature,
  rowSortingFeature,
  type Table,
  tableFeatures,
} from '@tanstack/react-table';
import { Pencil, Search, Trash2 } from 'lucide-react';
import { useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
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
import { VirtualTable } from '../../components/VirtualTable';

export const Route = createFileRoute('/_app/users')({
  // keep-alive：切走时保留页面状态，切回不重建
  staticData: { keepAlive: true },
  component: UsersPage,
});

interface User {
  id: number;
  name: string;
  email: string;
  role: string;
  status: 'active' | 'disabled';
}

const ROLES = ['管理员', '编辑', '访客'] as const;
const SURNAMES = ['张', '李', '王', '赵', '刘', '陈', '杨', '黄'];
const GIVEN = ['伟', '芳', '娜', '敏', '静', '磊', '军', '洋'];

function generateUsers(count: number, startId = 1): User[] {
  return Array.from({ length: count }, (_, i) => {
    const id = startId + i;
    return {
      id,
      name: SURNAMES[id % SURNAMES.length] + GIVEN[(id * 7) % GIVEN.length],
      email: `user${id}@example.com`,
      role: ROLES[id % ROLES.length],
      status: id % 3 === 0 ? 'disabled' : 'active',
    };
  });
}

// 只注册用到的 feature：过滤 + 排序 + 固定列（pinning 前置依赖 sizing）
const features = tableFeatures({
  columnFilteringFeature,
  columnSizingFeature,
  columnPinningFeature,
  globalFilteringFeature,
  rowSelectionFeature,
  rowSortingFeature,
  filteredRowModel: createFilteredRowModel(),
  sortedRowModel: createSortedRowModel(),
  // 只注册列会用到的内置过滤函数
  filterFns: { includesString: filterFn_includesString },
  // 列级自定义配置的类型声明（表头/单元格对齐）
  columnMeta: {} as { align?: 'left' | 'center' | 'right' },
});

const columnHelper = createColumnHelper<typeof features, User>();

// 可编辑字段（User 的字符串字段）
type EditFieldKey = 'name' | 'email' | 'role';

const EDIT_FIELDS: ReadonlyArray<{
  key: EditFieldKey;
  label: string;
  type?: string;
}> = [
  { key: 'name', label: '姓名' },
  { key: 'email', label: '邮箱', type: 'email' },
  { key: 'role', label: '角色' },
];

function patchUser(user: User, key: EditFieldKey, value: string): User {
  return { ...user, [key]: value } as User;
}

// 表头全选：半选（indeterminate）态需要 ref + effect
function HeaderCheckbox({ table }: { table: Table<typeof features, User> }) {
  const all = table.getIsAllRowsSelected();
  const some = table.getIsSomeRowsSelected();
  return (
    <Checkbox
      checked={all}
      indeterminate={some && !all}
      // 不依赖事件 checked（受控组件下时序不可靠），按当前状态显式切换
      onCheckedChange={() =>
        table.toggleAllRowsSelected(!table.getIsAllRowsSelected())
      }
      aria-label="全选"
    />
  );
}

function UsersPage() {
  const [users, setUsers] = useState(() => generateUsers(500));
  const [editing, setEditing] = useState<User | null>(null);
  const [deleting, setDeleting] = useState<User | null>(null);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const loadingRef = useRef(false);

  // 列定义：cell 闭包需要访问编辑/删除 setter（setter 引用稳定，useMemo 空依赖安全）
  const columns = useMemo(
    () =>
      columnHelper.columns([
        // 选择列：display 列，固定在 start 最前；全选 = 已加载行（无限滚动下新加载行不自动选中）
        columnHelper.display({
          id: 'select',
          size: 40,
          meta: { align: 'center' },
          header: ({ table }) => <HeaderCheckbox table={table} />,
          cell: ({ row }) => (
            <Checkbox
              checked={row.getIsSelected()}
              // 显式切换：不依赖受控组件下的事件 checked
              onCheckedChange={() => row.toggleSelected(!row.getIsSelected())}
              aria-label="选择该行"
            />
          ),
        }),
        columnHelper.accessor('id', { header: 'ID', size: 64 }),
        columnHelper.accessor('name', { header: '姓名', size: 140 }),
        // email 列通过 growColumnId 吸收剩余空间撑满容器
        columnHelper.accessor('email', { header: '邮箱', size: 220 }),
        columnHelper.accessor('role', { header: '角色', size: 100 }),
        columnHelper.accessor('status', {
          header: '状态',
          size: 100,
          cell: (info) => {
            const active = info.getValue() === 'active';
            return (
              <Badge
                variant={active ? 'default' : 'secondary'}
                className={
                  active ? 'bg-nord14/25 text-ink' : 'bg-nord4/40 text-ink-soft'
                }
              >
                {active ? '启用' : '禁用'}
              </Badge>
            );
          },
        }),
        columnHelper.display({
          id: 'actions',
          header: '操作',
          size: 96,
          meta: { align: 'center' },
          cell: ({ row }) => {
            const user = row.original;
            return (
              <div className="flex items-center gap-1">
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={`编辑 ${user.name}`}
                  title="编辑"
                  className="text-ink-soft"
                  onClick={() => setEditing(user)}
                >
                  <Pencil />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={`删除 ${user.name}`}
                  title="删除"
                  className="text-ink-soft hover:text-destructive"
                  onClick={() => setDeleting(user)}
                >
                  <Trash2 />
                </Button>
              </div>
            );
          },
        }),
      ]),
    [],
  );

  // 无限滚动：接近底部时追加 100 行（模拟分页接口）
  function loadMore() {
    if (loadingRef.current) return;
    loadingRef.current = true;
    setIsLoadingMore(true);
    setTimeout(() => {
      setUsers((prev) => [...prev, ...generateUsers(100, prev.length + 1)]);
      loadingRef.current = false;
      setIsLoadingMore(false);
    }, 400);
  }

  function saveEdit() {
    if (!editing) return;
    setUsers((prev) => prev.map((u) => (u.id === editing.id ? editing : u)));
    setEditing(null);
    toast.success(`已保存「${editing.name}」的修改`);
  }

  function confirmDelete() {
    if (!deleting) return;
    setUsers((prev) => prev.filter((u) => u.id !== deleting.id));
    setDeleting(null);
    toast.success(`已删除用户「${deleting.name}」`);
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <h1 className="text-xl font-semibold">用户管理</h1>
      <VirtualTable
        features={features}
        columns={columns}
        data={users}
        initialState={{
          columnPinning: { start: ['select', 'id'], end: ['actions'] },
        }}
        getRowId={(row) => String(row.id)}
        growColumnId="email"
        onLoadMore={loadMore}
        loadingMore={isLoadingMore}
        toolbar={(table) => (
          <div className="mt-4 flex flex-wrap items-center gap-3">
            <div className="relative">
              <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-soft" />
              <Input
                value={table.state.globalFilter ?? ''}
                onChange={(e) => table.setGlobalFilter(e.target.value)}
                placeholder="全局搜索：姓名 / 邮箱 / 角色 / 状态"
                className="w-72 bg-surface pl-8"
              />
            </div>
            <span className="text-xs text-ink-soft">
              {table.getRowModel().rows.length} 行
            </span>
            <span className="text-xs text-ink-soft">
              {table.getSelectedRowIds().length > 0
                ? `已选 ${table.getSelectedRowIds().length} 项 · `
                : ''}
              ID / 操作列已固定 · 滚动到底自动加载更多
            </span>
          </div>
        )}
      />

      {/* 编辑用户：抽屉表单 */}
      <Sheet
        open={editing !== null}
        onOpenChange={(open) => {
          if (!open) setEditing(null);
        }}
      >
        <SheetContent>
          <SheetHeader>
            <SheetTitle>编辑用户</SheetTitle>
            <SheetDescription>修改「{editing?.name}」的资料</SheetDescription>
          </SheetHeader>
          <form
            className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4"
            onSubmit={(e) => {
              e.preventDefault();
              saveEdit();
            }}
          >
            {EDIT_FIELDS.map((field) => (
              <label
                key={field.key}
                htmlFor={`edit-${field.key}`}
                className="block"
              >
                <span className="text-sm text-ink-soft">{field.label}</span>
                <Input
                  id={`edit-${field.key}`}
                  type={field.type}
                  value={editing?.[field.key] ?? ''}
                  onChange={(e) =>
                    setEditing((u) =>
                      u ? patchUser(u, field.key, e.target.value) : u,
                    )
                  }
                  className="mt-1 bg-surface"
                />
              </label>
            ))}
          </form>
          <SheetFooter>
            <Button variant="outline" onClick={() => setEditing(null)}>
              取消
            </Button>
            <Button type="submit" onClick={saveEdit}>
              保存
            </Button>
          </SheetFooter>
        </SheetContent>
      </Sheet>

      {/* 删除确认：对话框 */}
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
            <Button variant="destructive" onClick={confirmDelete}>
              删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
