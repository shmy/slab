import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createRootRoute, Outlet } from '@tanstack/react-router';
import { KeepAliveProvider } from '@/components/keep-alive';

export const Route = createRootRoute({
  component: RootComponent,
});

// 服务端状态缓存：全局单例（staleTime 30s / 重试 1 次 / 不随窗口聚焦刷新）
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

function RootComponent() {
  return (
    <QueryClientProvider client={queryClient}>
      <KeepAliveProvider>
        <Outlet />
      </KeepAliveProvider>
    </QueryClientProvider>
  );
}
