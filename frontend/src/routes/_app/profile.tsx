import { createFileRoute } from '@tanstack/react-router';
import { useSelector } from '@tanstack/react-store';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { authStore } from '../../store/auth';

export const Route = createFileRoute('/_app/profile')({
  // keep-alive：切走时保留页面状态，切回不重建
  staticData: { keepAlive: true },
  component: ProfilePage,
});

function ProfilePage() {
  const user = useSelector(authStore, (s) => s.user);
  if (!user) return null;

  return (
    <div className="mx-auto w-full max-w-md">
      <h1 className="text-xl font-semibold">个人信息</h1>
      <div className="mt-4 rounded-xl border border-line bg-surface p-6">
        <div className="flex items-center gap-4">
          <Avatar className="h-16 w-16 bg-accent text-2xl text-nord6">
            <AvatarFallback>
              {user.username.charAt(0).toUpperCase()}
            </AvatarFallback>
          </Avatar>
          <div className="min-w-0">
            <div className="truncate text-lg font-semibold text-ink">
              {user.username}
            </div>
            <div className="text-sm text-ink-soft">当前登录账号</div>
          </div>
        </div>
        <dl className="mt-6 divide-y divide-line border-t border-line">
          <div className="flex items-center justify-between py-3">
            <dt className="text-sm text-ink-soft">用户名</dt>
            <dd className="text-sm text-ink">{user.username}</dd>
          </div>
          <div className="flex items-center justify-between py-3">
            <dt className="text-sm text-ink-soft">角色</dt>
            <dd className="text-sm text-ink">管理员</dd>
          </div>
        </dl>
        <p className="mt-4 text-xs text-ink-soft">
          更多资料（头像、邮箱等）待接入后端后展示。
        </p>
      </div>
    </div>
  );
}
