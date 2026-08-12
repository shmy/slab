import { useLocation, useNavigate } from '@tanstack/react-router';
import { useSelector } from '@tanstack/react-store';
import { ChevronDown, CircleX, Layers, RefreshCw, X } from 'lucide-react';
import { Fragment, useEffect, useRef } from 'react';
import { useKeepAlive } from '@/components/keep-alive';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import { removeTab, tabsStore } from '../store/tabs';

// 首页固定，不可关闭
const HOME = '/';

// 标签操作菜单项配置：右键菜单与右端操作菜单共用
// 刷新是独立操作，与关闭类操作用分隔线隔开
const TAB_REFRESH_ACTIONS = [
  { key: 'refresh', label: '刷新', icon: RefreshCw },
] as const;

const TAB_CLOSE_ACTIONS = [
  { key: 'close', label: '关闭当前', icon: X },
  { key: 'close-others', label: '关闭其他', icon: CircleX },
  { key: 'close-all', label: '关闭全部', icon: Layers },
] as const;

type TabActionKey =
  | (typeof TAB_REFRESH_ACTIONS)[number]['key']
  | (typeof TAB_CLOSE_ACTIONS)[number]['key'];

// 执行菜单操作：刷新指定标签 / 关闭指定标签 / 关闭其他 / 关闭全部
function runTabAction(
  action: TabActionKey,
  to: string,
  refresh: (to: string) => void,
  closeTab: (to: string) => void,
  closeOthers: (to: string) => void,
  closeAll: () => void,
) {
  if (action === 'refresh') refresh(to);
  else if (action === 'close') closeTab(to);
  else if (action === 'close-others') closeOthers(to);
  else closeAll();
}

