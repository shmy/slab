import { Link, useLocation } from '@tanstack/react-router';
import type { LucideIcon } from 'lucide-react';
import {
  Building2,
  ChevronDown,
  FileText,
  FolderTree,
  LayoutDashboard,
  Newspaper,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  Users,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';

// 导航数据：普通项（to）或分组（children，点击展开 submenu）
interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

interface NavGroup {
  label: string;
  icon: LucideIcon;
  children: NavItem[];
}

export const navItems: (NavItem | NavGroup)[] = [
  { to: '/', label: '仪表盘', icon: LayoutDashboard },
  { to: '/customers', label: '客户管理', icon: Building2 },
  {
    label: '内容管理',
    icon: FileText,
    children: [
      { to: '/content/articles', label: '文章管理', icon: Newspaper },
      { to: '/content/categories', label: '分类管理', icon: FolderTree },
    ],
  },
  {
    label: '系统设置',
    icon: Settings,
    children: [
      { to: '/settings/users', label: '用户管理', icon: Users },
      { to: '/settings/general', label: '通用设置', icon: SlidersHorizontal },
      { to: '/settings/permissions', label: '权限管理', icon: ShieldCheck },
    ],
  },
];

// 扁平化：pageTitle 查找需要遍历到子项
export const flatNav = navItems.flatMap((item) =>
  'children' in item ? item.children : [item],
);

// 键盘焦点可见性：统一的 focus 环（颜色随主题）
const focusRing = 'focus-visible:ring-2 focus-visible:ring-ring/70';

interface SidebarNavProps {
  /** 侧边栏是否折叠（只显示图标） */
  collapsed: boolean;
  /** 移动端抽屉是否打开（打开时忽略 collapsed） */
  mobileOpen: boolean;
  /** 导航后回调（如关闭移动端抽屉） */
  onNavigate?: () => void;
}

export function SidebarNav({
  collapsed,
  mobileOpen,
  onNavigate,
}: SidebarNavProps) {
  const location = useLocation();
  // 展开的分组（默认展开当前路由所在分组）
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(
    () =>
      new Set(
        navItems
          .filter(
            (item) =>
              'children' in item &&
              item.children.some((child) => child.to === location.pathname),
          )
          .map((item) => item.label),
      ),
  );

  // 路由变化时自动展开所在分组（如收起态 popup 里导航后，展开侧边栏能看到激活项）
  useEffect(() => {
    const group = navItems.find(
      (item) =>
        'children' in item &&
        item.children.some((child) => child.to === location.pathname),
    );
    if (!group || !('children' in group)) return;
    setExpandedGroups((prev) => {
      if (prev.has(group.label)) return prev;
      return new Set(prev).add(group.label);
    });
  }, [location.pathname]);

  function toggleGroup(label: string) {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(label)) {
        next.delete(label);
      } else {
        next.add(label);
      }
      return next;
    });
  }

  // 分组按钮样式（非 Link，无激活态与 hover 冲突问题，颜色可合并）
  const groupClass = cn(
    'flex w-full items-center rounded-md py-2 text-sm text-sidebar-foreground hover:bg-sidebar-accent',
    focusRing,
    collapsed && !mobileOpen ? 'justify-center px-0' : 'gap-3 px-2.5',
  );

  return (
    <nav className="flex-1 space-y-1 overflow-y-auto p-3">
      {navItems.map((item) => {
        if (!('children' in item)) {
          return (
            <Link
              key={item.to}
              to={item.to}
              title={item.label}
              onClick={onNavigate}
              className={cn(
                'flex items-center rounded-md py-2 text-sm',
                focusRing,
                collapsed && !mobileOpen
                  ? 'justify-center px-0'
                  : 'gap-3 px-2.5',
              )}
              // 激活态：Filament 风格浅底胶囊（bg-sidebar-primary + 深蓝文字）
              activeProps={{
                className: 'bg-sidebar-primary text-sidebar-primary-foreground',
              }}
              inactiveProps={{
                className: 'text-sidebar-foreground hover:bg-sidebar-accent',
              }}
            >
              <item.icon className="h-4 w-4 shrink-0" />
              {(!collapsed || mobileOpen) && (
                <span className="truncate">{item.label}</span>
              )}
            </Link>
          );
        }

        const groupExpanded = expandedGroups.has(item.label);
        // 折叠态下 submenu 不可见（0fr），也不可交互
        const groupOpen = groupExpanded && (!collapsed || mobileOpen);
        // 子项激活时，分组按钮同步高亮（收起态 trigger 也提示"这里有激活项"）
        const groupActive = item.children.some(
          (child) => child.to === location.pathname,
        );

        // 折叠态：hover 弹出浮层子列表（portal，不受 nav overflow 裁剪）
        if (collapsed && !mobileOpen) {
          return (
            <DropdownMenu key={item.label}>
              <DropdownMenuTrigger
                openOnHover
                // 去掉默认 100ms 的 hover 打开延迟，响应更跟手
                delay={0}
                render={
                  <button
                    type="button"
                    title={item.label}
                    className={cn(
                      groupClass,
                      groupActive &&
                        'bg-sidebar-primary text-sidebar-primary-foreground hover:bg-sidebar-primary',
                    )}
                  >
                    <item.icon className="h-4 w-4 shrink-0" />
                  </button>
                }
              />
              <DropdownMenuContent
                align="start"
                side="right"
                sideOffset={12}
                // 直角 + 侧边栏同款背景（随主题），无边框无阴影
                className="w-44 rounded-none bg-sidebar p-1.5 text-sidebar-foreground shadow-none ring-0"
              >
                <DropdownMenuGroup>
                  <DropdownMenuLabel className="px-2 py-1.5 text-sidebar-foreground">
                    {item.label}
                  </DropdownMenuLabel>
                </DropdownMenuGroup>
                {item.children.map((child) => {
                  const active = child.to === location.pathname;
                  return (
                    <DropdownMenuItem
                      key={child.to}
                      render={<Link to={child.to} onClick={onNavigate} />}
                      className={cn(
                        'gap-2 px-2 py-2',
                        // 导航项用小手；hover 只变背景，文字/图标色不变（激活项保持品牌色）
                        'cursor-pointer focus:bg-sidebar-accent focus:text-sidebar-accent-foreground',
                        focusRing,
                        active &&
                          'bg-sidebar-primary text-sidebar-primary-foreground focus:bg-sidebar-primary focus:text-sidebar-primary-foreground',
                      )}
                    >
                      <child.icon className="h-4 w-4" />
                      {child.label}
                    </DropdownMenuItem>
                  );
                })}
              </DropdownMenuContent>
            </DropdownMenu>
          );
        }

        return (
          <div key={item.label}>
            <button
              type="button"
              title={item.label}
              aria-expanded={groupExpanded}
              onClick={() => toggleGroup(item.label)}
              className={cn(
                groupClass,
                // 子项激活时分组高亮，hover 保持激活底色
                groupActive &&
                  'bg-sidebar-primary text-sidebar-primary-foreground hover:bg-sidebar-primary',
              )}
            >
              <item.icon className="h-4 w-4 shrink-0" />
              {(!collapsed || mobileOpen) && (
                <>
                  <span className="flex-1 truncate text-left">
                    {item.label}
                  </span>
                  <ChevronDown
                    className={cn(
                      'h-4 w-4 shrink-0 transition-transform duration-200',
                      // 收起时朝右（逆时针 90°），展开时朝下
                      groupExpanded ? 'rotate-0' : '-rotate-90',
                    )}
                  />
                </>
              )}
            </button>
            {/* submenu：grid-rows 0fr↔1fr 过渡实现展开/收起动画（高度自适应） */}
            <div
              className={cn(
                'grid transition-[grid-template-rows] duration-200 ease-in-out',
                groupOpen ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]',
              )}
            >
              <div
                className="min-h-0 overflow-hidden"
                aria-hidden={!groupOpen}
                // 收起时阻止键盘聚焦到隐藏的子项
                inert={!groupOpen}
              >
                <div className="mt-1 ml-4 space-y-1 border-l border-sidebar-line pl-2">
                  {item.children.map((child) => (
                    <Link
                      key={child.to}
                      to={child.to}
                      title={child.label}
                      onClick={onNavigate}
                      className={cn(
                        'flex items-center gap-3 rounded-md px-2.5 py-2 text-sm',
                        focusRing,
                      )}
                      activeProps={{
                        className:
                          'bg-sidebar-primary text-sidebar-primary-foreground',
                      }}
                      // hover 只变背景：图标/文字保持前景色不变
                      inactiveProps={{
                        className:
                          'text-sidebar-foreground hover:bg-sidebar-accent',
                      }}
                    >
                      <child.icon className="h-4 w-4 shrink-0" />
                      <span className="truncate">{child.label}</span>
                    </Link>
                  ))}
                </div>
              </div>
            </div>
          </div>
        );
      })}
    </nav>
  );
}
