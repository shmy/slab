import { createStore } from '@tanstack/react-store';

const STORAGE_KEY = 'theme.mode';

export type ThemeMode = 'light' | 'dark' | 'system';

function readMode(): ThemeMode {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw === 'light' || raw === 'dark' || raw === 'system' ? raw : 'system';
}

/** 将模式写到 <html data-theme>：dark 强制深色，其余情况交给 CSS 媒体查询 */
function apply(mode: ThemeMode) {
  const root = document.documentElement;
  if (mode === 'dark') {
    root.dataset.theme = 'dark';
  } else if (mode === 'light') {
    root.dataset.theme = 'light';
  } else {
    delete root.dataset.theme;
  }
}

// 模块加载即应用，避免主题闪烁
const initial = readMode();
apply(initial);

export const themeStore = createStore<{ mode: ThemeMode }>({ mode: initial });

export function setTheme(mode: ThemeMode) {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // 隐身模式 / 隐私浏览下存储受限，忽略（会话内仍生效）
  }
  apply(mode);
  themeStore.setState((s) => ({ ...s, mode }));
}
