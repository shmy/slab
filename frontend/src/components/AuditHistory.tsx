// 实体变更历史（审计）：按 entity + entityId 查询，右侧抽屉展示日志流 + 字段级 diff。
// 通用组件：任何详情入口（客户/供应商/账户…）传 entity 即可复用。
import { useInfiniteQuery } from '@tanstack/react-query';
import { ArrowRight, Loader2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
import { type AuditLogItem, apiSearchAuditLogs } from '@/lib/audit';
import { cn, formatDateTime } from '@/lib/utils';

interface AuditHistorySheetProps {
  /** 实体类型，如 'customer' / 'account' */
  entity: string;
  entityId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

// 变更类型 → 中文标签 + 徽章配色
const CHANGE_TYPE_META: Record<string, { label: string; badge: string }> = {
  create: { label: '创建', badge: 'bg-nord14/25 text-ink' },
  update: { label: '更新', badge: 'bg-nord9/25 text-ink' },
  delete: { label: '删除', badge: 'bg-nord11/25 text-ink' },
};

const DEFAULT_META = { label: '变更', badge: 'bg-nord4/40 text-ink-soft' };

/** diff 值渲染：null/undefined → 占位（before 为 null 表示新增字段） */
function formatValue(value: unknown): string {
  if (value === null || value === undefined) return '—';
  return typeof value === 'string' ? value : JSON.stringify(value);
}

export function AuditHistorySheet({
  entity,
  entityId,
  open,
  onOpenChange,
}: AuditHistorySheetProps) {
  const logsQuery = useInfiniteQuery({
    queryKey: ['audit-logs', entity, entityId],
    queryFn: ({ pageParam }) =>
      apiSearchAuditLogs({
        entity,
        entityId,
        limit: 10,
        nextCursor: pageParam,
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor,
    // 每次打开都重新请求（staleTime 0）：审计要看到最新变更，不依赖全局 30s 缓存
    staleTime: 0,
    // 抽屉打开才请求；关闭后数据保留（再打开时因 stale 自动 refetch）
    enabled: open,
  });

  const logs = logsQuery.data?.pages.flatMap((p) => p.items) ?? [];

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      {/* 审计 diff 行较长，抽屉比默认宽；覆盖需带 data-[side=right] 前缀（否则特异性输给默认类）；移动端全屏 */}
      <SheetContent className="data-[side=right]:w-full data-[side=right]:sm:max-w-xl">
        <SheetHeader>
          <SheetTitle>变更历史</SheetTitle>
          <SheetDescription>
            该实体的字段级变更记录（审计日志，按时间倒序）。
          </SheetDescription>
        </SheetHeader>
        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 pb-6">
          {logsQuery.isLoading ? (
            <p className="flex items-center gap-2 text-sm text-ink-soft">
              <Loader2 className="size-4 animate-spin" />
              加载中…
            </p>
          ) : logs.length === 0 ? (
            <p className="text-sm text-ink-soft">暂无变更记录。</p>
          ) : (
            <>
              {logs.map((log) => (
                <AuditLogRow key={log.id} log={log} />
              ))}
              {logsQuery.isFetchingNextPage ? (
                <p className="text-center text-xs text-ink-soft">加载中…</p>
              ) : logsQuery.hasNextPage ? (
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full"
                  onClick={() => void logsQuery.fetchNextPage()}
                >
                  加载更多
                </Button>
              ) : (
                <p className="text-center text-xs text-ink-soft">已加载全部</p>
              )}
            </>
          )}
        </div>
      </SheetContent>
    </Sheet>
  );
}

function AuditLogRow({ log }: { log: AuditLogItem }) {
  const meta = CHANGE_TYPE_META[log.change_type] ?? DEFAULT_META;
  return (
    <div className="rounded-lg border border-line bg-surface p-3">
      <div className="flex flex-wrap items-center gap-2">
        <Badge className={cn(meta.badge)}>{meta.label}</Badge>
        <span className="text-sm font-medium text-ink">
          {log.operator_name ?? '已删除账号'}
        </span>
        <time className="text-xs text-ink-soft">
          {formatDateTime(log.created_at)}
        </time>
      </div>
      {log.diff.length > 0 && (
        <dl className="mt-2 space-y-1 border-t border-line pt-2 font-mono text-xs">
          {log.diff.map((field) => (
            <div
              key={field.field}
              className="flex flex-wrap items-baseline gap-x-2"
            >
              <dt className="text-ink-soft">{field.field}</dt>
              <dd className="flex items-center gap-1.5 text-ink">
                <span className="text-nord11">{formatValue(field.before)}</span>
                <ArrowRight className="size-3 text-ink-soft" />
                <span className="text-nord14">{formatValue(field.after)}</span>
              </dd>
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}
