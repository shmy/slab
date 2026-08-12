import { useForm } from '@tanstack/react-form';
import { createFileRoute } from '@tanstack/react-router';
import { useSelector } from '@tanstack/react-store';
import { KeyRound, ShieldCheck, UserRound } from 'lucide-react';
import { type FormEvent, type ReactNode, useState } from 'react';
import { toast } from 'sonner';
import { z } from 'zod';
import { FieldError } from '@/components/FieldError';
import { InfoRow } from '@/components/InfoRow';
import { TextField } from '@/components/TextField';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ApiError, apiUpdateMyPassword } from '@/lib/api';
import type { AuthUser } from '@/lib/token';
import { cn, maskPhone } from '@/lib/utils';
import { passwordSchema } from '@/lib/validators';
import { authStore } from '../../store/auth';

export const Route = createFileRoute('/_app/profile')({
  // keep-alive：切走时保留页面状态（含当前菜单 tab），切回不重建
  staticData: { keepAlive: true },
  component: ProfilePage,
});

type Tab = 'overview' | 'password';

const TABS = [
  { key: 'overview', label: '概览', icon: UserRound },
  { key: 'password', label: '修改密码', icon: KeyRound },
] as const;

function ProfilePage() {
  const user = useSelector(authStore, (s) => s.user);
  const [tab, setTab] = useState<Tab>('overview');
  if (!user) return null;

  return (
    <div className="mx-auto w-full max-w-4xl">
      {/* 封面横幅（渐变 + 装饰光晕） */}
      <div className="relative h-32 overflow-hidden rounded-2xl bg-gradient-to-r from-accent via-accent-soft to-accent">
        <div className="pointer-events-none absolute -top-20 -right-8 size-52 rounded-full bg-nord6/25 blur-3xl" />
        <div className="pointer-events-none absolute -bottom-28 left-1/3 size-44 rounded-full bg-nord0/25 blur-3xl" />
      </div>

      <div className="mt-2 flex flex-col gap-8 md:flex-row">
        {/* 左侧：头像 + 信息 + 菜单 */}
        <aside className="w-full shrink-0 md:w-64">
          <Avatar className="-mt-14 size-24 ring-4 ring-canvas bg-gradient-to-br from-accent to-accent-soft text-3xl text-nord6">
            <AvatarFallback>{user.name.charAt(0).toUpperCase()}</AvatarFallback>
          </Avatar>
          <h1 className="mt-3 text-2xl font-bold text-ink">{user.name}</h1>
          <p className="mt-0.5 text-sm text-ink-soft">
            {maskPhone(user.phone)}
          </p>
          <Badge
            className={cn(
              'mt-2',
              user.privileged
                ? 'bg-accent text-nord6'
                : 'bg-surface text-ink-soft border-line',
            )}
          >
            {user.privileged ? '管理员' : '普通用户'}
          </Badge>

          <nav className="mt-6 space-y-1">
            {TABS.map(({ key, label, icon: Icon }) => {
              const active = tab === key;
              return (
                <button
                  key={key}
                  type="button"
                  onClick={() => setTab(key)}
                  className={cn(
                    'group relative flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-sm transition-colors',
                    active
                      ? 'bg-accent-soft/15 font-medium text-ink'
                      : 'text-ink-soft hover:bg-surface hover:text-ink',
                  )}
                >
                  {/* active 左侧指示条 */}
                  <span
                    className={cn(
                      'absolute top-1/2 left-0 h-5 w-0.5 -translate-y-1/2 rounded-full bg-accent transition-opacity',
                      active ? 'opacity-100' : 'opacity-0',
                    )}
                  />
                  <span
                    className={cn(
                      'flex size-8 shrink-0 items-center justify-center rounded-lg transition-colors',
                      active
                        ? 'bg-accent text-nord6'
                        : 'bg-surface text-ink-soft group-hover:text-ink',
                    )}
                  >
                    <Icon className="h-4 w-4" />
                  </span>
                  {label}
                </button>
              );
            })}
          </nav>
        </aside>

        {/* 右侧：tab 内容 */}
        <div className="min-w-0 flex-1">
          {tab === 'overview' ? <OverviewCard user={user} /> : <PasswordCard />}
        </div>
      </div>
    </div>
  );
}

