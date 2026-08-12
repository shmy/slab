import { useForm } from '@tanstack/react-form';
import { createFileRoute, redirect } from '@tanstack/react-router';
import { Eye, EyeOff, LayoutDashboard, Lock, User } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { toast } from 'sonner';
import { z } from 'zod';
import { FieldError } from '@/components/FieldError';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ApiError } from '../lib/api';
import { passwordSchema } from '../lib/validators';
import { authStore, login } from '../store/auth';

// 校验 redirect 目标，防止开放重定向（open redirect）
function sanitizeRedirect(url: unknown): string {
  if (typeof url !== 'string' || !url.startsWith('/') || url.startsWith('//')) {
    return '/';
  }
  return url;
}

export const Route = createFileRoute('/login')({
  validateSearch: (search) => ({
    redirect: sanitizeRedirect(search.redirect),
  }),
  beforeLoad: ({ search }) => {
    if (authStore.state.user) {
      throw redirect({ to: search.redirect });
    }
  },
  component: LoginPage,
});

// Standard Schema（Zod 4 原生兼容）：onChange 实时反馈 + 提交时 validateAllFields('change') 兜底。
// 注意：form 不保留 schema 的 transform 输出，onSubmit 里仍需自行 trim
// 手机号规则与后端 PhoneNumber 一致（11 位大陆手机号 1[3-9]xxxxxxxxx）；密码 4–64 位
const phoneSchema = z
  .string()
  .trim()
  .regex(/^1[3-9]\d{9}$/, '请输入正确的 11 位手机号');

function LoginPage() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const [showPassword, setShowPassword] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const form = useForm({
    defaultValues: {
      phone: '',
      password: '',
    },
    onSubmit: async ({ value }) => {
      try {
        await login(value.phone.trim(), value.password.trim());
        navigate({ to: search.redirect });
      } catch (error) {
        // 后端 Problem Details：ApiError.message 已是 detail/title 的展示文本
        toast.error(
          error instanceof ApiError ? error.message : '登录失败，请稍后重试',
        );
      }
    },
  });

  // 提交兜底：handleSubmit 只跑 submit validators（字段未配），未触碰直接提交会被放行；
  // 这里用 onChange 规则强制校验所有字段（validateAllFields 会自动标 touched，错误可见）
  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    event.stopPropagation();
    const errors = await form.validateAllFields('change');
    if (errors.length > 0) return;
    setSubmitting(true);
    try {
      await form.handleSubmit();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="flex min-h-screen bg-canvas">
      {/* 左侧品牌区：永远深色（与侧边栏同色系），移动端隐藏 */}
      <div className="relative hidden w-[42%] flex-col justify-between overflow-hidden bg-sidebar p-10 lg:flex">
        {/* 装饰光晕 */}
        <div className="pointer-events-none absolute -top-32 -left-32 size-72 rounded-full bg-accent/20 blur-3xl" />
        <div className="pointer-events-none absolute -right-24 -bottom-24 size-64 rounded-full bg-accent-soft/10 blur-3xl" />

        <div className="relative flex items-center gap-2">
          <div className="flex size-8 items-center justify-center rounded-lg bg-accent text-nord6">
            <LayoutDashboard className="h-4 w-4" />
          </div>
          <span className="text-lg font-semibold text-nord6">Admin</span>
        </div>

        <div className="relative">
          <h1 className="text-3xl font-semibold text-nord6">欢迎回来</h1>
          <p className="mt-3 max-w-xs text-sm leading-relaxed text-nord4">
            登录管理后台，统一管理系统资源、用户与权限。
          </p>
        </div>

        <p className="relative text-xs text-nord4/60">
          © 2026 Admin · 管理后台
        </p>
      </div>

      {/* 右侧表单区 */}
      <div className="flex flex-1 items-center justify-center p-6">
        <div className="w-full max-w-sm">
          {/* 移动端品牌标识 */}
          <div className="mb-8 flex items-center justify-center gap-2 lg:hidden">
            <div className="flex size-8 items-center justify-center rounded-lg bg-accent text-nord6">
              <LayoutDashboard className="h-4 w-4" />
            </div>
            <span className="text-lg font-semibold">Admin</span>
          </div>

          <form
            onSubmit={handleSubmit}
            className="rounded-xl border border-line bg-surface p-8 shadow-sm"
          >
            <h2 className="text-xl font-semibold">登录</h2>
            <p className="mt-1 text-xs text-ink-soft">
              使用注册手机号登录管理后台
            </p>
            <div className="mt-6 space-y-4">
              <form.Field name="phone" validators={{ onChange: phoneSchema }}>
                {(field) => (
                  <label htmlFor="phone" className="block">
                    <span className="text-sm text-ink-soft">手机号</span>
                    <div className="relative mt-1">
                      <User className="absolute top-1/2 left-2.5 h-4 w-4 -translate-y-1/2 text-ink-soft" />
                      <Input
                        id="phone"
                        name="phone"
                        autoComplete="tel"
                        inputMode="numeric"
                        value={field.state.value}
                        onBlur={field.handleBlur}
                        onChange={(e) => field.handleChange(e.target.value)}
                        className="bg-surface pl-8"
                        placeholder="请输入手机号"
                      />
                    </div>
                    {/* 触碰过才显示错误 */}
                    <FieldError field={field} />
                  </label>
                )}
              </form.Field>
              <form.Field
                name="password"
                validators={{ onChange: passwordSchema }}
              >
                {(field) => (
                  <label htmlFor="password" className="block">
                    <span className="text-sm text-ink-soft">密码</span>
                    <div className="relative mt-1">
                      <Lock className="absolute top-1/2 left-2.5 h-4 w-4 -translate-y-1/2 text-ink-soft" />
                      <Input
                        id="password"
                        name="password"
                        autoComplete="current-password"
                        type={showPassword ? 'text' : 'password'}
                        value={field.state.value}
                        onBlur={field.handleBlur}
                        onChange={(e) => field.handleChange(e.target.value)}
                        className="bg-surface pr-9 pl-8"
                        placeholder="请输入密码"
                      />
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        aria-label={showPassword ? '隐藏密码' : '显示密码'}
                        // 用 top 定位（不用 -translate-y-1/2，避免与 active 位移冲突）
                        className="absolute top-1 right-1 h-6 w-6 text-ink-soft hover:bg-transparent"
                        onClick={() => setShowPassword((v) => !v)}
                      >
                        {showPassword ? (
                          <EyeOff className="h-4 w-4" />
                        ) : (
                          <Eye className="h-4 w-4" />
                        )}
                      </Button>
                    </div>
                    <FieldError field={field} />
                  </label>
                )}
              </form.Field>
            </div>
            <Button
              type="submit"
              disabled={submitting}
              className="mt-6 w-full disabled:opacity-60"
            >
              {submitting ? '登录中…' : '登录'}
            </Button>
          </form>
        </div>
      </div>
    </div>
  );
}
