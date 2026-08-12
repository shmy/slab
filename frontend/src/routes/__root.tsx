import { createRootRoute, Outlet } from '@tanstack/react-router';
import { KeepAliveProvider } from '@/components/keep-alive';

export const Route = createRootRoute({
  component: RootComponent,
});

function RootComponent() {
  return (
    <KeepAliveProvider>
      <Outlet />
    </KeepAliveProvider>
  );
}
