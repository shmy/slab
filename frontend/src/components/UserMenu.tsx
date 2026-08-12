import { useNavigate } from '@tanstack/react-router';
import { useSelector } from '@tanstack/react-store';
import { ChevronsUpDown, CircleUserRound, LogOut } from 'lucide-react';
import { useState } from 'react';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import { authStore, logout } from '../store/auth';

// 菜单项统一样式：加大行距与图标间距，避免过于紧凑
const itemClass = 'gap-2 px-2 py-2';

interface UserMenuProps {
  /** 侧边栏是否处于紧凑模式（只显示头像） */
  compact: boolean;
  /** 打开个人信息页前的回调（如关闭移动端抽屉） */
  onOpenProfile?: () => void;
}

export function UserMenu({ compact, onOpenProfile }: UserMenuProps) {
  const user = useSelector(authStore, (s) => s.user);
  const navigate = useNavigate();
  // 退出确认对话框
  const [confirmLogout, setConfirmLogout] = useState(false);

  function handleLogout() {
    setConfirmLogout(false);
    logout();
    navigate({ to: '/login' });
  }

  function handleOpenProfile() {
    onOpenProfile?.();
    navigate({ to: '/profile' });
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label="用户菜单"
        className={cn(
          'flex w-full items-center gap-3 rounded-md p-1 text-left outline-none transition-colors select-none hover:bg-sidebar-hover focus-visible:bg-sidebar-hover',
          compact && 'justify-center',
        )}
      >
        <Avatar className="shrink-0 bg-accent text-nord6">
          <AvatarFallback>
            {user?.username.charAt(0).toUpperCase()}
          </AvatarFallback>
        </Avatar>
        {!compact && (
          <>
            <span className="flex-1 truncate text-sm text-nord5">
              {user?.username}
            </span>
            <ChevronsUpDown className="h-4 w-4 shrink-0 text-nord4" />
          </>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        side="top"
        sideOffset={8}
        className="w-44 p-1.5"
      >
        <DropdownMenuItem onClick={handleOpenProfile} className={itemClass}>
          <CircleUserRound className="h-4 w-4" />
          个人信息
        </DropdownMenuItem>
        <DropdownMenuSeparator className="my-1.5" />
        <DropdownMenuItem
          variant="destructive"
          onClick={() => setConfirmLogout(true)}
          className={itemClass}
        >
          <LogOut className="h-4 w-4" />
          退出登录
        </DropdownMenuItem>
      </DropdownMenuContent>

      {/* 退出确认 */}
      <Dialog
        open={confirmLogout}
        onOpenChange={(open) => {
          if (!open) setConfirmLogout(false);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>确认退出登录</DialogTitle>
            <DialogDescription>
              确定要退出当前账号「{user?.username}」吗？
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmLogout(false)}>
              取消
            </Button>
            <Button variant="destructive" onClick={handleLogout}>
              退出登录
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </DropdownMenu>
  );
}
