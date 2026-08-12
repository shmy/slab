import { createRouter, RouterProvider } from '@tanstack/react-router';
import ReactDOM from 'react-dom/client';
import { Toaster } from './components/ui/sonner';
import { routeTree } from './routeTree.gen';
import './index.css';
// 副作用导入：任何页面加载时都先应用持久化的主题/字体设置（登录页等不引用 ThemeToggle 的页面也需要）
import './store/theme';
import './store/fontSize';

const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
  defaultStaleTime: 5000,
  scrollRestoration: true,
});

const rootEl = document.getElementById('root');
if (rootEl) {
  const root = ReactDOM.createRoot(rootEl);
  root.render(
    <>
      <RouterProvider router={router} />
      <Toaster richColors position="top-center" />
    </>,
  );
}
