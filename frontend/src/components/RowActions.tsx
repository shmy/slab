// 表格行操作：查看详情（直接显示）+ ⋯ 更多菜单（菜单项由业务方声明数组）。
// 两页（customers/users）共用；destructive 项自动红色 + 前置分隔线。
import { Eye, MoreHorizontal } from 'lucide-react';
import { Fragment, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';

export interface RowActionItem {
  key: string;
  label: string;
  icon: ReactNode;
  onClick: () => void;
  /** 红色警示 + 前加分隔线 */
  destructive?: boolean;
  disabled?: boolean;
  /** 禁用原因提示 */
  title?: string;
}

export function RowActions({
  name,
  busy,
  onDetail,
  items,
}: {
  /** 实体名（a11y 文案用） */
  name: string;
  /** 正在拉取详情（详情按钮禁用） */
  busy: boolean;
  onDetail: () => void;
  items: RowActionItem[];
}) {
  return (
    <div className="flex items-center justify-end gap-1">
      <Button
        variant="ghost"
        size="icon"
        aria-label={`查看 ${name} 详情`}
        title="查看详情"
        disabled={busy}
        className="text-ink-soft"
        onClick={onDetail}
      >
        <Eye />
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon"
              aria-label={`${name} 的更多操作`}
              title="更多"
              className="text-ink-soft"
            />
          }
        >
          <MoreHorizontal />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-40 p-1.5">
          {items.map((item, index) => (
            <Fragment key={item.key}>
              {item.destructive && index > 0 && (
                <DropdownMenuSeparator className="my-1.5" />
              )}
              <DropdownMenuItem
                variant={item.destructive ? 'destructive' : 'default'}
                onClick={item.onClick}
                disabled={item.disabled}
                title={item.title}
                className="gap-2"
              >
                {item.icon}
                {item.label}
              </DropdownMenuItem>
            </Fragment>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
