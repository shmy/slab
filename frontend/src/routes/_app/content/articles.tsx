import { createFileRoute } from '@tanstack/react-router';

export const Route = createFileRoute('/_app/content/articles')({
  // keep-alive：切走时保留页面状态，切回不重建
  staticData: { keepAlive: true },
  component: ArticlesPage,
});

function ArticlesPage() {
  return (
    <div>
      <h1 className="text-xl font-semibold">文章管理</h1>
      <p className="mt-4 text-sm text-ink-soft">
        这里是文章管理占位内容，待接入后端。
      </p>
    </div>
  );
}
