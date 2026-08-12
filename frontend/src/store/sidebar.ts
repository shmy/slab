import { createStore } from '@tanstack/react-store';

const STORAGE_KEY = 'sidebar.collapsed';

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'true';
  } catch {
    return false;
  }
}

const initial = readCollapsed();

export const sidebarStore = createStore<{ collapsed: boolean }>({
  collapsed: initial,
});

export function setCollapsed(collapsed: boolean) {
  try {
    localStorage.setItem(STORAGE_KEY, String(collapsed));
  } catch {
    // 隐身模式 / 隐私浏览下存储受限，忽略（会话内仍生效）
  }
  sidebarStore.setState((s) => ({ ...s, collapsed }));
}
