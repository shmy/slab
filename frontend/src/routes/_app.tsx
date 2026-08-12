import { createFileRoute, redirect, useLocation } from '@tanstack/react-router';
import { useSelector } from '@tanstack/react-store';
import { ChevronRight, Menu, PanelLeftClose, X } from 'lucide-react';
import { Fragment, useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { FontSizeToggle } from '../components/FontSizeToggle';
import { KeepAliveOutlet } from '../components/keep-alive';
import { PageTabs } from '../components/PageTabs';
import { flatNav, navItems, SidebarNav } from '../components/SidebarNav';
import { ThemeToggle } from '../components/ThemeToggle';
import { UserMenu } from '../components/UserMenu';
import { authStore } from '../store/auth';
import { setCollapsed, sidebarStore } from '../store/sidebar';
import { addTab } from '../store/tabs';

export const Route = createFileRoute('/_app')({
  beforeLoad: ({ location }) => {
    if (!authStore.state.user) {
      // 保存来源页，登录后跳回
      // 注意：location.search 是解析后的对象，拼接需用 searchStr（带前导 ?）
      throw redirect({
        to: '/login',
        search: { redirect: location.pathname + location.searchStr },
      });
    }
  },
  component: AppLayout,
});

const today = new Date().toLocaleDateString('zh-CN', {
  year: 'numeric',
  month: 'long',
  day: 'numeric',
  weekday: 'long',
});

function getPageTitle(pathname: string) {
  return flatNav.find((item) => item.to === pathname)?.label ?? '工作台';
}

// 面包屑：按 navItems 层级推导（分组 → 子项）
function getBreadcrumbs(pathname: string): string[] {
  for (const item of navItems) {
    if ('children' in item) {
      const child = item.children.find((c) => c.to === pathname);
      if (child) return [item.label, child.label];
    } else if (item.to === pathname) {
      return [item.label];
    }
  }
  return ['工作台'];
}

function AppLayout() {
  const location = useLocation();
  // 桌面端折叠（md+）：持久化到 localStorage
  const collapsed = useSelector(sidebarStore, (s) => s.collapsed);
  // 移动端抽屉开关
  const [mobileOpen, setMobileOpen] = useState(false);

  const breadcrumbs = getBreadcrumbs(location.pathname);
  // 滚动容器是 main（非 window），scrollRestoration 管不到它：路由切换时手动回顶
  const mainRef = useRef<HTMLElement>(null);

  // 路由变化时自动打开对应标签页（去重；登录跳转/菜单点击/地址栏直达都会触发）
  useEffect(() => {
    addTab({ to: location.pathname, label: getPageTitle(location.pathname) });
    mainRef.current?.scrollTo({ top: 0 });
  }, [location.pathname]);

  return (
    <div className="flex h-screen">
      {/* 移动端抽屉遮罩：用 button 保证可聚焦/键盘可操作 */}
      {mobileOpen && (
        <button
          type="button"
          aria-label="关闭菜单"
          tabIndex={-1}
          className="fixed inset-0 z-30 bg-black/30 md:hidden"
          onClick={() => setMobileOpen(false)}
        />
      )}

      <aside
        className={cn(
          'fixed inset-y-0 left-0 z-40 flex w-56 flex-col border-r border-sidebar-line bg-sidebar transition-all duration-200',
          mobileOpen ? 'translate-x-0' : '-translate-x-full',
          'md:static md:translate-x-0',
          collapsed ? 'md:w-14' : 'md:w-56',
        )}
      >
        <div className="flex h-14 shrink-0 items-center justify-between border-b border-sidebar-line px-3">
          {(!collapsed || mobileOpen) && (
            <span className="px-1 font-semibold text-sidebar-foreground">
              Admin
            </span>
          )}
          {mobileOpen && (
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setMobileOpen(false)}
              aria-label="关闭菜单"
              className="text-sidebar-foreground hover:bg-sidebar-accent md:hidden"
            >
              <X className="h-4 w-4" />
            </Button>
          )}
        </div>

        <SidebarNav
          collapsed={collapsed}
          mobileOpen={mobileOpen}
          onNavigate={() => setMobileOpen(false)}
        />

        {/* 底部：用户区（点击弹出菜单） */}
        <div className="shrink-0 border-t border-sidebar-line p-3">
          <UserMenu
            compact={collapsed && !mobileOpen}
            onOpenProfile={() => setMobileOpen(false)}
          />
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-header-line px-4 md:px-6">
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setMobileOpen(true)}
              aria-label="打开菜单"
              className="text-ink-soft md:hidden"
            >
              <Menu className="h-5 w-5" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setCollapsed(!collapsed)}
              aria-label={collapsed ? '展开菜单' : '收起菜单'}
              title={collapsed ? '展开菜单' : '收起菜单'}
              className="hidden text-ink-soft md:inline-flex"
            >
              <PanelLeftClose
                className={cn(
                  'h-5 w-5 transition-transform',
                  collapsed && 'rotate-180',
                )}
              />
            </Button>
            {/* 面包屑：分组 → 子项，最后一级强调 */}
            <nav
              aria-label="面包屑"
              className="flex min-w-0 items-center gap-1.5 text-sm"
            >
              {getBreadcrumbs(location.pathname).map((crumb, index) => {
                const last = index === breadcrumbs.length - 1;
                return (
                  <Fragment key={crumb}>
                    {index > 0 && (
                      <ChevronRight className="h-3.5 w-3.5 shrink-0 text-ink-soft" />
                    )}
                    <span
                      className={cn(
                        'truncate',
                        last
                          ? 'font-medium text-ink'
                          : 'shrink-0 text-ink-soft',
                      )}
                    >
                      {crumb}
                    </span>
                  </Fragment>
                );
              })}
            </nav>
          </div>
          <div className="flex items-center gap-2">
            <span className="hidden text-sm text-ink-soft md:block">
              {today}
            </span>
            <FontSizeToggle />
            <ThemeToggle />
          </div>
        </header>
        {/* 多标签页栏（Chrome 风格） */}
        <PageTabs />
        {/* 内容区：flex 列布局，让页面内容可撑满剩余高度 */}
        <main
          ref={mainRef}
          className="flex min-h-0 flex-1 flex-col overflow-auto p-4 md:p-6"
        >
          <KeepAliveOutlet />
        </main>
      </div>
    </div>
  );
}
