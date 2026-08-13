import { createRouter, RouterProvider } from '@tanstack/react-router';
import ReactDOM from 'react-dom/client';
import { Toaster } from './components/ui/sonner';
import { querySerialize } from './lib/url';
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
  // search 纯字符串解析：不做 JSON.parse（否则纯数字搜索词会被转 number，撞 z.string() 校验），
  // URLSearchParams 语义下裸 `;`/`=` 在值内正确解析
  parseSearch: (searchStr) => {
    const s = searchStr[0] === '?' ? searchStr.substring(1) : searchStr;
    const query: Record<string, string> = {};
    for (const [key, value] of new URLSearchParams(s)) {
      query[key] = value;
    }
    return query;
  },
  // 地址栏 search 严格编码（URLSearchParams 默认行为，与业界一致）
  stringifySearch: (search) => {
    const s = querySerialize(search as Record<string, unknown>);
    return s ? `?${s}` : '';
  },
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
