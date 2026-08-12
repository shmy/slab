import { Outlet, useLocation, useMatches } from '@tanstack/react-router';
import {
  createContext,
  type ReactNode,
  Suspense,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

// 路由选项：staticData.keepAlive 声明该页面参与缓存（见下方 module augmentation）
declare module '@tanstack/react-router' {
  interface StaticDataRouteOption {
    keepAlive?: boolean;
  }
}

// 已缓存（keep-alive）的路由路径集合；version 用于刷新（+1 触发页面重建）
type CachedRoutes = Record<string, { version: number }>;

const KeepAliveContext = createContext<{
  cached: CachedRoutes;
  register: (pathname: string) => void;
  refresh: (pathname: string) => void;
  destroy: (pathname: string | string[]) => void;
}>({
  cached: {},
  register: () => {},
  refresh: () => {},
  destroy: () => {},
});

// 根级 Provider：持有已缓存路由集合；刷新/关闭标签页时对应操作缓存
// （refresh 重建页面、destroy 释放页面状态）
export function KeepAliveProvider({ children }: { children: ReactNode }) {
  const [cached, setCached] = useState<CachedRoutes>({});

  const register = useCallback((pathname: string) => {
    setCached((prev) =>
      prev[pathname] ? prev : { ...prev, [pathname]: { version: 0 } },
    );
  }, []);

  // 版本号 +1：CacheView 的 key 随之变化，页面卸载重建（状态重置、重新加载）
  const refresh = useCallback((pathname: string) => {
    setCached((prev) => {
      const entry = prev[pathname];
      if (!entry) return prev;
      return { ...prev, [pathname]: { ...entry, version: entry.version + 1 } };
    });
  }, []);

  const destroy = useCallback((pathname: string | string[]) => {
    const keys = Array.isArray(pathname) ? pathname : [pathname];
    setCached((prev) => {
      const next = { ...prev };
      for (const k of keys) delete next[k];
      return next;
    });
  }, []);

  const value = useMemo(
    () => ({ cached, register, refresh, destroy }),
    [cached, register, refresh, destroy],
  );

  return (
    <KeepAliveContext.Provider value={value}>
      {children}
    </KeepAliveContext.Provider>
  );
}

export function useKeepAlive() {
  return useContext(KeepAliveContext);
}

// 缓存页面的显隐容器：hidden 时挂起（不渲染 children、保留已提交的 DOM 与状态），
// visible 时恢复——Suspense 挂起是唯一"不渲染"的冻结方式（Activity 会渲染 children）
function CacheView({ active }: { active: boolean }) {
  return (
    <Suspense fallback={null}>
      <OffScreenInner mode={active ? 'visible' : 'hidden'}>
        <Outlet />
      </OffScreenInner>
    </Suspense>
  );
}

function OffScreenInner({
  mode,
  children,
}: {
  mode: 'visible' | 'hidden';
  children: ReactNode;
}) {
  // hidden 时 throw 未 resolve 的 promise 挂起：React 保留已提交的 DOM 与状态、
  // 不响应任何更新；visible 时先 resolve 唤醒（若仍挂起）再渲染 children
  const pendingRef = useRef<{
    promise: Promise<void>;
    resolve: () => void;
  } | null>(null);

  if (mode === 'hidden') {
    pendingRef.current ??= (() => {
      let resolve!: () => void;
      const promise = new Promise<void>((r) => {
        resolve = r;
      });
      return { promise, resolve };
    })();
    throw pendingRef.current.promise;
  }

  pendingRef.current?.resolve();
  pendingRef.current = null;
  return children;
}

// 替代布局中的 <Outlet />：首次访问声明 keepAlive 的页面时登记缓存，
// 之后切走/切回都复用同一棵组件树（不卸载）
export function KeepAliveOutlet() {
  const { cached, register } = useKeepAlive();
  const pathname = useLocation({ select: (l) => l.pathname });
  const matches = useMatches();
  const isKeepAlive = matches.some((m) => m.staticData?.keepAlive);
  // 用已提交 matches 的叶子路径判断激活（location 在 transition 中会先变，
  // 此时若解冻缓存页，Outlet 会读到中间态路由导致重建）
  const resolvedPathname = matches[matches.length - 1]?.pathname ?? pathname;

  useEffect(() => {
    if (isKeepAlive) register(pathname);
  }, [isKeepAlive, pathname, register]);

  return (
    <>
      {Object.entries(cached).map(([p, entry]) => (
        <CacheView
          key={`${p}:${entry.version}`}
          active={p === resolvedPathname}
        />
      ))}
      {/* 未登记（首访瞬间或非 keepAlive 路由）时直接渲染当前路由 */}
      {!cached[resolvedPathname] && <Outlet />}
    </>
  );
}