export function PageTabs() {
  const tabs = useSelector(tabsStore, (s) => s.tabs);
  const location = useLocation();
  const navigate = useNavigate();
  const scrollerRef = useRef<HTMLDivElement>(null);
  // 关闭标签时同步销毁对应页面的 keep-alive 缓存（页面状态随之释放）
  const { destroy, refresh } = useKeepAlive();

  // 激活标签变化后，确保其滚入可视区
  // scrollIntoView 需在布局稳定后调用（commit 阶段布局可能未刷新，rAF 后必然稳定）
  useEffect(() => {
    const raf = requestAnimationFrame(() => {
      const scroller = scrollerRef.current;
      if (!scroller) return;
      scroller
        .querySelector<HTMLElement>(`[data-path="${location.pathname}"]`)
        ?.scrollIntoView({
          behavior: 'smooth',
          inline: 'nearest',
          block: 'nearest',
        });
    });
    return () => cancelAnimationFrame(raf);
  }, [location.pathname]);

  function closeTab(to: string) {
    const index = tabs.findIndex((t) => t.to === to);
    // 关闭的是激活标签：先跳到相邻标签（右侧优先，没有则左侧）；
    // 若首页不在标签列表且无相邻，回首页兜底
    if (to === location.pathname) {
      const next = tabs[index + 1] ?? tabs[index - 1];
      navigate({ to: next ? next.to : '/' });
    }
    destroy(to);
    removeTab(to);
  }

  // 关闭其他：只保留目标标签和首页；若当前页被关则跳到目标标签
  function closeOthers(to: string) {
    const keep = new Set([to, HOME]);
    const closed = tabs.filter((t) => !keep.has(t.to)).map((t) => t.to);
    tabsStore.setState((s) => ({
      tabs: s.tabs.filter((t) => keep.has(t.to)),
    }));
    destroy(closed);
    if (!keep.has(location.pathname)) navigate({ to });
  }

  // 关闭全部：只保留首页；若当前页不是首页则回首页
  function closeAll() {
    const closed = tabs.filter((t) => t.to !== HOME).map((t) => t.to);
    tabsStore.setState((s) => ({
      tabs: s.tabs.filter((t) => t.to === HOME),
    }));
    destroy(closed);
    if (location.pathname !== HOME) navigate({ to: HOME });
  }

  return (
    <div className="flex shrink-0 items-end border-b border-header-line bg-stripe">
      {/* 标签滚动区：flex-1 占满剩余空间，按钮固定在右端 */}
      <div
        ref={scrollerRef}
        className="flex min-w-0 flex-1 items-end gap-0.5 overflow-x-auto px-2 pt-1.5 scrollbar-none"
      >
        {tabs.map((tab) => {
          const active = tab.to === location.pathname;
          const closable = tab.to !== HOME;
          // 标签本体（首页固定不弹右键菜单）
          const tabEl = (
            <div
              key={tab.to}
              data-path={tab.to}
              className={cn(
                'group flex h-8 shrink-0 items-center gap-1 rounded-t-md border border-b-0 px-2.5 text-sm transition-colors',
                // 激活标签：白/亮卡片（surface）与标签栏底色形成对比，底部无边框与页面相连
                active
                  ? 'border-line bg-surface text-ink'
                  : 'border-transparent bg-transparent text-ink-soft hover:bg-stripe-hover hover:text-ink',
              )}
            >
              <button
                type="button"
                onClick={() => {
                  // 点击已激活标签不重复导航
                  if (!active) navigate({ to: tab.to });
                }}
                className="max-w-40 truncate"
              >
                {tab.label}
              </button>
              {closable && (
                <button
                  type="button"
                  aria-label={`关闭 ${tab.label}`}
                  onClick={() => closeTab(tab.to)}
                  // Chrome 风格：非激活标签 hover 时显示关闭按钮，激活标签常显
                  className={cn(
                    'flex size-4 shrink-0 items-center justify-center rounded-sm text-ink-soft transition-opacity hover:bg-muted hover:text-ink focus-visible:ring-2 focus-visible:ring-ring/70',
                    active
                      ? 'opacity-100'
                      : 'opacity-0 group-hover:opacity-100',
                  )}
                >
                  <X className="h-3 w-3" />
                </button>
              )}
            </div>
          );

          return closable ? (
            <ContextMenu key={tab.to}>
              <ContextMenuTrigger render={<div className="block" />}>
                {tabEl}
              </ContextMenuTrigger>
              <ContextMenuContent className="w-36">
                {TAB_REFRESH_ACTIONS.map((action) => (
                  <ContextMenuItem
                    key={action.key}
                    onClick={() =>
                      runTabAction(
                        action.key,
                        tab.to,
                        refresh,
                        closeTab,
                        closeOthers,
                        closeAll,
                      )
                    }
                  >
                    <action.icon className="h-4 w-4" />
                    {action.label}
                  </ContextMenuItem>
                ))}
                <ContextMenuSeparator />
                {TAB_CLOSE_ACTIONS.map((action, index) => (
                  <Fragment key={action.key}>
                    {index > 0 && <ContextMenuSeparator />}
                    <ContextMenuItem
                      disabled={action.key === 'close' && !closable}
                      onClick={() =>
                        runTabAction(
                          action.key,
                          tab.to,
                          refresh,
                          closeTab,
                          closeOthers,
                          closeAll,
                        )
                      }
                    >
                      <action.icon className="h-4 w-4" />
                      {action.label}
                    </ContextMenuItem>
                  </Fragment>
                ))}
              </ContextMenuContent>
            </ContextMenu>
          ) : (
            tabEl
          );
        })}
      </div>

      {/* 标签操作菜单（固定右端，不随标签滚动） */}
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon"
              aria-label="标签操作"
              // 上/右留白，底部与标签对齐
              className="mt-1.5 mr-1.5 h-8 w-8 shrink-0 text-ink-soft"
            >
              <ChevronDown className="h-4 w-4" />
            </Button>
          }
        />
        <DropdownMenuContent align="end" className="w-36">
          {TAB_REFRESH_ACTIONS.map((action) => (
            <DropdownMenuItem
              key={action.key}
              onClick={() =>
                runTabAction(
                  action.key,
                  location.pathname,
                  refresh,
                  closeTab,
                  closeOthers,
                  closeAll,
                )
              }
            >
              <action.icon className="h-4 w-4" />
              {action.label}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          {TAB_CLOSE_ACTIONS.map((action, index) => (
            <Fragment key={action.key}>
              {index > 0 && <DropdownMenuSeparator />}
              <DropdownMenuItem
                disabled={action.key === 'close' && location.pathname === HOME}
                onClick={() =>
                  runTabAction(
                    action.key,
                    location.pathname,
                    refresh,
                    closeTab,
                    closeOthers,
                    closeAll,
                  )
                }
              >
                <action.icon className="h-4 w-4" />
                {action.label}
              </DropdownMenuItem>
            </Fragment>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