/** 卡片容器：圆角 + 柔和阴影 + 图标化 header */
function Card({
  icon,
  title,
  description,
  children,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div className="rounded-2xl border border-line bg-surface shadow-sm">
      <div className="flex items-center gap-3 border-b border-line px-6 py-4">
        <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-accent-soft/15 text-accent">
          {icon}
        </span>
        <div>
          <h2 className="text-base font-semibold text-ink">{title}</h2>
          <p className="text-xs text-ink-soft">{description}</p>
        </div>
      </div>
      <div className="p-6">{children}</div>
    </div>
  );
}

function OverviewCard({ user }: { user: AuthUser }) {
  const { name, phone, privileged } = user;
  return (
    <Card
      icon={<UserRound className="h-4 w-4" />}
      title="概览"
      description="账号基本信息"
    >
      <dl className="divide-y divide-line">
        <InfoRow label="姓名" value={name} />
        <InfoRow label="手机号" value={maskPhone(phone)} />
        <InfoRow
          label="角色"
          value={privileged ? '管理员' : '普通用户'}
          valueClassName={privileged ? 'text-accent' : undefined}
        />
      </dl>
      <p className="mt-5 text-xs text-ink-soft">
        更多资料（头像、邮箱等）后续版本开放。
      </p>
    </Card>
  );
}

/** 修改自己的密码（后端不吊销令牌，改密后会话继续有效） */
const oldPasswordSchema = z.string().min(1, '请输入当前密码');

function PasswordCard() {
  const form = useForm({
    defaultValues: {
      oldPassword: '',
      newPassword: '',
      confirmPassword: '',
    },
    onSubmit: async ({ value }) => {
      try {
        // form 不保留 schema 的 transform 输出，提交时自行 trim
        await apiUpdateMyPassword(value.oldPassword, value.newPassword.trim());
        toast.success('密码已更新');
        form.reset();
      } catch (error) {
        toast.error(
          error instanceof ApiError
            ? error.message
            : '密码更新失败，请稍后重试',
        );
      }
    },
  });

  // 提交兜底：validateAllFields('change') 强制校验所有字段（自动标 touched，错误可见）
  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    event.stopPropagation();
    const errors = await form.validateAllFields('change');
    if (errors.length > 0) return;
    form.handleSubmit();
  }

  return (
    <Card
      icon={<ShieldCheck className="h-4 w-4" />}
      title="修改密码"
      description="更新后当前会话继续有效，下次登录使用新密码"
    >
      <form onSubmit={handleSubmit} className="space-y-4">
        <form.Field
          name="oldPassword"
          validators={{ onChange: oldPasswordSchema }}
        >
          {(field) => (
            <TextField
              field={field}
              id="old-password"
              label="当前密码"
              type="password"
              autoComplete="current-password"
              placeholder="请输入当前密码"
            />
          )}
        </form.Field>
        <form.Field
          name="newPassword"
          validators={{ onChange: passwordSchema }}
        >
          {(field) => (
            <TextField
              field={field}
              id="new-password"
              label="新密码"
              type="password"
              autoComplete="new-password"
              placeholder="至少 4 位"
            />
          )}
        </form.Field>
        <form.Field
          name="confirmPassword"
          validators={{
            // cross-field：与新密码一致性校验（TanStack Form 字段级函数 validator）
            onChange: ({ value, fieldApi }) =>
              value !== fieldApi.form.state.values.newPassword
                ? '两次输入的新密码不一致'
                : undefined,
          }}
        >
          {(field) => (
            <label htmlFor="confirm-password" className="block">
              <span className="text-sm text-ink-soft">确认新密码</span>
              <Input
                id="confirm-password"
                type="password"
                autoComplete="new-password"
                value={field.state.value}
                onBlur={field.handleBlur}
                onChange={(e) => field.handleChange(e.target.value)}
                className="mt-1 bg-surface"
                placeholder="再次输入新密码"
              />
              <FieldError field={field} />
            </label>
          )}
        </form.Field>
        <Button
          type="submit"
          disabled={form.state.isSubmitting}
          className="w-full disabled:opacity-60"
        >
          {form.state.isSubmitting ? '提交中…' : '更新密码'}
        </Button>
      </form>
    </Card>
  );
}
