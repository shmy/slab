import { createFileRoute } from '@tanstack/react-router';

export const Route = createFileRoute('/_app/settings/permissions')({
  // keep-alive：切走时保留页面状态，切回不重建
  staticData: { keepAlive: true },
  component: PermissionsPage,
});

function PermissionsPage() {
  return (
    <div>
      <h1 className="text-xl font-semibold">权限管理</h1>
      <p className="mt-4 text-sm text-ink-soft">
        这里是权限管理占位内容，待接入后端。
      </p>
    </div>
  );
}
