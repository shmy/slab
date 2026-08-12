import { createFileRoute } from '@tanstack/react-router';

export const Route = createFileRoute('/_app/settings/general')({
  // keep-alive：切走时保留页面状态，切回不重建
  staticData: { keepAlive: true },
  component: GeneralSettingsPage,
});

function GeneralSettingsPage() {
  return (
    <div>
      <h1 className="text-xl font-semibold">通用设置</h1>
      <p className="mt-4 text-sm text-ink-soft">
        这里是通用设置占位内容，待接入后端。
      </p>
    </div>
  );
}
